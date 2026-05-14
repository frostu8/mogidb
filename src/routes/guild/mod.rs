//! Application routes for the guild.

pub mod server;

use axum::extract::{Path, State};

use chrono::{DateTime, Utc};
use garde::Validate;

use mogidb_model::{event::FormatSelectionMode, guild::Guild, room::RoomSettings};

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
    pub settings: UpdateGuildRequest,
}

#[derive(Default, Debug, Deserialize, Validate)]
#[garde(context(AppState as state))]
#[serde(default)]
pub struct UpdateGuildRequest {
    #[garde(range(min = 1))]
    pub players_required: Option<i32>,
    #[garde(skip)]
    pub format_selection_mode: Option<FormatSelectionMode>,
    #[garde(range(min = 1))]
    pub votes_required: Option<i32>,
    #[garde(range(min = 0))]
    pub decay_after: Option<i32>,
    #[garde(range(min = 0))]
    pub inactivity_warning_after: Option<i32>,
    #[garde(range(min = 0))]
    pub inactivity_drop_after: Option<i32>,
}

impl UpdateGuildRequest {
    /// Merges settings.
    pub fn merge(&self, other: RoomSettings) -> RoomSettings {
        RoomSettings {
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
struct GuildQuery {
    pub id: i32,
    pub discord_guild_id: i64,
    pub players_required: i32,
    #[sqlx(try_from = "u8")]
    pub format_selection_mode: FormatSelectionMode,
    pub votes_required: i32,
    pub decay_after: i32,
    pub inactivity_warning_after: i32,
    pub inactivity_drop_after: i32,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<GuildQuery> for Guild {
    fn from(value: GuildQuery) -> Self {
        Guild {
            id: value.discord_guild_id,
            default_settings: RoomSettings {
                players_required: value.players_required,
                format_selection_mode: value.format_selection_mode,
                votes_required: value.votes_required,
                decay_after: value.decay_after,
                inactivity_warning_after: value.inactivity_warning_after,
                inactivity_drop_after: value.inactivity_drop_after,
            },
            servers: None,
            created_at: value.inserted_at,
            updated_at: value.updated_at,
        }
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
    let settings = request.settings.merge(RoomSettings::default());

    // Add new guild
    let res = sqlx::query(
        r#"
        INSERT INTO guild
        (
            discord_guild_id, players_required, format_selection_mode,
            votes_required, decay_after, inactivity_warning_after,
            inactivity_drop_after, inserted_at, updated_at
        )
        VALUES
        ($1, $3, $4, $5, $6, $7, $8, $2, $2)
        "#,
    )
    .bind(request.guild_id)
    .bind(now)
    .bind(settings.players_required)
    .bind(u8::from(settings.format_selection_mode))
    .bind(settings.votes_required)
    .bind(settings.decay_after)
    .bind(settings.inactivity_warning_after)
    .bind(settings.inactivity_drop_after)
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => Ok(Json(Guild {
            id: request.guild_id,
            created_at: now,
            updated_at: now,
            // The guild is new, so it should not have any servers.
            servers: Some(vec![]),
            default_settings: settings,
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
            let mut guild: Guild = row.into();
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
    let guild = Guild::from(row);
    let new_settings = request.merge(guild.default_settings);

    // Set guild settings
    sqlx::query(
        r#"
        UPDATE guild
        SET
            players_required = $3,
            format_selection_mode = $4,
            votes_required = $5,
            decay_after = $6,
            inactivity_warning_after = $7,
            inactivity_drop_after = $8,
            updated_at = $2
        WHERE id = $1
        "#,
    )
    .bind(guild_db_id)
    .bind(now)
    .bind(new_settings.players_required)
    .bind(u8::from(new_settings.format_selection_mode))
    .bind(new_settings.votes_required)
    .bind(new_settings.decay_after)
    .bind(new_settings.inactivity_warning_after)
    .bind(new_settings.inactivity_drop_after)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    let mut guild = Guild {
        id: guild.id,
        servers: None,
        created_at: guild.created_at,
        updated_at: now,
        default_settings: new_settings,
    };

    preload_servers(&mut guild, &state.server_tracker, &mut *tx).await?;

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(guild))
}

fn not_found(guild_id: i64) -> Error {
    Error::not_found(format_args!("guild {} not found", guild_id,))
}
