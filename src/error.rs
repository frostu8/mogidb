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
            kind: ErrorKind::Other(eyre::Report::new(error)),
            message: None,
        }
    }

    /// `true` if an error is internal.
    pub fn is_internal(&self) -> bool {
        matches!(self.kind, ErrorKind::Other(_))
    }

    /// Creates a not found error.
    pub fn not_found<T>(err: T) -> Error
    where
        T: Into<NotFound>,
    {
        Error::from(ErrorKind::NotFound(err.into()))
    }

    /// Creates an exists error.
    pub fn conflict<T>(msg: T) -> Error
    where
        T: Display,
    {
        Error::from(ErrorKind::Conflict).with_message(msg)
    }

    pub fn message<T>(message: T) -> Error
    where
        T: Display,
    {
        Error::from(ErrorKind::Other(eyre::Error::msg(message.to_string())))
    }

    /// Checks if an error was the result of an entity that couldn't be found.
    pub fn is_not_found(&self) -> bool {
        matches!(self.kind, ErrorKind::NotFound(_))
    }

    pub fn with_message<T>(self, msg: T) -> Error
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
            ErrorKind::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                ApiError {
                    message: "request was unauthorized".into(),
                },
            ),
            ErrorKind::NotFound(err) => (
                StatusCode::NOT_FOUND,
                ApiError {
                    message: err.to_string(),
                },
            ),
            ErrorKind::NoActiveEvent => (
                StatusCode::NOT_FOUND,
                ApiError {
                    message: "no active event in the room".into(),
                },
            ),
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
            err @ ErrorKind::LabelInUse(_)
            | err @ ErrorKind::Conflict
            | err @ ErrorKind::RemoteExists(_)
            | err @ ErrorKind::UserInEvent(_)
            | err @ ErrorKind::EventConcluded
            | err @ ErrorKind::EventFull
            | err @ ErrorKind::EventTeamsUnassignable
            | err @ ErrorKind::EventRejected => (
                StatusCode::CONFLICT,
                ApiError {
                    message: err.to_string(),
                },
            ),
            err @ ErrorKind::InvalidServerIds(_)
            | err @ ErrorKind::UndefinedLabel
            | err @ ErrorKind::NotPlaying(_)
            | err @ ErrorKind::NoSuchFormat(_)
            | err @ ErrorKind::NoSuchServer(_)
            | err @ ErrorKind::NoFormatAssigned
            | err @ ErrorKind::NoSuchUser(_) => (
                StatusCode::BAD_REQUEST,
                ApiError {
                    message: err.to_string(),
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
    /// Request was unauthorized.
    Unauthorized,
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
    #[display("{_0}")]
    NotFound(NotFound),
    /// The room has no active events.
    #[display("no active event in the room")]
    NoActiveEvent,
    /// A resource with that identifier already exists.
    #[display("entity already exists")]
    Conflict,
    /// An attempt was made to create a server with a remote already used.
    #[display("remote server {_0} already exists")]
    #[from(ignore)]
    RemoteExists(String),
    /// Cannot use a label that is already in-use.
    #[display("label \"{_0}\" in use")]
    #[from(ignore)]
    LabelInUse(String),
    /// Cannot assign a non-existant server (or list of servers) to a format.
    #[display("server(s) with ids {_0:?} do not exist")]
    InvalidServerIds(Vec<i32>),
    /// Cannot join as a non-existant user.
    #[display("user {_0} does not exist")]
    #[from(ignore)]
    NoSuchUser(String),
    /// Cannot assign a non-existant format to an event.
    #[display("format {_0} does not exist in the event's room")]
    #[from(ignore)]
    NoSuchFormat(i32),
    /// Cannot assign a non-existant server to an event.
    #[display("server {_0} does not exist in the event's room")]
    #[from(ignore)]
    NoSuchServer(i32),
    /// The user is already in the event.
    #[display("user with id {_0} already in event")]
    #[from(ignore)]
    UserInEvent(String),
    /// The user is not playing.
    #[display("user with id {_0} not playing")]
    #[from(ignore)]
    NotPlaying(String),
    /// The event cannot accept more participants because it is rejected or too
    /// far into its lifecycle.
    #[display("event is no longer accepting participants")]
    EventConcluded,
    /// The event cannot accept more participants because it is full.
    #[display("event is full")]
    EventFull,
    /// Teams cannot be assigned because the event has not format.
    #[display("no format is assigned to the event")]
    NoFormatAssigned,
    /// An event's teams cannot be assigned because the event's status is not
    /// [`EventStatus::Ongoing`].
    #[display("cannot assign teams")]
    EventTeamsUnassignable,
    /// A mutable operation was blocked because the event is rejected.
    #[display("event is rejected")]
    EventRejected,
    /// The server ran out of IDs while generating a new entity.
    #[display("{_0}")]
    IdsExhausted(crate::short_id::IdsExhausted),
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

/// A resource was not found.
#[derive(Debug, Display)]
pub enum NotFound {
    #[display("event with id \"{_0}\" not found")]
    Event(String),
    #[display("server with id {_0} not found")]
    Server(i32),
    #[display("event format with id {_0} not found")]
    Format(i32),
    #[display("guild with discord id {_0} not found")]
    Guild(i64),
    #[display("room with discord id {_0} not found")]
    Room(i64),
    #[display("user with id \"{_0}\" not found")]
    User(String),
    #[display("object not found")]
    Other,
}

/// Result extension methods.
pub trait ResultExt<T> {
    /// Transforms a `Result<T, Error>` into a `Result<Option<T>, Error>`,
    /// producing a `None` when a [`NotFound`] error is found.
    fn or_none(self) -> Result<Option<T>, Error>;
}

impl<T> ResultExt<T> for Result<T, Error> {
    fn or_none(self) -> Result<Option<T>, Error> {
        match self {
            Ok(inner) => Ok(Some(inner)),
            Err(err) if err.is_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }
}
