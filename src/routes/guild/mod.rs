//! Application routes for the guild.

pub mod room;
pub mod server;

use axum::extract::{Path, State};

use chrono::{DateTime, Utc};
use garde::Validate;

use mogidb_model::{
    event::FormatSelectionMode,
    guild::Guild,
    room::{RoomOptions, RoomOptionsOverrides},
};

use serde::Deserialize;
use sqlx::FromRow;

use crate::{
    AppState, error::Error, json::Json, routes::guild::server::preload_servers, validate::Valid,
};

#[derive(Debug, Deserialize, Validate)]
#[garde(context(AppState as state))]
pub struct CreateGuildRequest {
    #[garde(skip)]
    pub guild_id: i64,
    #[serde(flatten)]
    #[garde(dive)]
    pub settings: UpdateRoomSettings,
}

#[derive(Debug, Deserialize, Validate)]
#[garde(context(AppState as state))]
pub struct UpdateGuildRequest {
    #[serde(flatten)]
    #[garde(dive)]
    pub settings: UpdateRoomSettings,
}

#[derive(Default, Debug, Deserialize, Validate)]
#[garde(context(AppState as state))]
#[serde(default)]
pub struct UpdateRoomSettings {
    #[garde(range(min = 1))]
    pub players_required: Option<u32>,
    #[garde(skip)]
    pub format_selection_mode: Option<FormatSelectionMode>,
    #[garde(range(min = 1))]
    pub votes_required: Option<u32>,
    #[garde(range(min = 0))]
    pub decay_after: Option<u32>,
    #[garde(range(min = 0))]
    pub inactivity_warning_after: Option<u32>,
    #[garde(range(min = 0))]
    pub inactivity_drop_after: Option<u32>,
}

impl From<UpdateRoomSettings> for RoomOptionsOverrides {
    fn from(value: UpdateRoomSettings) -> Self {
        RoomOptionsOverrides {
            players_required: value.players_required,
            format_selection_mode: value.format_selection_mode,
            votes_required: value.votes_required,
            decay_after: value.decay_after,
            inactivity_warning_after: value.inactivity_warning_after,
            inactivity_drop_after: value.inactivity_drop_after,
        }
    }
}

#[derive(FromRow)]
struct GuildQuery {
    pub id: i32,
    pub discord_guild_id: i64,
    pub settings: String,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<GuildQuery> for Guild {
    type Error = serde_json::Error;

    fn try_from(value: GuildQuery) -> Result<Self, Self::Error> {
        Ok(Guild {
            id: value.discord_guild_id,
            settings: serde_json::from_str::<RoomOptions>(&value.settings)?,
            servers: None,
            created_at: value.inserted_at,
            updated_at: value.updated_at,
        })
    }
}

/// Creates a new guild.
#[axum::debug_handler]
pub async fn create(
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<CreateGuildRequest>>,
) -> Result<Json<Guild>, Error> {
    let now = Utc::now();

    // Get the settings for the new guild
    let settings = RoomOptionsOverrides::from(request.settings);
    let settings = RoomOptions::default().merge(settings);

    // Serialize settings
    let serialized = serde_json::to_string(&settings).map_err(Error::new)?;

    // Add new guild
    let res = sqlx::query(
        r#"
        INSERT INTO guild (discord_guild_id, settings, inserted_at, updated_at)
        VALUES
        ($1, $3, $2, $2)
        "#,
    )
    .bind(request.guild_id)
    .bind(now)
    .bind(serialized)
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => Ok(Json(Guild {
            id: request.guild_id,
            created_at: now,
            updated_at: now,
            // The guild is new, so it should not have any servers.
            servers: Some(vec![]),
            settings: settings,
        })),
        // Guild already exists
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => Err(Error::exists(
            format_args!("guild {} already exists", request.guild_id),
        )),
        Err(err) => Err(Error::new(err)),
    }
}

/// Fetches a guild.
pub async fn show(
    Path((guild_id,)): Path<(i64,)>,
    State(state): State<AppState>,
) -> Result<Json<Guild>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    // Fetch the guild
    let res = sqlx::query_as::<_, GuildQuery>(
        r#"
        SELECT *
        FROM guild
        WHERE discord_guild_id = $1
        "#,
    )
    .bind(guild_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)?;

    match res {
        Some(row) => {
            let mut guild: Guild = row.try_into().map_err(Error::new)?;
            preload_servers(&mut guild, &state.server_tracker, &mut *conn).await?;
            Ok(Json(guild))
        }
        None => Err(not_found(guild_id)),
    }
}

/// Updates guild details.
pub async fn update(
    Path((guild_id,)): Path<(i64,)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<UpdateGuildRequest>>,
) -> Result<Json<Guild>, Error> {
    let now = Utc::now();

    let mut tx = state.db.begin().await.map_err(Error::new)?;

    // Fetch the guild
    let res = sqlx::query_as::<_, GuildQuery>(
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

    let Some(row) = res else {
        return Err(not_found(guild_id));
    };

    // Get guild room settings
    let guild_db_id = row.id;
    let guild: Guild = row.try_into().map_err(Error::new)?;

    let new_settings = guild.settings.merge(request.settings.into());
    // Serialize settings
    let serialized = serde_json::to_string(&new_settings).map_err(Error::new)?;

    // Set guild settings
    sqlx::query(
        r#"
        UPDATE guild
        SET
            settings = $3,
            updated_at = $2
        WHERE id = $1
        "#,
    )
    .bind(guild_db_id)
    .bind(now)
    .bind(serialized)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    let mut guild = Guild {
        id: guild.id,
        servers: None,
        created_at: guild.created_at,
        updated_at: now,
        settings: new_settings,
    };

    preload_servers(&mut guild, &state.server_tracker, &mut *tx).await?;

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(guild))
}

fn not_found(guild_id: i64) -> Error {
    Error::not_found(format_args!("guild {} not found", guild_id,))
}
