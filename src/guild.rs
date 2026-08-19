//! Guild operators.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use mogidb_model::{
    guild::Guild,
    room::RoomOptions,
    server::{GameServer, PlayerInfo, ServerInfo},
};

use sqlx::{FromRow, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    error::{Error, ErrorKind},
    server::{Error as ServerError, KnockResult, ServerTracker},
};

#[derive(Debug, FromRow)]
pub struct GuildEntity {
    pub id: i32,
    pub discord_guild_id: i64,
    pub settings: String,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Preload fields
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
        .bind(self.id)
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

impl TryFrom<GuildEntity> for Guild {
    type Error = serde_json::Error;

    fn try_from(value: GuildEntity) -> Result<Self, Self::Error> {
        Ok(Guild {
            id: value.discord_guild_id,
            settings: serde_json::from_str::<RoomOptions>(&value.settings)?,
            servers: value
                .servers
                .map(|servers| {
                    servers
                        .into_iter()
                        .map(GameServer::try_from)
                        .collect::<Result<Vec<_>, serde_json::Error>>()
                })
                .transpose()?,
            created_at: value.inserted_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Debug)]
pub struct ServerEntity {
    pub id: i32,
    pub guild_id: i32,
    pub remote: String,
    pub label: String,
    pub note: Option<String>,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Preload fields
    pub guild: Option<GuildEntity>,

    // Knock result
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

impl TryFrom<ServerEntity> for GameServer {
    type Error = serde_json::Error;

    fn try_from(value: ServerEntity) -> Result<Self, Self::Error> {
        let guild = match value.guild {
            Some(guild) => Some(Guild::try_from(guild)?),
            None => None,
        };

        Ok(GameServer {
            id: value.id,
            remote: value.remote,
            label: value.label,
            note: value.note,
            last_update_time: value.remote_server.as_ref().map(|res| res.last_ping_time),
            info: value.remote_server.map(marshal_server_info),
            guild,
        })
    }
}

impl FromRow<'_, SqliteRow> for ServerEntity {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        #[derive(FromRow)]
        struct MaybeGuildEntity {
            #[sqlx(default, rename = "guild_id")]
            pub id: Option<i32>,
            pub discord_guild_id: Option<i64>,
            #[sqlx(default, rename = "guild_settings")]
            pub settings: Option<String>,
            #[sqlx(default, rename = "guild_inserted_at")]
            pub inserted_at: Option<DateTime<Utc>>,
            #[sqlx(default, rename = "guild_updated_at")]
            pub updated_at: Option<DateTime<Utc>>,
        }

        let guild = MaybeGuildEntity::from_row(row)
            .map(Some)?
            .and_then(|maybe| {
                Some(GuildEntity {
                    id: maybe.id?,
                    discord_guild_id: maybe.discord_guild_id?,
                    settings: maybe.settings?,
                    inserted_at: maybe.inserted_at?,
                    updated_at: maybe.updated_at?,
                    servers: None,
                })
            });

        Ok(ServerEntity {
            id: row.try_get("id")?,
            guild_id: row.try_get("guild_id")?,
            remote: row.try_get("remote")?,
            label: row.try_get("label")?,
            note: row.try_get("note")?,
            inserted_at: row.try_get("inserted_at")?,
            updated_at: row.try_get("updated_at")?,
            guild,
            remote_server: None,
        })
    }
}

/// Gets a server by its id.
pub async fn get_server_by_id(
    server_id: i32,
    conn: &mut SqliteConnection,
) -> Result<Option<ServerEntity>, Error> {
    // Fetch server
    sqlx::query_as::<_, ServerEntity>(
        r#"
        SELECT
            s.*,
            g.settings AS guild_settings,
            g.discord_guild_id,
            g.inserted_at AS guild_inserted_at,
            g.updated_at AS guild_inserted_at
        FROM
            server s, guild g
        WHERE
            s.guild_id = g.id
            AND s.id = $1
        "#,
    )
    .bind(server_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)
}

/// Checks if a list of servers is associated with a guild.
pub async fn check_servers(
    guild_id: i32,
    ids: impl IntoIterator<Item = i32>,
    conn: &mut SqliteConnection,
) -> Result<(), Error> {
    let set = sqlx::query_as::<_, (i32,)>(
        r#"
        SELECT s.id
        FROM server s
        WHERE s.guild_id = $1
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
