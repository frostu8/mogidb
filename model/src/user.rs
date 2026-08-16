//! User model.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A single user.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
pub struct User {
    /// The short ID of the user.
    pub id: String,
    /// The display name of the user.
    pub display_name: String,
    /// The ID of the associated Discord user, if it exists.
    pub discord_user_id: Option<i64>,
}
