//! Guild details and configs.

use serde::{Deserialize, Serialize};

use crate::room::RoomSettings;

/// A guild.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Guild {
    #[serde(flatten)]
    pub default_settings: RoomSettings,
}
