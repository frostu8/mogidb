//! Room management.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use chrono::{DateTime, Utc};
use garde::Validate;
use mogidb_model::{
    event::FormatSelectionMode,
    guild::Guild,
    room::{Room, RoomOptions, RoomOverrides},
};
use serde::Deserialize;
use sqlx::FromRow;

use crate::{
    AppState, error::Error, json::Json, routes::guild::server::preload_servers, validate::Valid,
};

#[derive(Debug, Deserialize, Validate)]
#[garde(context(AppState as state))]
pub struct CreateRoomRequest {
    #[garde(skip)]
    pub room_id: i64,
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(skip)]
    #[serde(default)]
    pub enabled: bool,
    #[serde(flatten)]
    #[garde(dive)]
    pub settings: super::UpdateRoomSettings,
}

#[derive(Default, Debug, Deserialize, Validate)]
#[garde(context(AppState as state))]
#[serde(default)]
pub struct UpdateRoomRequest {
    #[garde(length(min = 1))]
    pub name: Option<String>,
    #[garde(skip)]
    pub enabled: Option<bool>,
    #[garde(range(min = 1))]
    pub players_required: Option<Option<u32>>,
    #[garde(skip)]
    pub format_selection_mode: Option<Option<FormatSelectionMode>>,
    #[garde(range(min = 1))]
    pub votes_required: Option<Option<u32>>,
    #[garde(range(min = 0))]
    pub decay_after: Option<Option<u32>>,
    #[garde(range(min = 0))]
    pub inactivity_warning_after: Option<Option<u32>>,
    #[garde(range(min = 0))]
    pub inactivity_drop_after: Option<Option<u32>>,
}

impl UpdateRoomRequest {
    pub fn update(self, other: RoomOverrides) -> RoomOverrides {
        RoomOverrides {
            players_required: self.players_required.unwrap_or(other.players_required),
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

#[derive(FromRow)]
struct RoomQuery {
    pub id: i32,
    pub discord_channel_id: i64,
    pub name: String,
    pub enabled: bool,
    pub overrides: String,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Guild fields
    pub discord_guild_id: i64,
    pub settings: String,
    pub guild_inserted_at: DateTime<Utc>,
    pub guild_updated_at: DateTime<Utc>,
}

impl TryFrom<RoomQuery> for Room {
    type Error = serde_json::Error;

    fn try_from(value: RoomQuery) -> Result<Self, Self::Error> {
        Ok(Room {
            id: value.discord_channel_id,
            name: value.name,
            enabled: value.enabled,
            settings: serde_json::from_str::<RoomOverrides>(&value.overrides)?,
            created_at: value.inserted_at,
            updated_at: value.updated_at,

            guild: Guild {
                id: value.discord_guild_id,
                settings: serde_json::from_str::<RoomOptions>(&value.settings)?,
                servers: None,
                created_at: value.guild_inserted_at,
                updated_at: value.guild_updated_at,
            },
        })
    }
}

/// Creates a new room for a discord channel.
pub async fn create(
    Path((guild_id,)): Path<(i64,)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<CreateRoomRequest>>,
) -> Result<Json<Room>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    let now = Utc::now();

    // Get guild
    let guild = sqlx::query_as::<_, super::GuildQuery>(
        r#"
        SELECT *
        FROM guild
        WHERE discord_guild_id = $1
        "#,
    )
    .bind(guild_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(Error::new)?;
    let Some(guild) = guild else {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id
        )));
    };

    // Create room overrides
    let overrides = RoomOverrides::from(request.settings);

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

    let mut guild = Guild::try_from(guild).map_err(Error::new)?;
    preload_servers(&mut guild, &state.server_tracker, &mut *tx).await?;

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(Room {
        id: request.room_id,
        name: request.name,
        enabled: request.enabled,
        settings: overrides,
        guild,
        created_at: now,
        updated_at: now,
    }))
}

/// Shows an existing room.
pub async fn show(
    Path((guild_id, channel_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> Result<Json<Room>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    // Check if guild exists
    let (count,) =
        sqlx::query_as::<_, (i32,)>("SELECT COUNT(*) FROM guild WHERE discord_guild_id = $1")
            .bind(guild_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(Error::new)?;
    if count <= 0 {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id
        )));
    }

    // Get guild and room
    let row = sqlx::query_as::<_, RoomQuery>(
        r#"
        SELECT
            r.*,
            g.settings,
            g.discord_guild_id,
            g.inserted_at AS guild_inserted_at,
            g.updated_at AS guild_updated_at
        FROM room r, guild g
        WHERE
            discord_guild_id = $1
            AND discord_channel_id  = $2
            AND r.parent_id = g.id
        "#,
    )
    .bind(guild_id)
    .bind(channel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)?;
    let Some(room) = row else {
        return Err(Error::not_found(format_args!(
            "room {} not found",
            channel_id
        )));
    };

    let mut room = Room::try_from(room).map_err(Error::new)?;
    preload_servers(&mut room.guild, &state.server_tracker, &mut *conn).await?;

    Ok(Json(room))
}

/// Updates an existing room.
pub async fn update(
    Path((guild_id, channel_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<UpdateRoomRequest>>,
) -> Result<Json<Room>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    let now = Utc::now();

    // Check if guild exists
    let (count,) =
        sqlx::query_as::<_, (i32,)>("SELECT COUNT(*) FROM guild WHERE discord_guild_id = $1")
            .bind(guild_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(Error::new)?;
    if count <= 0 {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id
        )));
    }

    // Get guild and room
    let row = sqlx::query_as::<_, RoomQuery>(
        r#"
        SELECT
            r.*,
            g.settings,
            g.discord_guild_id,
            g.inserted_at AS guild_inserted_at,
            g.updated_at AS guild_updated_at
        FROM room r, guild g
        WHERE
            discord_guild_id = $1
            AND discord_channel_id  = $2
            AND r.parent_id = g.id
        "#,
    )
    .bind(guild_id)
    .bind(channel_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(Error::new)?;
    let Some(room) = row else {
        return Err(Error::not_found(format_args!(
            "room {} not found",
            channel_id
        )));
    };

    // Get room
    let room_id = room.id;
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
pub async fn delete(
    Path((guild_id, room_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> Result<StatusCode, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    // Get guild
    let guild = sqlx::query_as::<_, super::GuildQuery>(
        r#"
        SELECT *
        FROM guild
        WHERE discord_guild_id = $1
        "#,
    )
    .bind(guild_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(Error::new)?;
    let Some(guild) = guild else {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id
        )));
    };

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
        Err(Error::not_found(format_args!("room {} not found", room_id)))
    }
}
