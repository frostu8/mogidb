//! Server management and knocking.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use chrono::Utc;
use garde::Validate;

use mogidb_model::{
    guild::Guild,
    server::{GameServer, PlayerInfo, ServerInfo},
};

use serde::Deserialize;
use sqlx::{FromRow, SqliteConnection};

use crate::{
    AppState,
    error::{Error, ErrorKind},
    json::Json,
    server::{Error as ServerError, KnockResult, ServerTracker},
    validate::Valid,
};

#[derive(Debug, Deserialize, Validate)]
#[garde(context(AppState as state))]
pub struct CreateServerRequest {
    #[garde(custom(is_valid_remote))]
    pub remote: String,
    #[garde(length(min = 1))]
    pub label: Option<String>,
    #[garde(length(min = 0))]
    pub note: Option<String>,
}

#[derive(FromRow)]
struct ServerQuery {
    pub id: i32,
    pub remote: String,
    pub label: String,
    pub note: Option<String>,
}

/// Adds a server to the register.
///
/// This will first ping the server to see if it exists.
#[axum::debug_handler]
pub async fn create(
    Path((guild_id,)): Path<(i64,)>,
    State(state): State<AppState>,
    Valid(Json(mut request)): Valid<Json<CreateServerRequest>>,
) -> Result<Json<GameServer>, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    let remote = request.remote.trim();

    // Get guild
    let guild = sqlx::query_as::<_, super::GuildQuery>(
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
    let label = request
        .label
        .take()
        .or_else(|| {
            server_state
                .as_ref()
                .map(|res| res.info.server_name.to_stripped_str().into_owned())
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
            // The server with that remote already exists
            return Err(ErrorKind::RemoteExists(request.remote.clone()).into());
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
        guild: Some(guild.into()),
    }))
}

/// Shows a server.
pub async fn show(
    Path((guild_id, server_id)): Path<(i64, i32)>,
    State(state): State<AppState>,
) -> Result<Json<GameServer>, Error> {
    // Get guild
    let guild = sqlx::query_as::<_, super::GuildQuery>(
        r#"
        SELECT *
        FROM guild
        WHERE discord_guild_id = $1
        "#,
    )
    .bind(guild_id)
    .fetch_optional(&state.db)
    .await
    .map_err(Error::new)?;
    let Some(guild) = guild else {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id
        )));
    };

    // Get server
    let server = sqlx::query_as::<_, ServerQuery>(
        r#"
        SELECT *
        FROM server
        WHERE guild_id = $1 AND id = $2
        "#,
    )
    .bind(guild.id)
    .bind(server_id)
    .fetch_optional(&state.db)
    .await
    .map_err(Error::new)?;
    let Some(server) = server else {
        return Err(Error::not_found(format_args!(
            "server {} not found",
            server_id
        )));
    };

    // Try to ping server
    let res = match state.server_tracker.knock(&server.remote).await {
        Ok(res) => Some(res),
        // Request timed out, maybe the server is down?
        // Fetch cached result
        Err(ServerError::Timeout(_)) => state.server_tracker.get(&server.remote),
        Err(ServerError::Packet(err)) => return Err(ErrorKind::Srb2Packet(err).into()),
        Err(err) => return Err(Error::new(err)),
    };

    // Return server
    Ok(Json(GameServer {
        id: server.id,
        remote: server.remote,
        label: server.label,
        note: server.note,
        last_update_time: res.as_ref().map(|res| res.last_ping_time),
        info: res.map(marshal_server_info),
        guild: Some(guild.into()),
    }))
}

/// Deletes a server.
pub async fn delete(
    Path((guild_id, server_id)): Path<(i64, i32)>,
    State(state): State<AppState>,
) -> Result<StatusCode, Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;

    // Get guild
    let guild = sqlx::query_as::<_, super::GuildQuery>(
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

/// Preloads servers in a guild
pub async fn preload_servers(
    guild: &mut Guild,
    tracker: &ServerTracker,
    conn: &mut SqliteConnection,
) -> Result<(), Error> {
    let mut servers = Vec::new();

    // List all servers in a guild
    let res = sqlx::query_as::<_, ServerQuery>(
        r#"
        SELECT s.*
        FROM server s, guild g
        WHERE
            s.guild_id = g.id
            AND g.discord_guild_id = $1
        "#,
    )
    .bind(guild.id)
    .fetch_all(&mut *conn)
    .await
    .map_err(Error::new)?;

    for row in res {
        // Ping server
        let remote_server = match tracker.knock(&row.remote).await {
            Ok(res) => Some(res),
            // Request timed out, maybe the server is down?
            // Fetch cached result
            Err(ServerError::Timeout(_)) => tracker.get(&row.remote),
            Err(ServerError::Packet(err)) => return Err(ErrorKind::Srb2Packet(err).into()),
            Err(err) => return Err(Error::new(err)),
        };
        servers.push(GameServer {
            id: row.id,
            remote: row.remote,
            label: row.label,
            note: row.note,
            last_update_time: remote_server.as_ref().map(|res| res.last_ping_time),
            info: remote_server.map(marshal_server_info),
            guild: None,
        });
    }

    guild.servers = Some(servers);

    Ok(())
}

fn marshal_server_info(KnockResult { info, players, .. }: KnockResult) -> ServerInfo {
    ServerInfo {
        map_name: info.map_name(),
        server_name: info.server_name.to_stripped_str().into_owned(),
        gametype_name: info.gametype_name,
        max_players: info.max_players,
        modified_game: info.modified_game,
        cheats_enabled: info.cheats_enabled,
        avg_mobiums: info.avg_mobiums,
        game_speed: info.game_speed,
        flags: info.flags,
        time: info.time,
        level_time: info.level_time,
        map_md5: info.map_md5,
        http_source: info.http_source,
        players: players
            .into_iter()
            .map(|player| PlayerInfo {
                num: player.num,
                name: player.name,
                team: player.team,
                score: player.score,
                time_in_server: player.time_in_server,
            })
            .collect::<Vec<_>>(),
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
