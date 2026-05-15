//! Room management.

use axum::extract::{Path, State};

use chrono::{DateTime, Utc};
use garde::Validate;
use mogidb_model::{
    event::FormatSelectionMode,
    guild::Guild,
    room::{Room, RoomOptions, RoomOverrides},
};
use serde::Deserialize;
use sqlx::FromRow;

use crate::{AppState, error::Error, json::Json, validate::Valid};

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

#[derive(FromRow)]
struct RoomQuery {
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

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(Room {
        id: request.room_id,
        name: request.name,
        enabled: request.enabled,
        settings: overrides,
        guild: Guild::try_from(guild).map_err(Error::new)?,
        created_at: now,
        updated_at: now,
    }))
}

/// Shows an existing room.
pub async fn show(
    Path((guild_id, channel_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> Result<Json<Room>, Error> {
    // Check if guild exists
    let (count,) =
        sqlx::query_as::<_, (i32,)>("SELECT COUNT(*) FROM guild WHERE discord_guild_id = $1")
            .bind(guild_id)
            .fetch_one(&state.db)
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
    .fetch_optional(&state.db)
    .await
    .map_err(Error::new)?;
    let Some(row) = row else {
        return Err(Error::not_found(format_args!(
            "room {} not found",
            channel_id
        )));
    };

    Ok(Json(Room::try_from(row).map_err(Error::new)?))
}
