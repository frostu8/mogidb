//! Game server management.

use num_enum::{IntoPrimitive, TryFromPrimitive};

use serde::{Deserialize, Serialize};

use serde_repr::{Deserialize_repr, Serialize_repr};

use std::fmt::Write as _;

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
    pub version: u8,
    pub subversion: u8,
    // Initial bytes of hash of commit
    pub commit: String,

    // Game settings
    pub gametype_name: String,
    pub server_name: String,
    pub number_of_players: u8,
    pub max_players: u8,
    pub modified_game: bool,
    pub cheats_enabled: bool,
    pub avg_mobiums: u16,

    pub game_speed: GameSpeed,
    pub flags: ServerFlags,
    pub refuse_reason: RefuseReason,

    // Current level properties
    pub time: u32,
    pub level_time: u32,
    pub map_title: String,
    pub map_md5: String,
    pub actnum: u8,
    pub is_zone: bool,

    pub number_of_files: u8,
    pub http_source: String,
}

impl ServerInfo {
    /// The qualified map title of the map the server is playing.
    pub fn map_name(&self) -> String {
        let mut name = self.map_title.clone();
        if self.is_zone {
            write!(&mut name, " Zone").expect("write fmt");
        }
        if self.actnum > 0 {
            write!(&mut name, " {}", self.actnum).expect("write fmt");
        }
        name
    }
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

impl PlayerInfo {
    /// Checks if this player slot is empty.
    pub fn is_empty(&self) -> bool {
        self.num == 255
    }
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
