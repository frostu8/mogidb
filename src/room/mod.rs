pub mod format;

use chrono::{DateTime, Utc};
use mogidb_model::room::{Room, RoomOptionsOverrides};
use sqlx::{FromRow, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    error::Error, guild::GuildEntity, room::format::EventFormatEntity, server::ServerTracker,
};

#[derive(Debug)]
pub struct RoomEntity {
    pub id: i32,
    pub discord_channel_id: i64,
    pub name: String,
    pub enabled: bool,
    pub overrides: String,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Preload fields
    pub guild: GuildEntity,
    pub formats: Option<Vec<EventFormatEntity>>,
}

impl FromRow<'_, SqliteRow> for RoomEntity {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(RoomEntity {
            id: row.try_get("id")?,
            discord_channel_id: row.try_get("discord_channel_id")?,
            name: row.try_get("name")?,
            enabled: row.try_get("enabled")?,
            overrides: row.try_get("overrides")?,
            inserted_at: row.try_get("inserted_at")?,
            updated_at: row.try_get("updated_at")?,
            guild: GuildEntity {
                id: row.try_get("guild_id")?,
                discord_guild_id: row.try_get("discord_guild_id")?,
                settings: row.try_get("guild_settings")?,
                inserted_at: row.try_get("guild_inserted_at")?,
                updated_at: row.try_get("guild_updated_at")?,
                servers: None,
            },
            formats: None,
        })
    }
}

impl TryFrom<RoomEntity> for Room {
    type Error = serde_json::Error;

    fn try_from(value: RoomEntity) -> Result<Self, Self::Error> {
        Ok(Room {
            id: value.discord_channel_id,
            name: value.name,
            enabled: value.enabled,
            settings: serde_json::from_str::<RoomOptionsOverrides>(&value.overrides)?,
            created_at: value.inserted_at,
            updated_at: value.updated_at,
            guild: value.guild.try_into()?,
            formats: value
                .formats
                .unwrap_or_else(Vec::new)
                .into_iter()
                .map(From::from)
                .collect::<Vec<_>>(),
        })
    }
}

impl RoomEntity {
    /// Preloads formats for a room.
    pub async fn preload_formats(&mut self, conn: &mut SqliteConnection) -> Result<(), Error> {
        let res = list_room_formats(self.id, conn).await?;

        self.formats = Some(res);

        Ok(())
    }

    /// Preloads formats for a room, and recursively fetches available servers.
    pub async fn preload_formats_with_servers(
        &mut self,
        tracker: &ServerTracker,
        conn: &mut SqliteConnection,
    ) -> Result<(), Error> {
        let mut res = list_room_formats(self.id, conn).await?;
        for row in res.iter_mut() {
            row.preload_servers(tracker, conn).await?;
        }

        self.formats = Some(res);

        Ok(())
    }
}

/// Gets a room by its `discord_channel_id` and parent guild `guild_id`.
pub async fn get_room(
    discord_guild_id: i64,
    discord_channel_id: i64,
    conn: &mut SqliteConnection,
) -> Result<RoomEntity, Error> {
    // Check if guild exists
    let (no,) = sqlx::query_as::<_, (i32,)>(
        r#"
        SELECT COUNT(*) FROM guild g WHERE g.discord_guild_id = $1
        "#,
    )
    .bind(discord_guild_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(Error::new)?;
    if no == 0 {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            discord_guild_id
        )));
    }

    // Get guild and room
    let row = sqlx::query_as::<_, RoomEntity>(
        r#"
        SELECT
            r.*,
            g.id AS guild_id,
            g.settings AS guild_settings,
            g.discord_guild_id,
            g.inserted_at AS guild_inserted_at,
            g.updated_at AS guild_updated_at
        FROM room r, guild g
        WHERE
            discord_guild_id = $1
            AND discord_channel_id = $2
            AND r.parent_id = g.id
        "#,
    )
    .bind(discord_guild_id)
    .bind(discord_channel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)?;
    let Some(room) = row else {
        return Err(Error::not_found(format_args!(
            "room {} not found",
            discord_channel_id
        )));
    };

    Ok(room)
}

/// Lists all formats associated with a room.
pub async fn list_room_formats(
    room_id: i32,
    conn: &mut SqliteConnection,
) -> Result<Vec<EventFormatEntity>, Error> {
    // List all formats for the given room.
    let res = sqlx::query_as::<_, EventFormatEntity>(
        r#"
        SELECT f.*
        FROM event_format f
        WHERE f.room_id = $1
        "#,
    )
    .bind(room_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(Error::new)?;

    Ok(res)
}
