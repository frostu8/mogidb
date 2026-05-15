//! Game server management.

use chrono::{DateTime, Utc};
use derive_more::Display;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use serde::{Deserialize, Serialize};

use serde_with::{TryFromInto, serde_as};

use serde_repr::{Deserialize_repr, Serialize_repr};

use std::error::Error as StdError;

use crate::guild::Guild;

/// A game server.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct GameServer {
    /// The id of the guild.
    pub id: i32,
    /// The remote address of the server.
    pub remote: String,
    /// The server's label.
    pub label: String,
    /// A user defined note for the server.
    pub note: Option<String>,
    /// Information about the currently running server.
    pub info: Option<ServerInfo>,
    /// When the server was last pinged for information.
    pub last_update_time: Option<DateTime<Utc>>,
    /// The guild the server beelongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild: Option<Guild>,
}

/// Information about a running server.
#[serde_as]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ServerInfo {
    /// The name of the server, with colors removed.
    pub server_name: String,
    /// The gametype of the server.
    pub gametype_name: String,
    /// Maximum player count.
    pub max_players: u8,
    pub modified_game: bool,
    pub cheats_enabled: bool,
    pub avg_mobiums: u16,

    pub game_speed: GameSpeed,
    #[serde_as(as = "TryFromInto<u32>")]
    pub flags: ServerFlags,

    // Current level properties
    pub time: u32,
    pub level_time: u32,

    /// The name of the map.
    pub map_name: String,
    /// The map's MD5 hash.
    pub map_md5: String,

    /// The server's HTTP source for addons.
    pub http_source: String,

    /// The players in the server.
    pub players: Vec<PlayerInfo>,
}

/// Information about a player in a server.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PlayerInfo {
    pub num: u8,
    /// The player's display name.
    pub name: String,
    pub team: u8,
    pub score: i32,
    pub time_in_server: u16,
}

/// Game speed.
#[derive(
    Clone, Debug, Deserialize_repr, PartialEq, Eq, Serialize_repr, TryFromPrimitive, IntoPrimitive,
)]
#[repr(u8)]
pub enum GameSpeed {
    Easy = 0,
    Normal = 1,
    Hard = 2,
}

/// Refuse reason.
#[derive(
    Clone, Debug, Deserialize_repr, PartialEq, Eq, Serialize_repr, TryFromPrimitive, IntoPrimitive,
)]
#[repr(u8)]
pub enum RefuseReason {
    Ok = 0,
    JoinsDisabled = 1,
    Full = 2,
}

bitflags::bitflags! {
    /// Server flags.
    #[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
    pub struct ServerFlags: u32 {
        const LOTS_OF_ADDONS = 0x20;
        const DEDICATED = 0x40;
        const VOICE_ENABLED = 0x80;
    }
}

impl TryFrom<u32> for ServerFlags {
    type Error = ServerFlagsError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        ServerFlags::from_bits(value).ok_or_else(|| ServerFlagsError(value))
    }
}

impl From<ServerFlags> for u32 {
    fn from(value: ServerFlags) -> Self {
        value.bits()
    }
}

#[derive(Debug, Display)]
#[display("invalid server flags: {_0}")]
pub struct ServerFlagsError(u32);

impl StdError for ServerFlagsError {}
