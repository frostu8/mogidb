//! Event operations.

use chrono::{DateTime, Utc};
use derive_more::Display;
use mogidb_model::{
    event::{Event, EventFormat, EventParticipant, EventStatus},
    room::Room,
    server::GameServer,
    user::User,
};

use sea_query::{
    Alias, Asterisk, Expr, ExprTrait, Iden, JoinType, Order, Query, SelectStatement,
    SqliteQueryBuilder,
};
use sea_query_sqlx::SqlxBinder as _;

use sqlx::{FromRow, Row as _, SqliteConnection, sqlite::SqliteRow};

use crate::{
    entity::{
        guild::ServerEntity,
        room::{RoomEntity, format::EventFormatEntity, get_room},
        user::UserEntity,
    },
    error::{Error, NotFound},
    server::ServerTracker,
};

#[derive(Clone, Debug, FromRow)]
pub struct EventEntity {
    pub id: i32,
    pub short_id: String,
    pub room_id: i32,
    pub title: Option<String>,
    #[sqlx(try_from = "u8")]
    pub status: EventStatus,
    pub rejected: bool,
    pub format_id: Option<i32>,
    pub server_id: Option<i32>,
    pub gathered_at: Option<DateTime<Utc>>,
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
    /// Checks if an event allows participant mutations.
    pub fn is_roster_mutable(&self) -> bool {
        !self.rejected && matches!(self.status, EventStatus::LFG | EventStatus::Ongoing)
    }

    /// Preloads all the event's participants.
    pub async fn preload_participants(
        &mut self,
        conn: &mut SqliteConnection,
    ) -> Result<&mut Vec<EventParticipantEntity>, Error> {
        let participants = get_participants(self.id, conn).await?;
        self.participants = Some(participants);

        Ok(self.participants.as_mut().unwrap())
    }
}

