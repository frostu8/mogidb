//! Events and enums.

use serde_repr::{Deserialize_repr, Serialize_repr};

/// Event format selection mode.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize_repr, PartialEq, Eq, Serialize_repr, Hash)]
pub enum FormatSelectionMode {
    Vote = 0,
    Random = 1,
}
