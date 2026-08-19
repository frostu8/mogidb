//! Server management and knocking.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use chrono::Utc;
use garde::Validate;

use mogidb_model::{error::ApiError, server::GameServer};

use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState, deserialize_some,
    error::{Error, ErrorKind},
    guild::{get_server_by_id, marshal_server_info},
    json::Json,
    server::Error as ServerError,
    validate::Valid,
};

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
pub struct CreateServerRequest {
    /// The remote address of the server, in `<hostname>:<port>` form.
    #[garde(custom(is_valid_remote))]
    pub remote: String,
    /// A user-defined label for the server. Falls back to the server name.
    #[garde(length(min = 1))]
    #[schema(min_length = 1)]
    pub label: Option<String>,
    /// A user-defined note for the server.
    #[garde(length(min = 0))]
    pub note: Option<String>,
}

#[derive(Debug, Default, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
#[serde(default)]
pub struct UpdateServerRequest {
    /// A user-defined label for the server.
    ///
    /// Can be set to `null` to reset the server's label to the current server
    /// name.
    #[garde(length(min = 1))]
    #[schema(nullable, min_length = 1)]
    #[serde(deserialize_with = "deserialize_some")]
    pub label: Option<Option<String>>,
    /// A user-defined note for the server.
    ///
    /// Can be set to `null` to remove the note.
    #[garde(length(min = 0))]
    #[schema(nullable)]
    #[serde(deserialize_with = "deserialize_some")]
    pub note: Option<Option<String>>,
}

/// Adds a server to the register.
///
/// This will first ping the server to see if it exists.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/servers",
    tag = "server",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
    ),
    request_body = CreateServerRequest,
    responses(
        (status = OK, description = "The newly registered server", body = GameServer),
        (status = BAD_REQUEST, description = "Invalid request, undefined label, or remote already exists", body = ApiError),
        (status = NOT_FOUND, description = "Guild not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
#[axum::debug_handler]
pub async fn create(
    Path((guild_id,)): Path<(i64,)>,
    State(state): State<AppState>,
    Valid(Json(mut request)): Valid<Json<CreateServerRequest>>,
) -> Result<Json<GameServer>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    let remote = request.remote.trim();

    // Get guild
    let guild = sqlx::query_as::<_, super::GuildEntity>(
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

    // Try to ping server
    let server_state = match state.server_tracker.knock(remote).await {
        Ok(res) => Some(res),
        // Request timed out, maybe the server is down?
        Err(ServerError::Timeout(_)) => None,
        Err(ServerError::Packet(err)) => return Err(ErrorKind::Srb2Packet(err).into()),
        Err(err) => return Err(Error::new(err)),
    };

    // Use user-defined label, or use server name for label.
    let (label, generated_label) = request
        .label
        .take()
        .map(|label| (label, false))
        .or_else(|| {
            server_state
                .as_ref()
                .map(|res| res.info.server_name.to_stripped_str().into_owned())
                .map(|label| (label, true))
        })
        .ok_or_else(|| ErrorKind::UndefinedLabel)?;

    // Add result to db
    let now = Utc::now();
    let result = sqlx::query_as::<_, (i32,)>(
        r#"
        INSERT INTO server (guild_id, remote, label, note, inserted_at, updated_at)
        VALUES ($1, $3, $4, $5, $2, $2)
        RETURNING id
        "#,
    )
    .bind(guild.id)
    .bind(now)
    .bind(remote)
    .bind(&label)
    .bind(request.note.as_ref())
    .fetch_one(&mut *tx)
    .await;

    let (id,) = match result {
        Ok(res) => res,
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            if err.constraint() == Some("unique_remote") {
                // The server with that remote already exists
                return Err(ErrorKind::RemoteExists(request.remote.clone()).into());
            } else if err.constraint() == Some("unique_label") {
                // A server with that label already exists.
                if generated_label {
                    return Err(ErrorKind::UndefinedLabel.into());
                } else {
                    return Err(ErrorKind::LabelInUse(label).into());
                }
            } else {
                unreachable!()
            }
        }
        Err(err) => return Err(Error::new(err)),
    };

    tx.commit().await.map_err(Error::new)?;

    // Return server
    Ok(Json(GameServer {
        id,
        remote: remote.to_owned(),
        label,
        note: request.note.take(),
        last_update_time: server_state.as_ref().map(|res| res.last_ping_time),
        info: server_state.map(marshal_server_info),
        guild: Some(guild.try_into().map_err(Error::new)?),
    }))
}

/// Shows a server.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/servers/{server_id}",
    tag = "server",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("server_id" = i32, Path, description = "Server id"),
    ),
    responses(
        (status = OK, description = "The server", body = GameServer),
        (status = NOT_FOUND, description = "Guild or server not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn show(
    Path((guild_id, server_id)): Path<(i64, i32)>,
    State(state): State<AppState>,
) -> Result<Json<GameServer>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    // Check if guild exists
    let res = sqlx::query_as::<_, (i32,)>("SELECT id FROM guild WHERE discord_guild_id = $1")
        .bind(guild_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Error::new)?;
    let Some((guild_id,)) = res else {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id
        )));
    };

    // Get server
    let server = get_server_by_id(server_id, &mut *conn).await?;
    let Some(mut server) = server else {
        return Err(Error::not_found(format_args!(
            "server {} not found",
            server_id
        )));
    };

    // Check for guild id mismatch
    if server.guild_id != guild_id {
        return Err(Error::not_found(format_args!(
            "server {} not found",
            server_id
        )));
    }

    // Try to ping server
    server.knock(&state.server_tracker).await?;

    // Return server
    Ok(Json(GameServer::try_from(server).map_err(Error::new)?))
}

