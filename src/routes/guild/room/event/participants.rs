//! Event participant routes.

use std::collections::HashSet;

use axum::extract::{Path, State};

use chrono::Utc;
use garde::Validate;
use mogidb_model::{
    error::ApiError,
    event::{Event, EventParticipant, EventStatus, format::TeamMode},
    request::TeamBalanceMode,
    response::JoinEventResponse,
};
use rand::seq::SliceRandom;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState, deserialize_some,
    error::{Error, ErrorKind, ResultExt as _},
    event::{get_event, get_participants},
    json::Json,
    routes::guild::room::event::aggregate_event,
    user::get_user,
    validate::Valid,
};

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
pub struct JoinEventRequest {
    /// The id of the joining player.
    #[garde(skip)]
    pub user_id: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
#[serde(default)]
pub struct AssignTeamsRequest {
    /// The list of players to assign teams for, by id.
    ///
    /// When ommited, the API will consider all players in the list of
    /// participants.
    ///
    /// If the list is empty, this will unassign all player team numbers.
    #[garde(skip)]
    #[serde(deserialize_with = "deserialize_some")]
    pub players: Option<Vec<String>>,
    /// The method for team balancing.
    ///
    /// Defaults to [`TeamBalanceMode::Shuffle`].
    #[garde(skip)]
    pub balance_mode: TeamBalanceMode,
}

impl Default for AssignTeamsRequest {
    fn default() -> Self {
        Self {
            players: None,
            balance_mode: TeamBalanceMode::Shuffle,
        }
    }
}

/// Lists the participants of an event.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/rooms/{room_id}/events/{event_id}/participants",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("event_id" = String, Path, description = "Id of the event"),
    ),
    responses(
        (status = OK, description = "The participants of the event", body = Vec<EventParticipant>),
        (status = NOT_FOUND, description = "Guild, room or event not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn list(
    Path((guild_id, room_id, event_id)): Path<(i64, i64, String)>,
    State(state): State<AppState>,
) -> Result<Json<Vec<EventParticipant>>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    // Get the relevant event
    let event = get_event(
        guild_id,
        room_id,
        &event_id,
        &state.server_tracker,
        &mut conn,
    )
    .await?;

    let participants = get_participants(event.id, &mut conn)
        .await?
        .into_iter()
        .map(EventParticipant::try_from)
        .collect::<Result<Vec<_>, Error>>()?;

    Ok(Json(participants))
}

