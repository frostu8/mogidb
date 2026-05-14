//! Mogi database.

pub mod config;
pub mod error;
pub mod json;
pub mod routes;
pub mod server;
pub mod validate;

use std::sync::Arc;

use color_eyre::Section;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

use crate::{config::Config, server::ServerTracker};

use eyre::eyre;

/// The application state.
#[derive(Clone, Debug)]
pub struct AppState {
    pub db: SqlitePool,
    pub server_tracker: Arc<ServerTracker>,
    pub config: Arc<Config>,
}

impl AppState {
    /// Creates a new `AppState`.
    pub async fn new(config: Config) -> eyre::Result<AppState> {
        let Some(database_url) = config.server.database_url.as_ref() else {
            return Err(eyre!("failed to get `DATABASE_URL`")
                .note("set a `DATABASE_URL` in environment or .env"));
        };

        let db = SqlitePoolOptions::new().connect(database_url).await?;

        Ok(AppState {
            db,
            server_tracker: Arc::new(ServerTracker::new()),
            config: Arc::new(config),
        })
    }
}
