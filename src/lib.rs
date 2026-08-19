//! Mogi database.

pub mod config;
pub mod docs;
pub mod error;
pub mod form;
pub mod guild;
pub mod json;
pub mod room;
pub mod routes;
pub mod server;
pub mod validate;

use std::sync::Arc;

use color_eyre::Section;
use serde::{Deserialize, Deserializer};
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

/// Deserialization helper for distinguishing between null and absent.
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::deserialize(deserializer).map(Some)
}
