//! Events and enums.

use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// Event format selection mode.
#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize_repr,
    PartialEq,
    Eq,
    Serialize_repr,
    Hash,
    TryFromPrimitive,
    IntoPrimitive,
)]
pub enum FormatSelectionMode {
    Vote = 0,
    Random = 1,
}
