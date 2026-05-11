//! Game server management.

use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Information about the currently running server.
    pub info: Option<ServerInfo>,
    /// The guild the server beelongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild: Option<Guild>,
}

/// Information about a running server.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ServerInfo {
    // Server identification info.
    pub application: String,
    pub version: i32,
    pub subversion: i32,
    // Initial bytes of hash of commit
    pub commit: String,

    // Game settings
    pub gametype_name: String,
    pub server_name: String,
    pub number_of_players: i32,
    pub max_players: i32,
    pub modified_game: bool,
    pub cheats_enabled: bool,
    pub avg_mobiums: bool,

    pub game_speed: GameSpeed,
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
