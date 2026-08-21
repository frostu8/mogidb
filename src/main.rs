use std::sync::Arc;

use axum::{
    Router,
    extract::Request,
    middleware::{Next, from_fn},
    response::Response,
    routing::{delete, get, patch, post, put},
};
use mogidb::{AppState, config::read_config, docs::ApiDoc, error::Error};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};
use utoipa::OpenApi as _;
use utoipa_swagger_ui::SwaggerUi;

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
        .nest(
            "/users",
            Router::new()
                .route("/{user_id}", get(mogidb::routes::user::show))
                .route("/{user_id}", put(mogidb::routes::user::upsert)),
        )
        .nest(
            "/guilds",
            Router::new()
                .route("/", post(mogidb::routes::guild::create))
                .route("/{guild_id}", get(mogidb::routes::guild::show))
                .route("/{guild_id}", patch(mogidb::routes::guild::update))
                .route(
                    "/{guild_id}/events",
                    get(mogidb::routes::guild::event::list),
                )
                .nest(
                    "/{guild_id}/servers",
                    Router::new()
                        .route("/", post(mogidb::routes::guild::server::create))
                        .route("/{server_id}", get(mogidb::routes::guild::server::show))
                        .route("/{server_id}", patch(mogidb::routes::guild::server::update))
                        .route(
                            "/{server_id}",
                            delete(mogidb::routes::guild::server::delete),
                        ),
                )
                .nest(
                    "/{guild_id}/rooms",
                    Router::new()
                        .route("/", post(mogidb::routes::guild::room::create))
                        .route("/{room_id}", get(mogidb::routes::guild::room::show))
                        .route("/{room_id}", patch(mogidb::routes::guild::room::update))
                        .route("/{room_id}", delete(mogidb::routes::guild::room::delete)),
                )
                .nest(
                    "/{guild_id}/rooms/{room_id}/events",
                    Router::new()
                        .route("/", post(mogidb::routes::guild::room::event::create))
                        .route(
                            "/~current",
                            get(mogidb::routes::guild::room::event::show_current),
                        )
                        .route("/{event_id}", get(mogidb::routes::guild::room::event::show))
                        .route(
                            "/{event_id}",
                            patch(mogidb::routes::guild::room::event::update),
                        )
                        .route(
                            "/{event_id}/participants",
                            get(mogidb::routes::guild::room::event::participants::list),
                        )
                        .route(
                            "/{event_id}",
                            delete(mogidb::routes::guild::room::event::delete),
                        ),
                )
                .nest(
                    "/{guild_id}/rooms/{room_id}/formats",
                    Router::new()
                        .route("/", get(mogidb::routes::guild::room::format::list))
                        .route("/", post(mogidb::routes::guild::room::format::create))
                        .route(
                            "/{format_id}",
                            get(mogidb::routes::guild::room::format::show),
                        )
                        .route(
                            "/{format_id}",
                            patch(mogidb::routes::guild::room::format::update),
                        )
                        .route(
                            "/{format_id}",
                            delete(mogidb::routes::guild::room::format::delete),
                        ),
                ),
        )
        .with_state(app_state)
        .layer(from_fn(log_app_errors))
        .merge(SwaggerUi::new("/swagger").url("/openapi/openapi.json", ApiDoc::openapi()));

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
