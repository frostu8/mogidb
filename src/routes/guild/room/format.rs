//! Room format management.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use garde::Validate;

use mogidb_model::{
    error::ApiError,
    event::{EventFormat, format::TeamMode},
};
use serde::Deserialize;

use utoipa::ToSchema;

use crate::{
    AppState,
    error::Error,
    form::Form,
    guild::check_servers,
    json::Json,
    room::{
        format::{EventFormatEntity, get_format},
        list_room_formats,
    },
    routes::guild::room::find_room,
    validate::Valid,
};

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
pub struct CreateEventFormatRequest {
    #[garde(length(max = 255))]
    #[schema(max_length = 255)]
    pub name: String,
    #[garde(skip)]
    pub team_mode: TeamMode,
    /// A list of server IDs to associate with the format.
    ///
    /// Servers associated with a format may be selected for play.
    #[garde(skip)]
    #[serde(default)]
    pub servers: Vec<i32>,
}

#[derive(Debug, Default, Deserialize, Validate, ToSchema)]
#[serde(default)]
#[garde(context(AppState as state))]
pub struct UpdateEventFormatRequest {
    #[garde(length(max = 255))]
    #[schema(max_length = 255)]
    pub name: Option<String>,
    #[garde(skip)]
    pub team_mode: Option<TeamMode>,
    /// A list of server IDs to associate with the format.
    ///
    /// Servers associated with a format may be selected for play.
    #[garde(skip)]
    pub servers: Option<Vec<i32>>,
}

#[derive(Default, Debug, Deserialize, Validate, ToSchema)]
#[serde(default)]
#[garde(context(AppState as state))]
pub struct EventFormatsFilters {
    #[garde(range(min = 1))]
    #[schema(minimum = 1)]
    pub player_count: Option<usize>,
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
    request_body = CreateEventFormatRequest,
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
    Valid(Json(request)): Valid<Json<CreateEventFormatRequest>>,
) -> Result<Json<EventFormat>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    let now = Utc::now();

    // Get guild and room
    let room_id = find_room(guild_id, channel_id, &mut *tx).await?;
    let (guild_id,) = sqlx::query_as::<_, (i32,)>(
        r#"
        SELECT id FROM guild WHERE discord_guild_id = $1
        "#,
    )
    .bind(guild_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(Error::new)?;

    // Create new format
    let mut format = sqlx::query_as::<_, EventFormatEntity>(
        r#"
        INSERT INTO event_format (inserted_at, updated_at, room_id, name, team_mode)
        VALUES ($1, $1, $2, $3, $4)
        RETURNING id, room_id, name, team_mode, updated_at, inserted_at
        "#,
    )
    .bind(now)
    .bind(room_id)
    .bind(&request.name)
    .bind(u8::from(request.team_mode))
    .fetch_one(&mut *tx)
    .await
    .map_err(Error::new)?;

    // Add servers
    check_servers(guild_id, request.servers.iter().copied(), &mut *tx).await?;
    format.patch_servers(&request.servers[..], &mut *tx).await?;
    format
        .preload_servers(&state.server_tracker, &mut *tx)
        .await?;

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(EventFormat::from(format)))
}

