//! Event participant routes.

use axum::extract::{Path, State};

use mogidb_model::{error::ApiError, event::EventParticipant};

use crate::{
    AppState,
    error::{Error, OptionExt as _},
    event::{get_event, get_participants},
    json::Json,
};

/// Lists the participants of an event.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/rooms/{room_id}/events/{event_id}/participants",
    tag = "event",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("event_id" = String, Path, description = "Id of the event"),
    ),
    responses(
        (status = OK, description = "The participants of the event", body = Vec<EventParticipant>),
        (status = NOT_FOUND, description = "Guild, room or event not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn list(
    Path((guild_id, room_id, event_id)): Path<(i64, i64, String)>,
    State(state): State<AppState>,
) -> Result<Json<Vec<EventParticipant>>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    // Get the relevant event
    let event = get_event(
        guild_id,
        room_id,
        &event_id,
        &state.server_tracker,
        &mut conn,
    )
    .await?
    .ok_or_not_found()?;

    let participants = get_participants(event.id, &mut conn)
        .await?
        .into_iter()
        .map(EventParticipant::try_from)
        .collect::<Result<Vec<_>, Error>>()?;

    Ok(Json(participants))
}