/// Adds a participant to the event.
///
/// If this join brings the event to the room's `players_required`, the event
/// is automatically advanced to `Ongoing` and the response's `started` field
/// is `true`. This transition happens exactly once per event, even under
/// concurrent joins.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/rooms/{room_id}/events/{event_id}/participants",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("event_id" = String, Path, description = "Id of the event"),
    ),
    request_body = JoinEventRequest,
    responses(
        (status = OK, description = "The user joined the event", body = JoinEventResponse),
        (status = BAD_REQUEST, description = "Invalid request, or the user does not exist", body = ApiError),
        (status = NOT_FOUND, description = "Guild, room or event not found", body = ApiError),
        (status = CONFLICT, description = "The user is already in the event, the event is full, or the event has concluded", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn join(
    Path((guild_id, room_id, event_id)): Path<(i64, i64, String)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<JoinEventRequest>>,
) -> Result<Json<JoinEventResponse>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;
    let now = Utc::now();

    // Get the relevant event
    let mut event = get_event(
        guild_id,
        room_id,
        &event_id,
        &state.server_tracker,
        &mut *tx,
    )
    .await?;
    aggregate_event(&mut event, &state.server_tracker, &mut *tx).await?;

    // Fetch settings state
    let room = event.room.as_ref().expect("room to be preloaded");
    let guild = room.guild.as_ref().expect("guild to be preloaded");

    let settings = guild.settings.clone().merge(room.overrides.clone());

    // Find the relevant user
    let user = get_user(&request.user_id, &mut *tx).await.or_none()?;
    let Some(user) = user else {
        return Err(ErrorKind::NoSuchUser(request.user_id).into());
    };

    // Do not allow joins of rejected events, or events that are past
    // EventState::Ongoing
    if !event.is_roster_mutable() {
        return Err(ErrorKind::EventConcluded.into());
    }

    // First, we insert the new user with a conditional insert
    let res = sqlx::query(
        r#"
        INSERT INTO event_participant (inserted_at, updated_at, user_id, event_id)
        SELECT $1, $1, $2, $3
        WHERE (SELECT COUNT(*) FROM event_participant WHERE event_id = $3) < $4
        "#,
    )
    .bind(now)
    .bind(user.id)
    .bind(event.id)
    .bind(settings.max_players)
    .execute(&mut *tx)
    .await;

    // Check if the user already is in the queue
    match res {
        Ok(info) if info.rows_affected() > 0 => {}
        // Do not allow joins if the event is full
        Ok(_info) => return Err(ErrorKind::EventFull.into()),
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            // The user already exists!
            return Err(ErrorKind::UserInEvent(request.user_id).into());
        }
        Err(err) => return Err(Error::new(err)),
    };

    // Try to update event to start it up, if we meet the required players.
    let res = sqlx::query(
        r#"
        UPDATE event
        SET updated_at = $1, status = $2
        WHERE
            id = $3
            AND status = 0
            AND (SELECT COUNT(*) FROM event_participant WHERE event_id = event.id)
                >= $4
        "#,
    )
    .bind(now)
    .bind(u8::from(EventStatus::Ongoing))
    .bind(event.id)
    .bind(settings.players_required)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    // Preload participant list (includes the just-inserted row)
    event.preload_participants(&mut *tx).await?;

    tx.commit().await.map_err(Error::new)?;

    let started = if res.rows_affected() > 0 {
        // The event was started, so update stale data
        event.status = EventStatus::Ongoing;
        event.updated_at = now;
        true
    } else {
        false
    };

    Ok(Json(JoinEventResponse {
        event: Event::try_from(event)?,
        started,
    }))
}

