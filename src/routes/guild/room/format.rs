//! Room format management.

use axum::extract::{Path, State};
use chrono::Utc;
use garde::Validate;

use mogidb_model::{
    error::ApiError,
    event::{EventFormat, format::TeamMode},
};
use serde::Deserialize;

use utoipa::ToSchema;

use crate::{AppState, error::Error, json::Json, room::get_room, validate::Valid};

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
pub struct CreateFormatRequest {
    #[garde(length(max = 255))]
    pub name: String,
    #[garde(skip)]
    pub team_mode: TeamMode,
}

/// Creates a new format for a room.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/rooms/{room_id}/formats",
    tag = "room",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
    ),
    request_body = CreateFormatRequest,
    responses(
        (status = OK, description = "The newly created format", body = EventFormat),
        (status = BAD_REQUEST, description = "Invalid request", body = ApiError),
        (status = NOT_FOUND, description = "Guild not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn create(
    Path((guild_id, channel_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<CreateFormatRequest>>,
) -> Result<Json<EventFormat>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    let now = Utc::now();

    // Get guild and room
    let room = get_room(guild_id, channel_id, &mut *tx).await?;

    // Create new format
    let (id,) = sqlx::query_as::<_, (i32,)>(
        r#"
        INSERT INTO event_format (inserted_at, updated_at, room_id, name, team_mode)
        VALUES ($1, $1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(now)
    .bind(room.id)
    .bind(&request.name)
    .bind(u8::from(request.team_mode))
    .fetch_one(&mut *tx)
    .await
    .map_err(Error::new)?;

    tx.commit().await.map_err(Error::new)?;

    let format = EventFormat {
        id,
        name: request.name,
        team_mode: request.team_mode,
        servers: Some(Vec::new()),
    };

    Ok(Json(format))
}
