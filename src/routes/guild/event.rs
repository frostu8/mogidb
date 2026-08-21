//! Events on a guild-wide basis.

use axum::extract::{Path, State};
use garde::Validate;

use mogidb_model::{error::ApiError, event::Event};

use serde::Deserialize;

use sqlx::SqliteConnection;
use utoipa::IntoParams;

use crate::{
    AppState,
    error::Error,
    event::{EventEntity, ListEventsQuery},
    form::Form,
    json::Json,
    server::ServerTracker,
    validate::Valid,
};

#[derive(Debug, Deserialize, Validate, IntoParams)]
#[serde(default)]
#[garde(context(AppState as state))]
pub struct GuildEventsFilters {
    /// Whether or not to show only active events.
    ///
    /// Defaults to `false`.
    #[garde(skip)]
    pub active: bool,
}

impl Default for GuildEventsFilters {
    fn default() -> Self {
        GuildEventsFilters { active: false }
    }
}

/// Fetches all events in a guild.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/events",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        GuildEventsFilters,
    ),
    responses(
        (status = OK, description = "The events of the guild", body = Vec<Event>),
        (status = NOT_FOUND, description = "Guild not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn list(
    Path((discord_guild_id,)): Path<(i64,)>,
    State(state): State<AppState>,
    Valid(Form(filters)): Valid<Form<GuildEventsFilters>>,
) -> Result<Json<Vec<Event>>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    // Get guild ID first
    let guild_id = sqlx::query_as::<_, (i32,)>("SELECT id FROM guild WHERE discord_guild_id = $1")
        .bind(discord_guild_id)
        .fetch_optional(&mut *conn)
        .await
        .map(|id| id.map(|(id,)| id))
        .map_err(Error::new)?;
    let Some(guild_id) = guild_id else {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            discord_guild_id
        )));
    };

    // Search for active events
    let events = ListEventsQuery::new()
        .guild_id(guild_id)
        .active(filters.active)
        .fetch(&state.server_tracker, &mut *conn)
        .await?;

    let mut results = Vec::with_capacity(events.len());
    for mut event in events {
        aggregate_event(&mut event, &state.server_tracker, &mut *conn).await?;
        results.push(Event::try_from(event)?);
    }

    Ok(Json(results))
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
