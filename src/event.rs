//! Event operations.

use chrono::{DateTime, Utc};
use derive_more::Display;
use mogidb_model::{
    event::{Event, EventParticipant, EventStatus},
    room::Room,
    user::User,
};

use sqlx::{FromRow, Row as _, SqliteConnection};

use crate::{
    error::{Error, OptionExt as _},
    guild::ServerEntity,
    room::{RoomEntity, format::EventFormatEntity, get_room},
    server::ServerTracker,
    user::UserEntity,
};

#[derive(Clone, Debug, FromRow)]
pub struct EventEntity {
    pub id: i32,
    pub short_id: String,
    pub room_id: i32,
    pub title: Option<String>,
    #[sqlx(try_from = "u8")]
    pub status: EventStatus,
    pub format_id: Option<i32>,
    pub server_id: Option<i32>,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    #[sqlx(skip)]
    pub participants: Option<Vec<EventParticipantEntity>>,
    #[sqlx(skip)]
    pub room: Option<RoomEntity>,
    #[sqlx(skip)]
    pub format: Option<EventFormatEntity>,
    #[sqlx(skip)]
    pub server: Option<ServerEntity>,
}

impl EventEntity {
    /// Preloads all the event's participants.
    pub async fn preload_participants(&mut self, conn: &mut SqliteConnection) -> Result<(), Error> {
        // Fetch all players
        let res = sqlx::query(
            r#"
            SELECT
                p.*,
                u.short_id AS user_short_id,
                u.display_name AS user_display_name,
                u.flags AS user_flags,
                u.inserted_at AS user_inserted_at,
                u.updated_at AS user_updated_at,
                u.discord_user_id
            FROM event_participant p, user u
            WHERE
                p.user_id = u.id
                AND p.event_id = $1
            "#,
        )
        .bind(self.id)
        .fetch_all(&mut *conn)
        .await
        .map_err(Error::new)?
        .into_iter()
        .map(|row| {
            Ok(EventParticipantEntity {
                id: row.try_get("id")?,
                user_id: row.try_get("user_id")?,
                event_id: row.try_get("event_id")?,
                team_number: row.try_get("team_number")?,
                inserted_at: row.try_get("inserted_at")?,
                updated_at: row.try_get("updated_at")?,
                user: Some(UserEntity {
                    id: row.try_get("user_id")?,
                    short_id: row.try_get("user_short_id")?,
                    display_name: row.try_get("user_display_name")?,
                    flags: row.try_get::<i32, _>("user_flags")?.into(),
                    discord_user_id: row.try_get("discord_user_id")?,
                    inserted_at: row.try_get("user_inserted_at")?,
                    updated_at: row.try_get("user_updated_at")?,
                }),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(Error::new)?;

        self.participants = Some(res);
        Ok(())
    }
}

impl TryFrom<EventEntity> for Event {
    type Error = Error;

    fn try_from(value: EventEntity) -> Result<Self, Self::Error> {
        Ok(Event {
            id: value.short_id,
            status: value.status,
            title: value.title,
            players: value
                .participants
                .map(|p| {
                    p.into_iter()
                        .map(EventParticipant::try_from)
                        .collect::<Result<Vec<_>, Error>>()
                })
                .ok_or_else(|| Error::new(MissingPlayers))
                .flatten()?,
            format: None,
            server: None,
            room: value
                .room
                .map(Room::try_from)
                .transpose()
                .map_err(Error::new)?,
            created_at: value.inserted_at,
        })
    }
}

#[derive(Debug, Display)]
#[display("missing embedded players list when converting to API model")]
pub struct MissingPlayers;

impl std::error::Error for MissingPlayers {}

#[derive(Clone, Debug, FromRow)]
pub struct EventParticipantEntity {
    pub id: i32,
    pub user_id: i32,
    pub event_id: i32,
    pub team_number: Option<i32>,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Nested user data
    #[sqlx(skip)]
    pub user: Option<UserEntity>,
}

impl TryFrom<EventParticipantEntity> for EventParticipant {
    type Error = Error;

    fn try_from(value: EventParticipantEntity) -> Result<Self, Self::Error> {
        Ok(EventParticipant {
            team_number: value.team_number,
            user: value
                .user
                .map(User::from)
                .ok_or_else(|| Error::new(MissingUser))?,
        })
    }
}

#[derive(Debug, Display)]
#[display("missing embedded user when converting to API model")]
pub struct MissingUser;

impl std::error::Error for MissingUser {}

/// Gets an event by its parent guild id, its parent room id, and the event id.
pub async fn get_event(
    discord_guild_id: i64,
    discord_channel_id: i64,
    event_id: &str,
    tracker: &ServerTracker,
    conn: &mut SqliteConnection,
) -> Result<Option<EventEntity>, Error> {
    // Get room w/ embedded guild
    let room = get_room(discord_guild_id, discord_channel_id, &mut *conn)
        .await?
        .ok_or_not_found()?;

    // Get event, format and server
    let row = sqlx::query(
        r#"
        SELECT
            e.*,
            f.room_id AS format_room_id,
            f.name AS format_name,
            f.team_mode AS format_team_mode,
            f.inserted_at AS format_inserted_at,
            f.updated_at AS format_updated_at,
            s.guild_id AS server_guild_id,
            s.remote AS server_remote,
            s.label AS server_label,
            s.note AS server_note,
            s.inserted_at AS server_inserted_at,
            s.updated_at AS server_updated_at
        FROM event e
        LEFT JOIN event_format AS f ON e.format_id = f.id
        LEFT JOIN server AS s ON e.server_id = s.id
        WHERE short_id = $1
        "#,
    )
    .bind(event_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)?;

    if let Some(row) = row {
        let extract = || -> Result<EventEntity, sqlx::Error> {
            let mut event = EventEntity::from_row(&row)?;
            event.room = Some(room);

            // Extract joined fields
            if let Some(format_id) = event.format_id {
                event.format = Some(EventFormatEntity {
                    id: format_id,
                    room_id: row.try_get("format_room_id")?,
                    name: row.try_get("format_name")?,
                    team_mode: row
                        .try_get::<u8, _>("format_team_mode")?
                        .try_into()
                        .map_err(|err| sqlx::Error::ColumnDecode {
                            index: "format_team_mode".into(),
                            source: Box::new(err),
                        })?,
                    inserted_at: row.try_get("format_inserted_at")?,
                    updated_at: row.try_get("format_updated_at")?,
                    // Do not prefill this unless the endpoint wants it
                    servers: None,
                });
            }

            if let Some(server_id) = event.server_id {
                event.server = Some(ServerEntity {
                    id: server_id,
                    guild_id: row.try_get("server_guild_id")?,
                    remote: row.try_get("server_remote")?,
                    label: row.try_get("server_label")?,
                    note: row.try_get("server_note")?,
                    inserted_at: row.try_get("server_inserted_at")?,
                    updated_at: row.try_get("server_updated_at")?,
                    guild: None,
                    remote_server: None,
                })
            }

            Ok(event)
        };

        let mut event = extract().map_err(Error::new)?;
        if let Some(server) = event.server.as_mut() {
            server.knock(tracker).await?;
        }

        Ok(Some(event))
    } else {
        Ok(None)
    }
}
