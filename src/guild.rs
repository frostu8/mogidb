//! Guild operators.

use chrono::{DateTime, Utc};

use mogidb_model::{
    guild::Guild,
    room::RoomOptions,
    server::{GameServer, PlayerInfo, ServerInfo},
};

use sqlx::{FromRow, SqliteConnection};

use crate::{
    error::{Error, ErrorKind},
    server::{Error as ServerError, KnockResult, ServerTracker},
};

#[derive(FromRow)]
pub struct GuildEntity {
    pub id: i32,
    pub discord_guild_id: i64,
    pub settings: String,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<GuildEntity> for Guild {
    type Error = serde_json::Error;

    fn try_from(value: GuildEntity) -> Result<Self, Self::Error> {
        Ok(Guild {
            id: value.discord_guild_id,
            settings: serde_json::from_str::<RoomOptions>(&value.settings)?,
            servers: None,
            created_at: value.inserted_at,
            updated_at: value.updated_at,
        })
    }
}

/// Preloads servers in a guild.
pub async fn preload_servers(
    guild: &mut Guild,
    tracker: &ServerTracker,
    conn: &mut SqliteConnection,
) -> Result<(), Error> {
    #[derive(FromRow)]
    struct ServerEntity {
        pub id: i32,
        pub remote: String,
        pub label: String,
        pub note: Option<String>,
    }

    let mut servers = Vec::new();

    // List all servers in a guild
    let res = sqlx::query_as::<_, ServerEntity>(
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
