use chrono::{DateTime, Utc};
use mogidb_model::{
    event::{EventFormat, format::TeamMode},
    server::GameServer,
};
use sqlx::{SqliteConnection, prelude::FromRow};

use crate::{
    error::{Error, ErrorKind, NotFound},
    guild::marshal_server_info,
    server::{Error as ServerError, ServerTracker},
};

/// A format entity.
#[derive(Clone, Debug, FromRow)]
pub struct EventFormatEntity {
    pub id: i32,
    pub room_id: i32,
    pub name: String,
    #[sqlx(try_from = "u8")]
    pub team_mode: TeamMode,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    #[sqlx(skip)]
    pub servers: Option<Vec<GameServer>>,
}

impl From<EventFormatEntity> for EventFormat {
    fn from(value: EventFormatEntity) -> Self {
        EventFormat {
            id: value.id,
            name: value.name,
            team_mode: value.team_mode,
            servers: value.servers,
        }
    }
}

impl EventFormatEntity {
    /// For a given server, patches the server list.
    ///
    /// This does **not** automatically update [`EventFormatEntity::servers`].
    /// If you need the list back, call [`EventFormatEntity::preload_servers`].
    pub async fn patch_servers(
        &self,
        ids: impl Into<Vec<i32>>,
        conn: &mut SqliteConnection,
    ) -> Result<(), Error> {
        let ids = ids.into();
        // Fetch server ids.
        let current_ids: Vec<i32> = sqlx::query_as::<_, (i32,)>(
            r#"
            SELECT server_id
            FROM format_server fs
            WHERE fs.event_format_id = $1
            "#,
        )
        .bind(self.id)
        .fetch_all(&mut *conn)
        .await
        .map_err(Error::new)?
        .into_iter()
        .map(|(id,)| id)
        .collect();

        // Remove IDs that exist in both containers.
        let mut removed_ids = current_ids.clone();
        removed_ids.retain_mut(|id| !ids.contains(id));

        // Find the delta between the two servers.
        let mut added_ids = ids;
        added_ids.retain_mut(|id| !current_ids.contains(id));

        // Execute
        for server_id in removed_ids {
            sqlx::query("DELETE FROM format_server WHERE server_id = $2 AND event_format_id = $1")
                .bind(self.id)
                .bind(server_id)
                .execute(&mut *conn)
                .await
                .map_err(Error::new)?;
        }
        for server_id in added_ids {
            sqlx::query("INSERT INTO format_server (server_id, event_format_id) VALUES ($2, $1)")
                .bind(self.id)
                .bind(server_id)
                .execute(&mut *conn)
                .await
                .map_err(Error::new)?;
        }

        Ok(())
    }

    /// Preloads servers for the event format.
    pub async fn preload_servers(
        &mut self,
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

        // List all servers in a guild.
        let res = sqlx::query_as::<_, ServerEntity>(
            r#"
            SELECT s.*
            FROM server s, format_server fs
            WHERE
                s.id = fs.server_id
                AND fs.event_format_id = $1
            "#,
        )
        .bind(self.id)
        .fetch_all(&mut *conn)
        .await
        .map_err(Error::new)?;

        let mut servers = Vec::with_capacity(res.len());

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

        self.servers = Some(servers);

        Ok(())
    }
}

/// Gets a format by its ID.
pub async fn get_format(
    format_id: i32,
    conn: &mut SqliteConnection,
) -> Result<EventFormatEntity, Error> {
    sqlx::query_as::<_, EventFormatEntity>(
        r#"
        SELECT f.*
        FROM event_format f
        WHERE f.id = $1
        "#,
    )
    .bind(format_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)
    .and_then(|format| format.ok_or_else(|| NotFound::Format(format_id).into()))
}
