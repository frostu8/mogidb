//! Application routes for the guild.

pub mod room;
pub mod server;
pub mod user;

use axum::extract::{Path, State};

use chrono::Utc;
use garde::Validate;

use mogidb_model::{
    error::ApiError,
    event::FormatSelectionMode,
    guild::Guild,
    room::{RoomOptions, RoomOptionsOverrides},
};

use serde::Deserialize;
use utoipa::ToSchema;

use crate::{AppState, error::Error, guild::GuildEntity, json::Json, validate::Valid};

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
pub struct CreateGuildRequest {
    #[garde(skip)]
    pub guild_id: i64,
    #[serde(flatten)]
    #[garde(dive)]
    #[schema(inline)]
    pub settings: UpdateRoomSettings,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
pub struct UpdateGuildRequest {
    #[serde(flatten)]
    #[garde(dive)]
    #[schema(inline)]
    pub settings: UpdateRoomSettings,
}

#[derive(Default, Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
#[serde(default)]
pub struct UpdateRoomSettings {
    /// The amount of players needed to start an event.
    #[garde(range(min = 1))]
    #[schema(minimum = 1)]
    pub players_required: Option<u32>,
    /// The mode for format selection.
    #[garde(skip)]
    pub format_selection_mode: Option<FormatSelectionMode>,
    /// The amount of votes needed for a format to be selected.
    #[garde(range(min = 1))]
    #[schema(minimum = 1)]
    pub votes_required: Option<u32>,
    /// The amount of time it takes for events to decay, in seconds.
    #[garde(range(min = 0))]
    pub decay_after: Option<u32>,
    /// The amount of time before the bot warns someone for inactivity, in seconds.
    #[garde(range(min = 0))]
    pub inactivity_warning_after: Option<u32>,
    /// The amount of time before the bot drops someone for inactivity, in seconds.
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

/// Creates a new guild.
#[utoipa::path(
    post,
    path = "/guilds",
    tag = "guild",
    request_body = CreateGuildRequest,
    responses(
        (status = OK, description = "The newly created guild", body = Guild),
        (status = BAD_REQUEST, description = "Invalid request or guild already exists", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
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
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}",
    tag = "guild",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
    ),
    responses(
        (status = OK, description = "The guild", body = Guild),
        (status = NOT_FOUND, description = "Guild not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn show(
    Path((guild_id,)): Path<(i64,)>,
    State(state): State<AppState>,
) -> Result<Json<Guild>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    // Fetch the guild
    let res = sqlx::query_as::<_, GuildEntity>(
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
        Some(mut row) => {
            row.preload_servers(&state.server_tracker, &mut *conn)
                .await?;
            let guild: Guild = row.try_into().map_err(Error::new)?;
            Ok(Json(guild))
        }
        None => Err(not_found(guild_id)),
    }
}

/// Updates guild details.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}",
    tag = "guild",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
    ),
    request_body = UpdateGuildRequest,
    responses(
        (status = OK, description = "The updated guild", body = Guild),
        (status = BAD_REQUEST, description = "Invalid request", body = ApiError),
        (status = NOT_FOUND, description = "Guild not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn update(
    Path((guild_id,)): Path<(i64,)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<UpdateGuildRequest>>,
) -> Result<Json<Guild>, Error> {
    let now = Utc::now();

    let mut tx = state.db.begin().await.map_err(Error::new)?;

    // Fetch the guild
    let res = sqlx::query_as::<_, GuildEntity>(
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

    let Some(mut row) = res else {
        return Err(not_found(guild_id));
    };
    row.preload_servers(&state.server_tracker, &mut *tx).await?;

    // Get guild room settings
    let guild_db_id = row.id;
    let mut guild: Guild = row.try_into().map_err(Error::new)?;

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

    guild.settings = new_settings;
    guild.updated_at = now;

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(guild))
}

fn not_found(guild_id: i64) -> Error {
    Error::not_found(format_args!("guild {} not found", guild_id,))
}