/// Lists all the formats in a room.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/rooms/{room_id}/formats",
    tag = "room",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
    ),
    request_body = EventFormatsFilters,
    responses(
        (status = OK, description = "The formats assigned to the room", body = Vec<EventFormat>),
        (status = NOT_FOUND, description = "Room not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn list(
    Path((guild_id, channel_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Valid(Form(filters)): Valid<Form<EventFormatsFilters>>,
) -> Result<Json<Vec<EventFormat>>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    let room_id = find_room(guild_id, channel_id, &mut conn).await?;

    // List results
    let res = list_room_formats(room_id, &mut conn).await?;
    let mut formats = Vec::with_capacity(res.len());

    for mut format in res {
        // Skip formats incompatible with player count
        if let Some(player_count) = filters.player_count {
            if !format.team_mode.has_even_teams(player_count) {
                continue;
            }
        }

        format
            .preload_servers(&state.server_tracker, &mut conn)
            .await?;
        formats.push(EventFormat::from(format));
    }

    Ok(Json(formats))
}

/// Fetches an event format.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/rooms/{room_id}/formats/{format_id}",
    tag = "room",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("format_id" = i32, Path, description = "The id of the event format"),
    ),
    responses(
        (status = OK, description = "The event format", body = EventFormat),
        (status = NOT_FOUND, description = "Format not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn show(
    Path((guild_id, channel_id, format_id)): Path<(i64, i64, i32)>,
    State(state): State<AppState>,
) -> Result<Json<EventFormat>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    let room_id = find_room(guild_id, channel_id, &mut *conn).await?;
    let format = get_format(format_id, &mut *conn).await?;
    let Some(mut format) = format else {
        return Err(Error::not_found(format_args!(
            "format {} not found",
            format_id,
        )));
    };

    // Check for room mismatch
    if format.room_id != room_id {
        return Err(Error::not_found(format_args!(
            "format {} not found",
            format_id,
        )));
    }

    format
        .preload_servers(&state.server_tracker, &mut *conn)
        .await?;

    Ok(Json(EventFormat::from(format)))
}

/// Updates an event format.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/rooms/{room_id}/formats/{format_id}",
    tag = "room",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("format_id" = i32, Path, description = "The id of the event format"),
    ),
    request_body = UpdateEventFormatRequest,
    responses(
        (status = OK, description = "The updated format", body = EventFormat),
        (status = BAD_REQUEST, description = "Invalid request", body = ApiError),
        (status = NOT_FOUND, description = "Guild not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn update(
    Path((guild_id, channel_id, format_id)): Path<(i64, i64, i32)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<UpdateEventFormatRequest>>,
) -> Result<Json<EventFormat>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;
    let now = Utc::now();

    let room_id = find_room(guild_id, channel_id, &mut *tx).await?;
    let (guild_id,) = sqlx::query_as::<_, (i32,)>(
        r#"
        SELECT id FROM guild WHERE discord_guild_id = $1
        "#,
    )
    .bind(guild_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(Error::new)?;

    let format = get_format(format_id, &mut *tx).await?;
    let Some(mut format) = format else {
        return Err(Error::not_found(format_args!(
            "format {} not found",
            format_id,
        )));
    };

    // Check for room mismatch
    if format.room_id != room_id {
        return Err(Error::not_found(format_args!(
            "format {} not found",
            format_id,
        )));
    }

    // Update settings
    if let Some(name) = request.name {
        format.name = name;
    }
    if let Some(team_mode) = request.team_mode {
        format.team_mode = team_mode;
    }
    if let Some(servers) = request.servers {
        // Add servers
        check_servers(guild_id, servers.iter().copied(), &mut *tx).await?;
        format.patch_servers(&servers[..], &mut *tx).await?;
    }

    format
        .preload_servers(&state.server_tracker, &mut *tx)
        .await?;

    // Actually do update
    sqlx::query(
        r#"
        UPDATE event_format
        SET
            name = $3,
            team_mode = $4,
            updated_at = $2
        WHERE
            id = $1
        "#,
    )
    .bind(format.id)
    .bind(now)
    .bind(&format.name)
    .bind(u8::from(format.team_mode))
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    tx.commit().await.map_err(Error::new)?;

    Ok(Json(EventFormat::from(format)))
}

/// Removes an event format.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/rooms/{room_id}/formats/{format_id}",
    tag = "room",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("room_id" = i64, Path, description = "Discord channel id of the room"),
        ("format_id" = i32, Path, description = "The id of the event format"),
    ),
    responses(
        (status = NO_CONTENT, description = "The event format was deleted"),
        (status = NOT_FOUND, description = "Guild not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn delete(
    Path((guild_id, channel_id, format_id)): Path<(i64, i64, i32)>,
    State(state): State<AppState>,
) -> Result<StatusCode, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    let room_id = find_room(guild_id, channel_id, &mut *tx).await?;

    // Delete event format
    let res = sqlx::query(
        r#"
        DELETE FROM event_format
        WHERE id = $1 AND room_id = $2
        "#,
    )
    .bind(format_id)
    .bind(room_id)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    tx.commit().await.map_err(Error::new)?;

    if res.rows_affected() > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::not_found(format_args!(
            "format {} not found",
            format_id
        )))
    }
}
