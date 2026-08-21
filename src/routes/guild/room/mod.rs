//! Room management.

pub mod event;
pub mod format;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use chrono::Utc;
use garde::Validate;
use mogidb_model::{
    error::ApiError,
    event::FormatSelectionMode,
    guild::Guild,
    room::{Room, RoomOptionsOverrides},
};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState, deserialize_some,
    error::{Error, NotFound},
    guild::get_guild,
    json::Json,
    room::get_room,
    validate::Valid,
};

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
pub struct CreateRoomRequest {
    /// The ID of the Discord channel associated with the room.
    #[garde(skip)]
    pub room_id: i64,
    /// The name of the room.
    #[garde(length(min = 1))]
    #[schema(min_length = 1)]
    pub name: String,
    /// If the room is enabled.
    #[garde(skip)]
    #[serde(default)]
    pub enabled: bool,
    /// The room settings.
    #[serde(flatten)]
    #[garde(dive)]
    #[schema(inline)]
    pub settings: super::UpdateRoomSettings,
}

#[derive(Default, Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
#[serde(default)]
pub struct UpdateRoomRequest {
    /// The name of the room.
    ///
    /// This is always the channel name, and can be used to update the channel
    /// name when it changes on Discord's end.
    #[garde(length(min = 1))]
    #[schema(min_length = 1)]
    pub name: Option<String>,
    /// If the room is enabled.
    #[garde(skip)]
    pub enabled: Option<bool>,
    /// The amount of players needed to start an event.
    #[garde(range(min = 1))]
    #[schema(minimum = 1)]
    #[serde(deserialize_with = "deserialize_some")]
    pub players_required: Option<Option<u32>>,
    /// The maximum amount of players allowed in an event.
    #[garde(range(min = 1))]
    #[schema(minimum = 1)]
    #[serde(deserialize_with = "deserialize_some")]
    pub max_players: Option<Option<u32>>,
    /// The mode for format selection.
    #[garde(skip)]
    #[serde(deserialize_with = "deserialize_some")]
    pub format_selection_mode: Option<Option<FormatSelectionMode>>,
    /// The amount of votes needed for a format to be selected.
    #[garde(range(min = 1))]
    #[schema(minimum = 1)]
    #[serde(deserialize_with = "deserialize_some")]
    pub votes_required: Option<Option<u32>>,
    /// The amount of time it takes for events to decay, in seconds.
    #[garde(range(min = 0))]
    #[serde(deserialize_with = "deserialize_some")]
    pub decay_after: Option<Option<u32>>,
    /// The amount of time before the bot warns someone for inactivity, in seconds.
    #[garde(range(min = 0))]
    #[serde(deserialize_with = "deserialize_some")]
    pub inactivity_warning_after: Option<Option<u32>>,
    /// The amount of time before the bot drops someone for inactivity, in seconds.
    #[garde(range(min = 0))]
    #[serde(deserialize_with = "deserialize_some")]
    pub inactivity_drop_after: Option<Option<u32>>,
}

impl UpdateRoomRequest {
    pub fn update(self, other: RoomOptionsOverrides) -> RoomOptionsOverrides {
        RoomOptionsOverrides {
            players_required: self.players_required.unwrap_or(other.players_required),
            max_players: self.max_players.unwrap_or(other.max_players),
            format_selection_mode: self
                .format_selection_mode
                .unwrap_or(other.format_selection_mode),
            votes_required: self.votes_required.unwrap_or(other.votes_required),
            decay_after: self.decay_after.unwrap_or(other.decay_after),
            inactivity_warning_after: self
                .inactivity_warning_after
                .unwrap_or(other.inactivity_warning_after),
            inactivity_drop_after: self
                .inactivity_drop_after
                .unwrap_or(other.inactivity_drop_after),
        }
    }
}