/// Removes a participant from the event.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/rooms/{room_id}/events/{event_id}/participants/{user_id}",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("event_id" = String, Path, description = "Id of the event"),
        ("user_id" = String, Path, description = "Short ID of the leaving user"),
    ),
    responses(
        (status = OK, description = "The user left the event; returns the event", body = Event),
        (status = BAD_REQUEST, description = "Invalid request, the user does not exist, or the user is not in the event", body = ApiError),
        (status = NOT_FOUND, description = "Guild, room or event not found", body = ApiError),
        (status = CONFLICT, description = "The event has concluded, or its roster is locked", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn leave(
    Path((guild_id, room_id, event_id, user_id)): Path<(i64, i64, String, String)>,
    State(state): State<AppState>,
) -> Result<Json<Event>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    // Get the relevant event
    let mut event = get_event(
        guild_id,
        room_id,
        &event_id,
        &state.server_tracker,
        &mut *tx,
    )
    .await?;
    aggregate_event(&mut event, &state.server_tracker, &mut *tx).await?;

    // Find the relevant user
    let user = get_user(&user_id, &mut *tx).await.or_none()?;
    let Some(user) = user else {
        return Err(ErrorKind::NoSuchUser(user_id).into());
    };

    // Do not allow leaves of rejected events, or events that are past
    // EventState::Ongoing
    if !event.is_roster_mutable() {
        return Err(ErrorKind::EventConcluded.into());
    }

    // Apply change
    let res = sqlx::query(
        r#"
        DELETE FROM event_participant
        WHERE event_id = $1 AND user_id = $2
        "#,
    )
    .bind(event.id)
    .bind(user.id)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    // Preload only after editing
    event.preload_participants(&mut *tx).await?;

    tx.commit().await.map_err(Error::new)?;

    if res.rows_affected() > 0 {
        Ok(Json(Event::try_from(event)?))
    } else {
        Err(ErrorKind::NotPlaying(user_id).into())
    }
}

/// Assigns teams atomically to the event.
///
/// An optional [`AssignTeamsRequest::players`] list may be passed to assign
/// teams only to those players. When teams are assigned, all other players get
/// assigned a `null` `team_number`.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/rooms/{room_id}/events/{event_id}/participants/teams~assign",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("event_id" = String, Path, description = "Id of the event"),
    ),
    request_body = AssignTeamsRequest,
    responses(
        (status = OK, description = "The event with teams assigned, or unassigned when the players list is empty", body = Event),
        (status = BAD_REQUEST, description = "Invalid request, no format assigned to the event, or a listed user does not exist or is not playing", body = ApiError),
        (status = NOT_FOUND, description = "Guild, room or event not found", body = ApiError),
        (status = CONFLICT, description = "The event was rejected, or is not currently ongoing", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn assign_teams(
    Path((guild_id, room_id, event_id)): Path<(i64, i64, String)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<AssignTeamsRequest>>,
) -> Result<Json<Event>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;
    let now = Utc::now();

    // Get the relevant event
    let mut event = get_event(
        guild_id,
        room_id,
        &event_id,
        &state.server_tracker,
        &mut *tx,
    )
    .await?;
    aggregate_event(&mut event, &state.server_tracker, &mut *tx).await?;

    // If the event is rejected, no teams can be changed
    // This shouldn't happen often so the serialization is nothing to worry
    // about.
    if event.rejected {
        return Err(ErrorKind::EventRejected.into());
    }

    // An event can only have teams shuffled if it is ongoing...
    if event.status != EventStatus::Ongoing {
        return Err(ErrorKind::EventTeamsUnassignable.into());
    }

    // An event cannot have teams automatically shuffled without an assigned
    // format.
    let Some(format) = event.format.clone() else {
        return Err(ErrorKind::NoFormatAssigned.into());
    };

    // Clear participant teams
    // Also check if we even need to do fucking anything
    let res = sqlx::query(
        r#"
        UPDATE event_participant
        SET
            updated_at = $1,
            team_number = NULL
        WHERE
            event_id = $2
        "#,
    )
    .bind(now)
    .bind(event.id)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;
    if res.rows_affected() == 0 {
        // You are a fool...
        event.participants = Some(Vec::new());
        return Ok(Json(Event::try_from(event)?));
    }

    // Fetch players
    let participants = event.preload_participants(&mut *tx).await?;
    if let Some(players) = request.players {
        // Resolve IDs
        let mut user_ids = HashSet::<i32>::new();
        for short_id in players {
            let res = sqlx::query_as::<_, (i32,)>("SELECT id FROM user WHERE short_id = $1")
                .bind(&short_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(Error::new)?;

            if let Some((player_id,)) = res {
                user_ids.insert(player_id);

                // Check if user is playing
                let is_playing = participants
                    .iter()
                    .filter_map(|p| p.user.as_ref().map(|u| &u.short_id))
                    .any(|other_short_id| short_id == *other_short_id);
                if !is_playing {
                    return Err(ErrorKind::NotPlaying(short_id).into());
                }
            } else {
                return Err(ErrorKind::NoSuchUser(short_id).into());
            }
        }

        // Filter participants
        participants.retain_mut(|p| user_ids.contains(&p.user_id));
    }

    match request.balance_mode {
        TeamBalanceMode::Shuffle if format.team_mode == TeamMode::FreeForAll => {
            // We don't really need to shuffle to give everyone a team
            for (i, participant) in participants.iter_mut().enumerate() {
                participant.team_number = Some(i as i32);
            }
        }
        TeamBalanceMode::Shuffle => {
            // Using the list of participants, shuffle.
            let mut rng = rand::rng();
            participants.shuffle(&mut rng);

            // Divide up round-robin
            let number_of_teams = format.team_mode.team_count();
            for (i, participant) in participants.iter_mut().enumerate() {
                participant.team_number = Some((i % number_of_teams) as i32);
            }
        }
    }

    // God's work has been done, push back to database
    for participant in participants.iter() {
        sqlx::query("UPDATE event_participant SET team_number = $1 WHERE id = $2")
            .bind(participant.team_number)
            .bind(participant.id)
            .execute(&mut *tx)
            .await
            .map_err(Error::new)?;
    }

    tx.commit().await.map_err(Error::new)?;

    // Push back
    Ok(Json(Event::try_from(event)?))
}
