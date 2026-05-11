//! Error handling.

use std::{
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use axum::{
    Json,
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use derive_more::{Display, From};
use mogidb_model::error::ApiError;

/// An error.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: Option<String>,
}

impl Error {
    /// Creates a new error by wrapping it in [`eyre::Error`].
    pub fn new<T>(error: T) -> Error
    where
        T: StdError + Send + Sync + 'static,
    {
        Error {
            kind: ErrorKind::Other(eyre::Error::new(error)),
            message: None,
        }
    }

    /// `true` if an error is internal.
    pub fn is_internal(&self) -> bool {
        matches!(self.kind, ErrorKind::Other(_))
    }

    /// Creates a not found error.
    pub fn not_found<T>(msg: T) -> Error
    where
        T: Display,
    {
        Error::from(ErrorKind::NotFound).message(msg)
    }

    /// Creates an exists error.
    pub fn exists<T>(msg: T) -> Error
    where
        T: Display,
    {
        Error::from(ErrorKind::Exists).message(msg)
    }

    pub fn message<T>(self, msg: T) -> Error
    where
        T: Display,
    {
        Error {
            message: Some(msg.to_string()),
            ..self
        }
    }

    fn to_status_and_api_error(self) -> (StatusCode, ApiError) {
        let (status, mut error) = match self.kind {
            ErrorKind::InvalidValue(err) => (
                StatusCode::BAD_REQUEST,
                ApiError {
                    message: err.to_string(),
                },
            ),
            ErrorKind::Json(err) => (
                StatusCode::BAD_REQUEST,
                ApiError {
                    message: err.to_string(),
                },
            ),
            ErrorKind::NotFound => (
                StatusCode::NOT_FOUND,
                ApiError {
                    message: "Resource does not exist".into(),
                },
            ),
            ErrorKind::Exists => (
                StatusCode::BAD_REQUEST,
                ApiError {
                    message: "A resource with that id already exists".into(),
                },
            ),
            _err => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError {
                    message: "An internal server error occured".into(),
                },
            ),
        };

        if let Some(message) = self.message {
            error.message = message;
        }

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
            ErrorKind::InvalidValue(err) => Some(err),
            ErrorKind::Other(err) => err.source(),
            _ => None,
        }
    }
}

impl<T> From<T> for Error
where
    T: Into<ErrorKind>,
{
    fn from(value: T) -> Self {
        Error {
            kind: value.into(),
            message: None,
        }
    }
}

/// An error kind.
#[derive(Debug, Display, From)]
pub enum ErrorKind {
    /// An invalid value was given.
    #[display("{_0}")]
    InvalidValue(garde::Report),
    /// A JSON rejection.
    #[display("{_0}")]
    Json(JsonRejection),
    /// A resource was not found.
    NotFound,
    /// A resource with that identifier already exists.
    Exists,
    /// Some other internal error.
    #[from(ignore)]
    Other(eyre::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut internal_error = None;

        let (status, error) = if self.is_internal() {
            internal_error = Some(Error {
                kind: self.kind,
                message: self.message,
            });
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
