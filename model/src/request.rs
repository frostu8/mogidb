//! Request models.

use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// The mode to balance teams with.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TeamBalanceMode {
    /// Shuffles players randomly.
    Shuffle,
}
