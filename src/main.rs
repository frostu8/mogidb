use std::sync::Arc;

use axum::{
    Router,
    extract::Request,
    middleware::{Next, from_fn},
    response::Response,
    routing::{get, patch, post},
};
use mogidb::{AppState, config::read_config, error::Error};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let _ = dotenv::dotenv();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = read_config("config.toml")?;

    let app_state = AppState::new(config).await?;
    let app = Router::new()
        .route("/guilds", post(mogidb::routes::guild::create))
        .route("/guilds/{guild_id}", get(mogidb::routes::guild::show))
        .route("/guilds/{guild_id}", patch(mogidb::routes::guild::update))
        .route(
            "/guilds/{guild_id}/servers",
            post(mogidb::routes::guild::server::create),
        )
        .route(
            "/guilds/{guild_id}/servers/{server_id}",
            get(mogidb::routes::guild::server::show),
        )
        .with_state(app_state)
        .layer(from_fn(log_app_errors));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000")
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;

    Ok(())
}

// Stolen from: https://github.com/tokio-rs/axum/blob/main/examples/error-handling/src/main.rs
async fn log_app_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    // If the response contains an AppError Extension, log it.
    if let Some(err) = response.extensions().get::<Arc<Error>>() {
        tracing::error!(?err, "an unexpected error occurred inside a handler");
    }
    response
}
