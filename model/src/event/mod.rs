//! Events and enums.

pub mod format;

pub use format::{EventFormat, FormatSelectionMode};

use chrono::{DateTime, Utc};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

use utoipa::ToSchema;

use crate::{room::Room, server::GameServer, user::User};

/// Event status.
#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize_repr,
    PartialEq,
    Eq,
    Serialize_repr,
    Hash,
    TryFromPrimitive,
    IntoPrimitive,
    ToSchema,
)]
pub enum EventStatus {
    /// Looking for players.
    LFG = 0,
    /// The event queue is closed and the event is ongoing.
    Ongoing = 1,
    /// The event is over.
    Concluded = 2,
    /// The event has been scored.
    Scored = 3,
}

/// A single event.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
pub struct Event {
    /// The short ID of the event.
    pub id: String,
    /// The event status.
    pub status: EventStatus,
    /// The alternate title of the event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The list of players registered for the event.
    pub players: Vec<EventParticipant>,
    /// The format, if it has been selected.
    pub format: Option<EventFormat>,
    /// The server, if one was found.
    pub server: Option<GameServer>,
    /// The room the event is a part of.
    pub room: Room,
    /// When the event was created.
    pub created_at: DateTime<Utc>,
}

/// An event participant.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
pub struct EventParticipant {
    /// The team number assigned to the participant. Players with the same
    /// number are on the same team.
    pub assigned_team: i32,
    /// The associated user of the event.
    pub user: User,
}
