//! Guild operators.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use mogidb_model::{
    guild::Guild,
    room::RoomOptions,
    server::{GameServer, PlayerInfo, ServerInfo},
};

use sqlx::{FromRow, SqliteConnection};

use crate::{
    error::{Error, ErrorKind, NotFound},
    server::{Error as ServerError, KnockResult, ServerTracker},
};

#[derive(Clone, Debug, FromRow)]
pub struct GuildEntity {
    pub id: i32,
    pub discord_guild_id: i64,
    #[sqlx(json)]
    pub settings: RoomOptions,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    #[sqlx(skip)]
    pub servers: Option<Vec<ServerEntity>>,
}

impl GuildEntity {
    /// Preloads servers in a guild, fetching the server's latest information.
    pub async fn preload_servers(
        &mut self,
        tracker: &ServerTracker,
        conn: &mut SqliteConnection,
    ) -> Result<(), Error> {
        // List all servers in a guild
        let mut servers = sqlx::query_as::<_, ServerEntity>(
            r#"
            SELECT s.*
            FROM server s, guild g
            WHERE
                s.guild_id = g.id
                AND g.discord_guild_id = $1
            "#,
        )
        .bind(self.discord_guild_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(Error::new)?;

        for server in servers.iter_mut() {
            // Ping server
            let remote_server = match tracker.knock(&server.remote).await {
                Ok(res) => Some(res),
                // Request timed out, maybe the server is down?
                // Fetch cached result
                Err(ServerError::Timeout(_)) => tracker.get(&server.remote),
                Err(ServerError::Packet(err)) => return Err(ErrorKind::Srb2Packet(err).into()),
                Err(err) => return Err(Error::new(err)),
            };
            server.remote_server = remote_server;
        }

        self.servers = Some(servers);

        Ok(())
    }
}

impl From<GuildEntity> for Guild {
    fn from(value: GuildEntity) -> Self {
        Guild {
            id: value.discord_guild_id,
            settings: value.settings,
            servers: value.servers.map(|servers| {
                servers
                    .into_iter()
                    .map(GameServer::from)
                    .collect::<Vec<_>>()
            }),
            created_at: value.inserted_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone, Debug, FromRow)]
pub struct ServerEntity {
    pub id: i32,
    pub guild_id: i32,
    pub remote: String,
    pub label: String,
    pub note: Option<String>,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Preload fields
    #[sqlx(skip)]
    pub guild: Option<GuildEntity>,

    // Knock result
    #[sqlx(skip)]
    pub remote_server: Option<KnockResult>,
}

impl ServerEntity {
    /// Knocks for server information.
    pub async fn knock(&mut self, tracker: &ServerTracker) -> Result<&KnockResult, Error> {
        // Ping server
        let remote_server = match tracker.knock(&self.remote).await {
            Ok(res) => Some(res),
            // Request timed out, maybe the server is down?
            // Fetch cached result
            Err(ServerError::Timeout(_)) => tracker.get(&self.remote),
            Err(ServerError::Packet(err)) => return Err(ErrorKind::Srb2Packet(err).into()),
            Err(err) => return Err(Error::new(err)),
        };
        self.remote_server = remote_server;

        Ok(self.remote_server.as_ref().unwrap())
    }
}

impl From<ServerEntity> for GameServer {
    fn from(value: ServerEntity) -> Self {
        GameServer {
            id: value.id,
            remote: value.remote,
            label: value.label,
            note: value.note,
            last_update_time: value.remote_server.as_ref().map(|res| res.last_ping_time),
            info: value.remote_server.map(marshal_server_info),
            guild: value.guild.map(Guild::from),
        }
    }
}

/// Gets a guild by its Discord guild ID.
pub async fn get_guild(
    discord_guild_id: i64,
    conn: &mut SqliteConnection,
) -> Result<GuildEntity, Error> {
    sqlx::query_as::<_, GuildEntity>(
        r#"
        SELECT *
        FROM guild g
        WHERE discord_guild_id = $1
        "#,
    )
    .bind(discord_guild_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)
    .and_then(|guild| guild.ok_or_else(|| NotFound::Guild(discord_guild_id).into()))
}

/// Gets a server by its id.
pub async fn get_server(
    server_id: i32,
    conn: &mut SqliteConnection,
) -> Result<ServerEntity, Error> {
    sqlx::query_as::<_, ServerEntity>(
        r#"
        SELECT *
        FROM server s
        WHERE id = $1
        "#,
    )
    .bind(server_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)
    .and_then(|server| server.ok_or_else(|| NotFound::Server(server_id).into()))
}

/// Checks if a list of servers is associated with a guild.
pub async fn check_servers(
    guild_id: i32,
    ids: impl IntoIterator<Item = i32>,
    conn: &mut SqliteConnection,
) -> Result<(), Error> {
    let set = sqlx::query_as::<_, (i32,)>(
        r#"
        SELECT id
        FROM server
        WHERE guild_id = $1
        "#,
    )
    .bind(guild_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(Error::new)?
    .into_iter()
    .map(|(id,)| id)
    .collect::<HashSet<i32>>();

    let invalid_ids = ids
        .into_iter()
        .filter(|id| !set.contains(id))
        .collect::<Vec<_>>();

    if invalid_ids.len() > 0 {
        Err(ErrorKind::InvalidServerIds(invalid_ids).into())
    } else {
        Ok(())
    }
}

/// Converts a [`KnockResult`] into a ready-to-serve [`ServerInfo`].
pub fn marshal_server_info(KnockResult { info, players, .. }: KnockResult) -> ServerInfo {
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