/// Updates a server.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/servers/{server_id}",
    tag = "server",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("server_id" = i32, Path, description = "Server id"),
    ),
    request_body = UpdateServerRequest,
    responses(
        (status = OK, description = "The server", body = GameServer),
        (status = NOT_FOUND, description = "Guild or server not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn update(
    Path((guild_id, server_id)): Path<(i64, i32)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<UpdateServerRequest>>,
) -> Result<Json<GameServer>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;
    let now = Utc::now();

    // Check if guild exists
    let res = sqlx::query_as::<_, (i32,)>("SELECT id FROM guild WHERE discord_guild_id = $1")
        .bind(guild_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::new)?;
    let Some((guild_id,)) = res else {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id
        )));
    };

    // Get server
    let server = get_server_by_id(server_id, &mut *tx).await?;
    let Some(mut server) = server else {
        return Err(Error::not_found(format_args!(
            "server {} not found",
            server_id
        )));
    };

    // Check for guild id mismatch
    if server.guild_id != guild_id {
        return Err(Error::not_found(format_args!(
            "server {} not found",
            server_id
        )));
    }

    // Try to ping server
    let remote_server = server.knock(&state.server_tracker).await?;

    // Update details
    let mut generated_label = false;
    match request.label {
        Some(Some(label)) => {
            // Basic set operation.
            server.label = label;
        }
        Some(None) => {
            // Reset label, use server label
            generated_label = true;
            server.label = remote_server
                .info
                .server_name
                .to_stripped_str()
                .into_owned();
        }
        None => (), // Do nothing
    }
    if let Some(note) = request.note {
        server.note = note;
    }

    // Update database
    let result = sqlx::query(
        r#"
        UPDATE server
        SET
            label = $3,
            note = $4,
            updated_at = $2
        WHERE
            id = $1
        "#,
    )
    .bind(server.id)
    .bind(now)
    .bind(&server.label)
    .bind(server.note.as_ref())
    .execute(&mut *tx)
    .await;
    match result {
        Ok(_res) => (),
        // Conflicting label? Tell end user
        // This can't trip the remote unique constraint.
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            // A server with that label already exists.
            if generated_label {
                return Err(ErrorKind::UndefinedLabel.into());
            } else {
                return Err(ErrorKind::LabelInUse(server.label.clone()).into());
            }
        }
        // rethrow
        Err(err) => return Err(Error::new(err)),
    }

    tx.commit().await.map_err(Error::new)?;

    // Return server
    Ok(Json(GameServer::try_from(server).map_err(Error::new)?))
}

/// Deletes a server.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/servers/{server_id}",
    tag = "server",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("server_id" = i32, Path, description = "Server id"),
    ),
    responses(
        (status = NO_CONTENT, description = "The server was deleted"),
        (status = NOT_FOUND, description = "Guild or server not found", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn delete(
    Path((guild_id, server_id)): Path<(i64, i32)>,
    State(state): State<AppState>,
) -> Result<StatusCode, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    // Get guild
    let guild = sqlx::query_as::<_, super::GuildEntity>(
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

    // Delete server
    let res = sqlx::query(
        r#"
        DELETE FROM server
        WHERE
            id = $1
            AND guild_id = $2
        "#,
    )
    .bind(server_id)
    .bind(guild.id)
    .execute(&mut *tx)
    .await
    .map_err(Error::new)?;

    tx.commit().await.map_err(Error::new)?;

    if res.rows_affected() > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::not_found(format_args!(
            "server {} not found",
            server_id
        )))
    }
}

fn is_valid_remote(value: &str, _state: &AppState) -> garde::Result {
    let (_address, port) = match value.split_once(':') {
        Some(res) => res,
        None => return Err(garde::Error::new("missing port")),
    };
    if let Err(err) = port.parse::<u16>() {
        return Err(garde::Error::new(format_args!("invalid port: {}", err)));
    }
    Ok(())
}
