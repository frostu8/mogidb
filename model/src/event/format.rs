//! Event formats.

use num_enum::{IntoPrimitive, TryFromPrimitive};

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

use utoipa::ToSchema;

use crate::server::GameServer;

/// Event format selection mode.
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
pub enum FormatSelectionMode {
    Vote = 0,
    Random = 1,
}

/// Format team mode.
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
pub enum TeamMode {
    FreeForAll = 1,
    Two = 2,
    Three = 3,
    Four = 4,
}

/// An event format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
pub struct EventFormat {
    /// The id of the format.
    pub id: i32,
    /// The human-readable name of the format.
    pub name: String,
    /// The team mode for the event.
    pub team_mode: TeamMode,
    /// The allowed servers for the event format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<GameServer>>,
}
