//! Application configuration.

use std::path::Path;

use figment::{
    Figment,
    providers::{Env, Format as _, Serialized, Toml},
    value::Uncased,
};
use serde::{Deserialize, Serialize};

/// Full application configuration.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Config {
    /// General server configuration.
    pub server: ServerConfig,
    /// HTTP server configuration.
    pub http: HttpConfig,
}

/// HTTP server configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpConfig {
    /// The port to listen on.
    pub port: u16,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig { port: 4000 }
    }
}

/// General server configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    /// The database url to connect to.
    pub database_url: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig { database_url: None }
    }
}

/// Reads the configuration.
pub fn read_config(config_path: impl AsRef<Path>) -> eyre::Result<Config> {
    Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file(config_path))
        .merge(Env::prefixed("MOGIDB_"))
        .merge(Env::raw().filter_map(|k| match k.as_str() {
            "DATABASE_URL" => Some(Uncased::from("server.database_url")),
            //"DISCORD_CLIENT_ID" => Some(Uncased::from("discord.client_id")),
            //"DISCORD_CLIENT_SECRET" => Some(Uncased::from("discord.client_secret")),
            "PORT" => Some(Uncased::from("http.port")),
            _ => None,
        }))
        .extract()
        .map_err(From::from)
}
