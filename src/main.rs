use axum::{
    Router,
    routing::{get, post},
};
use mogidb::{AppState, config::read_config};
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
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000")
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;

    Ok(())
}
