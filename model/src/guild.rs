//! Guild details and configs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{room::RoomOptions, server::GameServer};

/// A guild.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Guild {
    pub id: i64,
    pub settings: RoomOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<GameServer>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
