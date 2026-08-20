//! Event routes.

use axum::extract::{Path, State};

use chrono::Utc;
use garde::Validate;

use mogidb_model::{
    error::ApiError,
    event::{Event, EventStatus},
};

use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState,
    error::{Error, OptionExt as _},
    event::{EventEntity, get_event},
    json::Json,
    room::get_room,
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
    let mut room = get_room(guild_id, room_id, &mut *tx)
        .await?
        .ok_or_not_found()?;
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
    .await?
    .ok_or_not_found()?;

    // Preload all the things.
    if let Some(room) = event.room.as_mut() {
        room.preload_formats_with_servers(&state.server_tracker, &mut conn)
            .await?;
        room.guild
            .as_mut()
            .expect("guild preloaded")
            .preload_servers(&state.server_tracker, &mut conn)
            .await?;
    }

    event.preload_participants(&mut conn).await?;

    Ok(Json(Event::try_from(event)?))
}