/// Fetches all participants of an event, with their users embedded.
pub async fn get_participants(
    event_id: i32,
    conn: &mut SqliteConnection,
) -> Result<Vec<EventParticipantEntity>, Error> {
    // Fetch all players
    sqlx::query(
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
    .bind(event_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(Error::new)?
    .into_iter()
    .map(|row| {
        Ok(EventParticipantEntity {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            event_id: row.try_get("event_id")?,
            substitute: row.try_get("substitute")?,
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
    .map_err(Error::new)
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
            format: value.format.map(EventFormat::from),
            server: value.server.map(GameServer::from),
            room: value
                .room
                .map(Room::try_from)
                .transpose()
                .map_err(Error::new)?,
            created_at: value.inserted_at,
            gathered_at: value.gathered_at,
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
    pub substitute: bool,
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
            substitute: value.substitute,
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
) -> Result<EventEntity, Error> {
    // Get room w/ embedded guild
    let room = get_room(discord_guild_id, discord_channel_id, &mut *conn).await?;

    // Get event, format and server
    let (query, values) = select_event_query()
        .and_where(Expr::col((Table::Event, "short_id")).eq(event_id.to_owned()))
        .build_sqlx(SqliteQueryBuilder);
    let row = sqlx::query_with(sqlx::AssertSqlSafe(query.as_str()), values)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Error::new)?;

    if let Some(row) = row {
        let mut event = unpack_row(&row)?;
        event.room = Some(room);

        if let Some(server) = event.server.as_mut() {
            server.knock(tracker).await?;
        }

        Ok(event)
    } else {
        Err(NotFound::Event(event_id.to_owned()).into())
    }
}

/// For some room, gets the active event.
pub async fn get_active_event(
    discord_guild_id: i64,
    discord_channel_id: i64,
    tracker: &ServerTracker,
    conn: &mut SqliteConnection,
) -> Result<Option<EventEntity>, Error> {
    // Get room w/ embedded guild
    let room = get_room(discord_guild_id, discord_channel_id, &mut *conn).await?;

    // Find current active event
    let (query, values) = select_event_query()
        .and_where(Expr::col((Table::Event, "room_id")).eq(room.id))
        .and_where(
            Expr::col((Table::Event, "status"))
                .eq(u8::from(EventStatus::LFG))
                .or(Expr::col((Table::Event, "status")).eq(u8::from(EventStatus::Ongoing))),
        )
        .order_by((Table::Event, "inserted_at"), Order::Desc)
        .limit(1)
        .build_sqlx(SqliteQueryBuilder);
    let row = sqlx::query_with(sqlx::AssertSqlSafe(query.as_str()), values)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Error::new)?;

    if let Some(row) = row {
        let mut event = unpack_row(&row)?;
        event.room = Some(room);

        if let Some(server) = event.server.as_mut() {
            server.knock(tracker).await?;
        }

        Ok(Some(event))
    } else {
        Ok(None)
    }
}

/// A list events query.
#[derive(Clone, Debug)]
pub struct ListEventsQuery {
    /// Only show events from this guild.
    guild_id: Option<i32>,
    /// Shows only active events.
    active: bool,
}

impl ListEventsQuery {
    /// Creates a new `ListEventsQuery`.
    pub fn new() -> ListEventsQuery {
        ListEventsQuery::default()
    }

    /// The guild to search through.
    pub fn guild_id(self, guild_id: i32) -> ListEventsQuery {
        ListEventsQuery {
            guild_id: Some(guild_id),
            ..self
        }
    }

    /// Whether or not to show only active events.
    pub fn active(self, active: bool) -> ListEventsQuery {
        ListEventsQuery { active, ..self }
    }

    /// Fetches the results.
    pub async fn fetch(
        self,
        tracker: &ServerTracker,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<EventEntity>, Error> {
        use Table::*;

        // Start building query
        let (query, values) = select_event_query()
            .join(
                JoinType::Join,
                Room,
                Expr::col((Event, "room_id")).equals((Room, "id")),
            )
            .column((Room, "discord_channel_id"))
            .expr_as(Expr::col((Room, "parent_id")), Alias::new("room_parent_id"))
            .expr_as(Expr::col((Room, "name")), Alias::new("room_name"))
            .expr_as(Expr::col((Room, "enabled")), Alias::new("room_enabled"))
            .expr_as(Expr::col((Room, "overrides")), Alias::new("room_overrides"))
            .expr_as(
                Expr::col((Room, "inserted_at")),
                Alias::new("room_inserted_at"),
            )
            .expr_as(
                Expr::col((Room, "updated_at")),
                Alias::new("room_updated_at"),
            )
            .apply(|q| {
                if self.active {
                    q.and_where(Expr::col((Event, "rejected")).not()).and_where(
                        Expr::col((Event, "status"))
                            .eq(u8::from(EventStatus::LFG))
                            .or(Expr::col((Event, "status")).eq(u8::from(EventStatus::Ongoing))),
                    );
                }
            })
            .apply_if(self.guild_id, |q, guild_id| {
                q.and_where(Expr::col((Room, "parent_id")).eq(guild_id));
            })
            .build_sqlx(SqliteQueryBuilder);
        let mut events = sqlx::query_with(sqlx::AssertSqlSafe(query.as_str()), values)
            .fetch_all(&mut *conn)
            .await
            .map_err(Error::new)?
            .into_iter()
            .map(|row| -> Result<(SqliteRow, RoomEntity), sqlx::Error> {
                // get room entity
                let room = RoomEntity {
                    id: row.try_get("room_id")?,
                    parent_id: row.try_get("room_parent_id")?,
                    discord_channel_id: row.try_get("discord_channel_id")?,
                    name: row.try_get("room_name")?,
                    enabled: row.try_get("room_enabled")?,
                    overrides: row.try_get::<String, _>("room_overrides").and_then(
                        |overrides_str| {
                            serde_json::from_str(&overrides_str).map_err(|e| {
                                sqlx::Error::ColumnDecode {
                                    index: "room_overrides".into(),
                                    source: Box::new(e),
                                }
                            })
                        },
                    )?,
                    inserted_at: row.try_get("room_inserted_at")?,
                    updated_at: row.try_get("room_updated_at")?,
                    guild: None,
                    formats: None,
                };

                Ok((row, room))
            })
            .map(|e| {
                e.map_err(Error::new).and_then(|(row, room)| {
                    unpack_row(&row).map(|event| EventEntity {
                        room: Some(room),
                        ..event
                    })
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        for event in events.iter_mut() {
            if let Some(server) = event.server.as_mut() {
                server.knock(tracker).await?;
            }
        }

        Ok(events)
    }
}

impl Default for ListEventsQuery {
    fn default() -> Self {
        ListEventsQuery {
            guild_id: None,
            active: true,
        }
    }
}

#[derive(Iden)]
enum Table {
    Event,
    EventFormat,
    Server,
    Room,
}

fn unpack_row(row: &SqliteRow) -> Result<EventEntity, Error> {
    fn extract(row: &SqliteRow) -> Result<EventEntity, sqlx::Error> {
        let mut event = EventEntity::from_row(row)?;

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
    }

    extract(row).map_err(Error::new)
}

fn select_event_query() -> SelectStatement {
    use Table::*;

    Query::select()
        .column((Event, Asterisk))
        .expr_as(
            Expr::col((EventFormat, "room_id")),
            Alias::new("format_room_id"),
        )
        .expr_as(Expr::col((EventFormat, "name")), Alias::new("format_name"))
        .expr_as(
            Expr::col((EventFormat, "team_mode")),
            Alias::new("format_team_mode"),
        )
        .expr_as(
            Expr::col((EventFormat, "inserted_at")),
            Alias::new("format_inserted_at"),
        )
        .expr_as(
            Expr::col((EventFormat, "updated_at")),
            Alias::new("format_updated_at"),
        )
        .expr_as(
            Expr::col((Server, "guild_id")),
            Alias::new("server_guild_id"),
        )
        .expr_as(Expr::col((Server, "remote")), Alias::new("server_remote"))
        .expr_as(Expr::col((Server, "label")), Alias::new("server_label"))
        .expr_as(Expr::col((Server, "note")), Alias::new("server_note"))
        .expr_as(
            Expr::col((Server, "inserted_at")),
            Alias::new("server_inserted_at"),
        )
        .expr_as(
            Expr::col((Server, "updated_at")),
            Alias::new("server_updated_at"),
        )
        .from(Event)
        .left_join(
            EventFormat,
            Expr::col((Event, "format_id")).equals((EventFormat, "id")),
        )
        .left_join(
            Server,
            Expr::col((Event, "server_id")).equals((Server, "id")),
        )
        .take()
}
