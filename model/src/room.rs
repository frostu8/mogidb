//! Mogi rooms.

use serde::{Deserialize, Serialize};

use crate::event::FormatSelectionMode;

/// A single mogi room.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Room {
    #[serde(flatten)]
    pub settings: RoomSettings,
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
