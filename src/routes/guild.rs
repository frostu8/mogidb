//! Application routes for the guild.

use mogidb_model::guild::Guild;

use crate::{error::Error, json::Json};

/// Creates a new guild.
pub fn create() -> Result<Json<Guild>, Error> {
    todo!()
}
