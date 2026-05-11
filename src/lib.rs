//! Mogi database.

pub mod config;
pub mod error;
pub mod json;
pub mod routes;

use std::sync::Arc;

use crate::config::Config;

/// The application state.
#[derive(Clone, Debug)]
pub struct AppState {
    pub config: Arc<Config>,
}

impl AppState {
    /// Creates a new `AppState`.
    pub fn new(config: Config) -> AppState {
        AppState {
            config: Arc::new(config),
        }
    }
}
