//! Error handling.

use std::{
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use derive_more::{Display, From};
use mogidb_model::error::ApiError;

/// An error.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

impl Error {
    /// Creates a new error by wrapping it in [`eyre::Error`].
    pub fn new<T>(error: T) -> Error
    where
        T: StdError + Send + Sync + 'static,
    {
        Error {
            kind: ErrorKind::Other(eyre::Error::new(error)),
        }
    }

    /// `true` if an error is internal.
    pub fn is_internal(&self) -> bool {
        matches!(self.kind, ErrorKind::Other(_))
    }

    fn to_status_and_api_error(self) -> (StatusCode, ApiError) {
        let (status, error) = match self.kind {
            _err => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError {
                    message: "An internal server error occured".into(),
                },
            ),
        };

        (status, error)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.kind)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match &self.kind {
            ErrorKind::Other(err) => err.source(),
        }
    }
}

impl<T> From<T> for Error
where
    T: Into<ErrorKind>,
{
    fn from(value: T) -> Self {
        Error { kind: value.into() }
    }
}

/// An error kind.
#[derive(Debug, Display, From)]
pub enum ErrorKind {
    /// Some other internal error.
    #[from(ignore)]
    Other(eyre::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut internal_error = None;

        let (status, error) = if self.is_internal() {
            internal_error = Some(Error { kind: self.kind });
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError {
                    message: "An internal server error occured.".into(),
                },
            )
        } else {
            self.to_status_and_api_error()
        };

        let mut response = (status, Json(error)).into_response();
        if let Some(error) = internal_error {
            response.extensions_mut().insert(Arc::new(error));
        }
        response
    }
}
