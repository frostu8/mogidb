//! Mogi rooms.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::event::FormatSelectionMode;

/// A single mogi room.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Room {
    pub enabled: bool,
    #[serde(flatten)]
    pub settings: RoomSettings,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Room configuration.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RoomSettings {
    /// The amount of players needed to start an event.
    pub players_required: i32,
    /// The mode for format selection.
    pub format_selection_mode: FormatSelectionMode,
    /// The amount of votes needed for a format to be selected before time is
    /// up.
    pub votes_required: i32,
    /// The amount of time it takes for events to decay, in seconds.
    ///
    /// When an event decays, it can be ended before the event starts.
    pub decay_after: i32,
    /// The amount of time before the bot warns someone for inactivity, in
    /// seconds.
    pub inactivity_warning_after: i32,
    /// The amount of time before the bot drops someone for inactivity, in
    /// seconds.
    pub inactivity_drop_after: i32,
}

impl Default for RoomSettings {
    fn default() -> RoomSettings {
        RoomSettings {
            players_required: 8,
            format_selection_mode: FormatSelectionMode::Vote,
            votes_required: 4,
            decay_after: 3000,
            inactivity_warning_after: 1500,
            inactivity_drop_after: 2100,
        }
    }
}
