//! Mogi rooms.

use chrono::{DateTime, Utc};

use serde::{Deserialize, Serialize};

use serde_with::skip_serializing_none;
use utoipa::ToSchema;

use crate::{
    event::{EventFormat, FormatSelectionMode},
    guild::Guild,
};

/// A single mogi room.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
pub struct Room {
    /// The id of the room.
    pub id: i64,
    /// The name of the room.
    ///
    /// This is the same as the name of the channel.
    pub name: String,
    /// Whether the room is enabled or not.
    pub enabled: bool,
    /// The room settings.
    pub settings: RoomOptionsOverrides,
    /// The allowed formats for the room.
    #[schema(no_recursion)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formats: Option<Vec<EventFormat>>,
    #[schema(no_recursion)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild: Option<Guild>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Room configuration.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, ToSchema)]
#[serde(default)]
pub struct RoomOptions {
    /// The amount of players needed to start an event.
    pub players_required: u32,
    /// The mode for format selection.
    pub format_selection_mode: FormatSelectionMode,
    /// The amount of votes needed for a format to be selected before time is
    /// up.
    pub votes_required: u32,
    /// The amount of time it takes for events to decay, in seconds.
    ///
    /// When an event decays, it can be ended before the event starts.
    pub decay_after: u32,
    /// The amount of time before the bot warns someone for inactivity, in
    /// seconds.
    pub inactivity_warning_after: u32,
    /// The amount of time before the bot drops someone for inactivity, in
    /// seconds.
    pub inactivity_drop_after: u32,
}

impl RoomOptions {
    /// Merges a room options with its overrides.
    pub fn merge(self, other: RoomOptionsOverrides) -> RoomOptions {
        RoomOptions {
            players_required: other.players_required.unwrap_or(self.players_required),
            format_selection_mode: other
                .format_selection_mode
                .unwrap_or(self.format_selection_mode),
            votes_required: other.votes_required.unwrap_or(self.votes_required),
            decay_after: other.decay_after.unwrap_or(self.decay_after),
            inactivity_warning_after: other
                .inactivity_warning_after
                .unwrap_or(self.inactivity_warning_after),
            inactivity_drop_after: other
                .inactivity_drop_after
                .unwrap_or(self.inactivity_drop_after),
        }
    }
}

impl Default for RoomOptions {
    fn default() -> RoomOptions {
        RoomOptions {
            players_required: 8,
            format_selection_mode: FormatSelectionMode::Vote,
            votes_required: 4,
            decay_after: 3000,
            inactivity_warning_after: 1500,
            inactivity_drop_after: 2100,
        }
    }
}

/// Room configuration overrides.
///
/// These allow `None` values for the fields.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, ToSchema)]
pub struct RoomOptionsOverrides {
    pub players_required: Option<u32>,
    pub format_selection_mode: Option<FormatSelectionMode>,
    pub votes_required: Option<u32>,
    pub decay_after: Option<u32>,
    pub inactivity_warning_after: Option<u32>,
    pub inactivity_drop_after: Option<u32>,
}
