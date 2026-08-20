pub mod format;

use chrono::{DateTime, Utc};
use derive_more::Display;
use mogidb_model::{
    guild::Guild,
    room::{Room, RoomOptionsOverrides},
};
use sqlx::{FromRow, SqliteConnection};

use crate::{
    error::{Error, OptionExt as _},
    guild::{GuildEntity, get_guild},
    room::format::EventFormatEntity,
    server::ServerTracker,
};

#[derive(Clone, Debug, FromRow)]
pub struct RoomEntity {
    pub id: i32,
    pub discord_channel_id: i64,
    pub name: String,
    pub enabled: bool,
    #[sqlx(json)]
    pub overrides: RoomOptionsOverrides,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Preload fields
    #[sqlx(skip)]
    pub guild: Option<GuildEntity>,
    #[sqlx(skip)]
    pub formats: Option<Vec<EventFormatEntity>>,
}

impl TryFrom<RoomEntity> for Room {
    type Error = MissingGuild;

    fn try_from(value: RoomEntity) -> Result<Self, Self::Error> {
        Ok(Room {
            id: value.discord_channel_id,
            name: value.name,
            enabled: value.enabled,
            settings: value.overrides,
            created_at: value.inserted_at,
            updated_at: value.updated_at,
            guild: value.guild.map(Guild::from).ok_or(MissingGuild)?,
            formats: value
                .formats
                .map(|v| v.into_iter().map(From::from).collect::<Vec<_>>()),
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

#[derive(Debug, Display)]
#[display("missing embedded guild when converting to API model")]
pub struct MissingGuild;

impl std::error::Error for MissingGuild {}

/// Gets a room by its `discord_channel_id` and parent guild `guild_id`.
pub async fn get_room(
    discord_guild_id: i64,
    discord_channel_id: i64,
    conn: &mut SqliteConnection,
) -> Result<Option<RoomEntity>, Error> {
    // Get guild
    let guild = get_guild(discord_guild_id, &mut *conn)
        .await?
        .ok_or_not_found()?;

    // Get room
    sqlx::query_as::<_, RoomEntity>(
        r#"
        SELECT r.*
        FROM room r
        WHERE r.parent_id = $1 AND r.discord_channel_id = $2
        "#,
    )
    .bind(guild.id)
    .bind(discord_channel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)
    .map(|room| {
        room.map(|room| RoomEntity {
            guild: Some(guild),
            ..room
        })
    })
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