/// Creates a new room for a discord channel.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/rooms",
    tag = "room",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
    ),
    request_body = CreateRoomRequest,
    responses(
        (status = OK, description = "The newly created room", body = Room),
        (status = BAD_REQUEST, description = "Invalid request", body = ApiError),
        (status = NOT_FOUND, description = "Guild not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn create(
    Path((guild_id,)): Path<(i64,)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<CreateRoomRequest>>,
) -> Result<Json<Room>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;
    let now = Utc::now();

    // Get guild
    let mut guild = get_guild(guild_id, &mut *tx).await?;
    guild
        .preload_servers(&state.server_tracker, &mut *tx)
        .await?;

    // Create room overrides
    let overrides = RoomOptionsOverrides::from(request.settings);

    // Serialize
    let serialized = serde_json::to_string(&overrides).map_err(Error::new)?;

    // Add new room
    sqlx::query(
        r#"
        INSERT INTO room (
            discord_channel_id, parent_id, name, enabled, overrides,
            inserted_at, updated_at
        )
        VALUES ($1, $2, $4, $5, $6, $3, $3)
        "#,
    )
    .bind(request.room_id)
    .bind(guild.id)
    .bind(now)
    .bind(&request.name)
    .bind(request.enabled)
    .bind(serialized)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    let guild = Guild::try_from(guild).map_err(Error::new)?;
    let room = Room {
        id: request.room_id,
        name: request.name,
        enabled: request.enabled,
        settings: overrides,
        guild: Some(guild),
        formats: Some(vec![]),
        created_at: now,
        updated_at: now,
    };

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(room))
}

/// Shows an existing room.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/rooms/{room_id}",
    tag = "room",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
    ),
    responses(
        (status = OK, description = "The room", body = Room),
        (status = NOT_FOUND, description = "Guild or room not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn show(
    Path((guild_id, channel_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> Result<Json<Room>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    let mut room = get_room(guild_id, channel_id, &mut conn).await?;
    room.preload_formats_with_servers(&state.server_tracker, &mut conn)
        .await?;
    room.guild
        .as_mut()
        .expect("guild preloaded")
        .preload_servers(&state.server_tracker, &mut conn)
        .await?;

    let room = Room::try_from(room).map_err(Error::new)?;

    Ok(Json(room))
}

/// Updates an existing room.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/rooms/{room_id}",
    tag = "room",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
    ),
    request_body = UpdateRoomRequest,
    responses(
        (status = OK, description = "The updated room", body = Room),
        (status = BAD_REQUEST, description = "Invalid request", body = ApiError),
        (status = NOT_FOUND, description = "Guild or room not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn update(
    Path((guild_id, channel_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<UpdateRoomRequest>>,
) -> Result<Json<Room>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;
    let now = Utc::now();

    // Get room
    let mut room = get_room(guild_id, channel_id, &mut tx).await?;
    let room_id = room.id;
    room.preload_formats_with_servers(&state.server_tracker, &mut *tx)
        .await?;
    room.guild
        .as_mut()
        .expect("guild preloaded")
        .preload_servers(&state.server_tracker, &mut *tx)
        .await?;

    let mut room = Room::try_from(room).map_err(Error::new)?;

    // Update settings
    if let Some(enabled) = request.enabled {
        room.enabled = enabled;
    }

    if let Some(name) = request.name.clone() {
        room.name = name;
    }

    // Update overrides (help)
    room.settings = request.update(room.settings);
    room.updated_at = now;

    // Serialize
    let serialized = serde_json::to_string(&room.settings).map_err(Error::new)?;

    // Update in database
    sqlx::query(
        r#"
        UPDATE room
        SET
            enabled = $3,
            name = $4,
            overrides = $5,
            updated_at = $2
        WHERE
            id = $1
        "#,
    )
    .bind(room_id)
    .bind(now)
    .bind(room.enabled)
    .bind(&room.name)
    .bind(serialized)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(room))
}

/// Deletes a room.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/rooms/{room_id}",
    tag = "room",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
    ),
    responses(
        (status = NO_CONTENT, description = "The room was deleted"),
        (status = NOT_FOUND, description = "Guild or room not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn delete(
    Path((guild_id, room_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> Result<StatusCode, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    // Get guild
    let guild = get_guild(guild_id, &mut *tx).await?;

    // Delete room
    let res = sqlx::query(
        r#"
        DELETE FROM room
        WHERE discord_channel_id = $1 AND parent_id = $2
        "#,
    )
    .bind(room_id)
    .bind(guild.id)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    tx.commit().await.map_err(Error::new)?;

    if res.rows_affected() > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(NotFound::Room(room_id).into())
    }
}
