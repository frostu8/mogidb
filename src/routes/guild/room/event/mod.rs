//! Event routes.

pub mod participants;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use chrono::Utc;
use garde::Validate;

use mogidb_model::{
    error::ApiError,
    event::{Event, EventStatus},
};

use serde::Deserialize;
use sqlx::SqliteConnection;
use utoipa::ToSchema;

use crate::{
    AppState, deserialize_some,
    error::{Error, ErrorKind, NotFound, ResultExt as _},
    event::{EventEntity, get_active_event, get_event},
    guild::get_server,
    json::Json,
    room::{format::get_format, get_room},
    server::ServerTracker,
    short_id,
    validate::Valid,
};

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
pub struct CreateEventRequest {
    /// An optional title for the event.
    #[garde(length(min = 1, max = 255))]
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Default, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
#[serde(default)]
pub struct UpdateEventRequest {
    /// An optional title for the event.
    #[garde(length(min = 1, max = 255))]
    #[schema(nullable, min_length = 1, max_length = 255)]
    #[serde(deserialize_with = "deserialize_some")]
    pub title: Option<Option<String>>,
    /// The status of the event.
    #[serde(deserialize_with = "deserialize_some")]
    #[garde(skip)]
    pub status: Option<EventStatus>,
    /// The format of the event.
    #[serde(deserialize_with = "deserialize_some")]
    #[garde(skip)]
    pub format_id: Option<i32>,
    /// The server the event will be played on.
    #[serde(deserialize_with = "deserialize_some")]
    #[garde(skip)]
    pub server_id: Option<i32>,
}

/// Creates a new event in a given room.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/rooms/{room_id}/events",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
    ),
    request_body = CreateEventRequest,
    responses(
        (status = OK, description = "The newly created event", body = Event),
        (status = BAD_REQUEST, description = "Invalid request", body = ApiError),
        (status = NOT_FOUND, description = "Guild or room not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn create(
    Path((guild_id, room_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<CreateEventRequest>>,
) -> Result<Json<Event>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;
    let now = Utc::now();

    // Get guild and room
    // Cache room for later as we embed this
    let mut room = get_room(guild_id, room_id, &mut *tx).await?;
    let room_id = room.id;

    room.preload_formats_with_servers(&state.server_tracker, &mut *tx)
        .await?;
    room.guild
        .as_mut()
        .expect("guild preloaded")
        .preload_servers(&state.server_tracker, &mut *tx)
        .await?;

    let CreateEventRequest { title } = request;
    let status = EventStatus::LFG;

    // Create new event
    let mut event = short_id::allocate()
        .length(8)
        .insert(&mut *tx, async move |short_id, conn| {
            let (id,) = sqlx::query_as::<_, (i32,)>(
                r#"
                INSERT INTO event (inserted_at, updated_at, short_id, room_id, title, status)
                VALUES ($1, $1, $2, $3, $4, $5)
                RETURNING id
                "#,
            )
            .bind(now)
            .bind(short_id)
            .bind(room_id)
            .bind(title.as_ref())
            .bind(u8::from(status))
            .fetch_one(conn)
            .await?;
            Ok(EventEntity {
                id,
                short_id: short_id.to_owned(),
                room_id,
                title: title.clone(),
                status,
                rejected: false,
                format_id: None,
                server_id: None,
                inserted_at: now,
                updated_at: now,
                participants: Some(Vec::new()),
                room: None,
                format: None,
                server: None,
            })
        })
        .await?;
    // embed room in event
    event.room = Some(room);

    // When a new event gets added, we forcibly end the events before it.
    let res = sqlx::query(
        r#"
        UPDATE event
        SET status = $1, rejected = $2
        WHERE
            (status = $3 OR status = $4)
            AND room_id = $5
            AND id != $6
        "#,
    )
    .bind(u8::from(EventStatus::Concluded))
    .bind(true)
    .bind(u8::from(EventStatus::LFG))
    .bind(u8::from(EventStatus::Ongoing))
    .bind(room_id)
    .bind(event.id)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;
    if res.rows_affected() > 0 {
        tracing::debug!(
            "clearing {} events for a new active event",
            res.rows_affected()
        );
    }

    tx.commit().await.map_err(Error::new)?;

    // Return result
    Ok(Json(Event::try_from(event).map_err(Error::new)?))
}

/// Fetches an event from a room by ID.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/rooms/{room_id}/events/{event_id}",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("event_id" = String, Path, description = "Id of the event"),
    ),
    responses(
        (status = OK, description = "The event", body = Event),
        (status = NOT_FOUND, description = "Guild, room or event not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn show(
    Path((guild_id, room_id, event_id)): Path<(i64, i64, String)>,
    State(state): State<AppState>,
) -> Result<Json<Event>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    let mut event = get_event(
        guild_id,
        room_id,
        &event_id,
        &state.server_tracker,
        &mut conn,
    )
    .await?;
    aggregate_event(&mut event, &state.server_tracker, &mut conn).await?;

    Ok(Json(Event::try_from(event)?))
}

/// Fetches the currently active event from a room by ID.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/rooms/{room_id}/events/~current",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
    ),
    responses(
        (status = OK, description = "The event", body = Event),
        (status = NOT_FOUND, description = "Guild or room not found, or there are no active events in the room", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn show_current(
    Path((guild_id, room_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> Result<Json<Event>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    let event = get_active_event(guild_id, room_id, &state.server_tracker, &mut conn).await?;
    if let Some(mut event) = event {
        aggregate_event(&mut event, &state.server_tracker, &mut conn).await?;
        Ok(Json(Event::try_from(event)?))
    } else {
        Err(ErrorKind::NoActiveEvent.into())
    }
}

/// Updates an event.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/rooms/{room_id}/events/{event_id}",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("event_id" = String, Path, description = "Id of the event"),
    ),
    responses(
        (status = OK, description = "The event", body = Event),
        (status = BAD_REQUEST, description = "Failed to update the event", body = ApiError),
        (status = NOT_FOUND, description = "Guild, room or event not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn update(
    Path((guild_id, room_id, event_id)): Path<(i64, i64, String)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<UpdateEventRequest>>,
) -> Result<Json<Event>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;
    let now = Utc::now();

    // Fetch event
    let mut event = get_event(
        guild_id,
        room_id,
        &event_id,
        &state.server_tracker,
        &mut *tx,
    )
    .await?;
    aggregate_event(&mut event, &state.server_tracker, &mut *tx).await?;
    let guild_id = event.room.as_ref().expect("preloaded room").parent_id;

    // Apply the changes
    if let Some(title) = request.title {
        event.title = title;
    }
    if let Some(status) = request.status {
        // TODO: enforce forward-only status progression
        // TODO: do not allow scoring of rejected events
        event.status = status;
    }

    if let Some(format_id) = request.format_id {
        // Find the associated format before applying, to check if its in the
        // same room.
        let format = get_format(format_id, &mut *tx).await.or_none()?;
        if let Some(mut format) = format {
            if format.room_id != event.room_id {
                return Err(ErrorKind::NoSuchFormat(format_id).into());
            }

            // Works out, apply format
            event.format_id = Some(format_id);

            format
                .preload_servers(&state.server_tracker, &mut *tx)
                .await?;
            event.format = Some(format);
        } else {
            return Err(ErrorKind::NoSuchFormat(format_id).into());
        }
    } else if let Some(format_id) = event.format_id {
        // Preload original format
        let mut format = get_format(format_id, &mut *tx)
            .await
            .or_none()?
            .ok_or_else(|| Error::message("nonexistant format for event"))?;
        format
            .preload_servers(&state.server_tracker, &mut *tx)
            .await?;

        event.format = Some(format);
    }

    if let Some(server_id) = request.server_id {
        // Find the associated server before applying, to check if its in the
        // same guild.
        let server = get_server(server_id, &mut *tx).await.or_none()?;
        if let Some(mut server) = server {
            if server.guild_id != guild_id {
                return Err(ErrorKind::NoSuchServer(server_id).into());
            }

            // Works out, apply server
            event.server_id = Some(server_id);
            server.knock(&state.server_tracker).await?;
            event.server = Some(server);
        } else {
            return Err(ErrorKind::NoSuchServer(server_id).into());
        }
    } else if let Some(server_id) = event.server_id {
        // Preload original server
        let mut server = get_server(server_id, &mut *tx)
            .await
            .or_none()?
            .ok_or_else(|| Error::message("nonexistant server for event"))?;
        server.knock(&state.server_tracker).await?;
        event.server = Some(server);
    }

    // Persist the changes
    sqlx::query(
        r#"
        UPDATE event
        SET
            title = $3,
            status = $4,
            format_id = $5,
            server_id = $6,
            updated_at = $2
        WHERE
            id = $1
        "#,
    )
    .bind(event.id)
    .bind(now)
    .bind(&event.title)
    .bind(u8::from(event.status))
    .bind(event.format_id)
    .bind(event.server_id)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(Event::try_from(event).map_err(Error::new)?))
}

/// Deletes an event.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/rooms/{room_id}/events/{event_id}",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("event_id" = String, Path, description = "Id of the event"),
    ),
    responses(
        (status = NO_CONTENT, description = "The event was deleted", body = Event),
        (status = NOT_FOUND, description = "Guild, room or event not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn delete(
    Path((discord_guild_id, discord_channel_id, event_id)): Path<(i64, i64, String)>,
    State(state): State<AppState>,
) -> Result<StatusCode, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    // Get guild
    let room = get_room(discord_guild_id, discord_channel_id, &mut *tx).await?;

    // Delete event
    let res = sqlx::query(
        r#"
        DELETE FROM event
        WHERE short_id = $1 AND room_id = $2
        "#,
    )
    .bind(&event_id)
    .bind(room.id)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    tx.commit().await.map_err(Error::new)?;

    if res.rows_affected() > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(NotFound::Event(event_id).into())
    }
}

async fn aggregate_event(
    event: &mut EventEntity,
    tracker: &ServerTracker,
    conn: &mut SqliteConnection,
) -> Result<(), Error> {
    // Preload all the things.
    if let Some(room) = event.room.as_mut() {
        room.preload_formats_with_servers(&tracker, conn).await?;
        if let Some(guild) = room.guild.as_mut() {
            guild.preload_servers(&tracker, conn).await?;
        }
    }

    event.preload_participants(conn).await?;

    Ok(())
}
