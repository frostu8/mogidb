//! Server authentication.

use axum::{
    extract::{Request, State},
    http::HeaderName,
    middleware::Next,
    response::{IntoResponse as _, Response},
};

use subtle::ConstantTimeEq as _;

use crate::{
    AppState,
    error::{Error, ErrorKind},
};

/// A middleware to verify incoming connections.
pub async fn auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(expected_key) = state.config.server.access_token.as_ref() else {
        // Accept all requests without an access token.
        return next.run(req).await;
    };

    // Get the token from the X-API-KEY header.
    let header = req
        .headers()
        .get(HeaderName::from_static("x-api-key"))
        .and_then(|v| v.to_str().ok());
    match header {
        Some(key) if key.as_bytes().ct_eq(expected_key.as_bytes()).into() => {
            // Okay, the key is right, pass through
            next.run(req).await
        }
        _ => Error::from(ErrorKind::Unauthorized).into_response(),
    }
}
