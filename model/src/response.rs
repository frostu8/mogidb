//! Response models from various endpoints.

use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

use crate::event::Event;

/// A response to a join request.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
pub struct JoinEventResponse {
    /// The event after the join.
    pub event: Event,
    /// Whether or not the event was started from this join.
    ///
    /// When this is `true`, the status of the event will have been
    /// automatically advanced to [`EventStatus::Ongoing`].
    pub started: bool,
}
