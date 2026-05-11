//! Application routes for the guild.

use axum::extract::{Path, State};

use chrono::{DateTime, Utc};
use garde::Validate;

use mogidb_model::{event::FormatSelectionMode, guild::Guild, room::RoomSettings};

use serde::Deserialize;
use sqlx::FromRow;

use crate::{AppState, error::Error, json::Json, validate::Garde};

#[derive(Default, Deserialize, Validate)]
pub struct CreateGuildRequest {
    #[garde(skip)]
    pub guild_id: i64,
    #[serde(flatten)]
    #[garde(dive)]
    pub settings: UpdateGuildRequest,
}

#[derive(Default, Deserialize, Validate)]
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
pub struct GuildQuery {
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

impl From<GuildQuery> for RoomSettings {
    fn from(value: GuildQuery) -> Self {
        RoomSettings {
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
pub async fn create(
    State(state): State<AppState>,
    Json(Garde(request)): Json<Garde<CreateGuildRequest>>,
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
            created_at: now,
            updated_at: now,
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
    // Fetch the guild
    let res = sqlx::query_as::<_, GuildQuery>(
        r#"
        SELECT *
        FROM guild
        WHERE id = $1
        "#,
    )
    .bind(guild_id)
    .fetch_optional(&state.db)
    .await
    .map_err(Error::new)?;

    match res {
        Some(row) => Ok(Json(Guild {
            created_at: row.inserted_at,
            updated_at: row.updated_at,
            default_settings: row.into(),
        })),
        None => Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id
        ))),
    }
}
