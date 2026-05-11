//! Server management and knocking.

use axum::extract::{Path, State};

use crate::{AppState, error::Error, json::Json};

/// Adds a server to the register.
pub async fn create(
    Path((guild_id, server_id)): Path<(i64, i32)>,
    State(state): State<AppState>,
) -> Result<Json<GameServer>, Error> {
    todo!()
}
