use chrono::{DateTime, Utc};
use mogidb_model::{
    event::{EventFormat, format::TeamMode},
    guild::Guild,
    room::{Room, RoomOptions, RoomOptionsOverrides},
};
use sqlx::{FromRow, SqliteConnection};

use crate::error::Error;

#[derive(FromRow)]
pub struct RoomEntity {
    pub id: i32,
    pub discord_channel_id: i64,
    pub name: String,
    pub enabled: bool,
    pub overrides: String,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Guild fields
    pub discord_guild_id: i64,
    pub settings: String,
    pub guild_inserted_at: DateTime<Utc>,
    pub guild_updated_at: DateTime<Utc>,
}

impl TryFrom<RoomEntity> for Room {
    type Error = serde_json::Error;

    fn try_from(value: RoomEntity) -> Result<Self, Self::Error> {
        Ok(Room {
            id: value.discord_channel_id,
            name: value.name,
            enabled: value.enabled,
            settings: serde_json::from_str::<RoomOptionsOverrides>(&value.overrides)?,
            formats: vec![],
            created_at: value.inserted_at,
            updated_at: value.updated_at,

            guild: Guild {
                id: value.discord_guild_id,
                settings: serde_json::from_str::<RoomOptions>(&value.settings)?,
                servers: None,
                created_at: value.guild_inserted_at,
                updated_at: value.guild_updated_at,
            },
        })
    }
}

/// Gets a room by its `discord_channel_id` and parent guild `guild_id`.
pub async fn get_room(
    guild_id: i64,
    discord_channel_id: i64,
    conn: &mut SqliteConnection,
) -> Result<RoomEntity, Error> {
    // Get guild and room
    let row = sqlx::query_as::<_, RoomEntity>(
        r#"
        SELECT
            r.*,
            g.settings,
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
    .bind(guild_id)
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

/// Preloads formats for a room.
pub async fn preload_formats(room: &mut Room, conn: &mut SqliteConnection) -> Result<(), Error> {
    #[derive(FromRow)]
    struct FormatEntity {
        pub id: i32,
        pub name: String,
        #[sqlx(try_from = "u8")]
        pub team_mode: TeamMode,
    }

    impl From<FormatEntity> for EventFormat {
        fn from(value: FormatEntity) -> Self {
            EventFormat {
                id: value.id,
                name: value.name,
                team_mode: value.team_mode,
                servers: None,
            }
        }
    }

    let res = sqlx::query_as::<_, FormatEntity>(
        r#"
        SELECT f.*
        FROM event_format f, room r
        WHERE
            r.discord_channel_id = $1
            AND f.room_id = r.id
        "#,
    )
    .bind(room.id)
    .fetch_all(&mut *conn)
    .await
    .map_err(Error::new)?
    .into_iter()
    .map(EventFormat::from);

    room.formats.extend(res);

    Ok(())
}
