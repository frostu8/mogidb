//! Events on a guild-wide basis.

use std::collections::HashSet;

use axum::extract::{Path, State};
use garde::Validate;

use mogidb_model::{error::ApiError, event::Event};

use serde::Deserialize;

use sqlx::SqliteConnection;
use utoipa::IntoParams;

use crate::{
    AppState,
    error::{Error, ErrorKind, ResultExt},
    event::{EventEntity, ListEventsQuery},
    form::Form,
    guild::get_guild,
    json::Json,
    server::ServerTracker,
    user::get_user,
    validate::Valid,
};

#[derive(Debug, Deserialize, Validate, IntoParams)]
#[serde(default)]
#[garde(context(AppState as state))]
#[into_params(parameter_in = Query)]
pub struct GuildEventsFilters {
    /// Whether or not to show only active events.
    ///
    /// Defaults to `false`.
    #[garde(skip)]
    pub active: bool,
    /// Only show events that the user with the given id is in.
    #[garde(skip)]
    pub user: Option<String>,
}

impl Default for GuildEventsFilters {
    fn default() -> Self {
        GuildEventsFilters {
            active: false,
            user: None,
        }
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

    // Get guild
    let guild = get_guild(discord_guild_id, &mut conn).await?;

    // Search for active events
    let mut events = ListEventsQuery::new()
        .guild_id(guild.id)
        .active(filters.active)
        .fetch(&state.server_tracker, &mut *conn)
        .await?;

    // Filter event based on user id, if applicable
    if let Some(short_id) = filters.user.as_ref() {
        // Fetch user
        let user = get_user(&short_id, &mut conn).await.or_none()?;
        let Some(user) = user else {
            return Err(ErrorKind::NoSuchUser(short_id.clone()).into());
        };

        let event_ids = sqlx::query_as::<_, (i32,)>(
            r#"
            SELECT DISTINCT e.id
            FROM event_participant p, event e
            WHERE
                p.event_id = e.id
                AND p.user_id = $1
            "#,
        )
        .bind(user.id)
        .fetch_all(&mut *conn)
        .await
        .map_err(Error::new)?
        .into_iter()
        .map(|(id,)| id)
        .collect::<HashSet<i32>>();

        events.retain_mut(|event| event_ids.contains(&event.id));
    }

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
