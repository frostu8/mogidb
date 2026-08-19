//! Error handling.

use std::{
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use axum::{
    Json,
    extract::rejection::{FormRejection, JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use derive_more::{Display, From};
use mogidb_model::error::ApiError;

use crate::server::packet;

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
                    message: "resource does not exist".into(),
                },
            ),
            ErrorKind::Exists => (
                StatusCode::BAD_REQUEST,
                ApiError {
                    message: "a resource with that id already exists".into(),
                },
            ),
            ErrorKind::UndefinedLabel => (
                StatusCode::BAD_REQUEST,
                ApiError {
                    message: "server label not defined".into(),
                },
            ),
            ErrorKind::RemoteExists(remote) => (
                StatusCode::BAD_REQUEST,
                ApiError {
                    message: format!("remote {} already exists", remote),
                },
            ),
            _err => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError {
                    message: "an internal server error occured".into(),
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
    /// A Form rejection.
    #[display("{_0}")]
    Form(FormRejection),
    /// An error occured during SRB2 communications.
    #[display("{_0}")]
    Srb2Packet(packet::Error),
    /// A label was not given, and an attempt to generate one failed.
    #[display("server label not defined")]
    UndefinedLabel,
    /// A resource was not found.
    #[display("entity not found")]
    NotFound,
    /// A resource with that identifier already exists.
    #[display("entity already exists")]
    Exists,
    /// An attempt was made to create a server with a remote already used.
    #[display("remote server {_0} already exists")]
    RemoteExists(String),
    /// Cannot assign a non-existant server (or list of servers) to a format.
    #[display("server(s) with ids {_0:?} do not exist")]
    InvalidServerIds(Vec<i32>),
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
