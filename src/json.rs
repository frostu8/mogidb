//! JSON extractors and responders.

use axum::{
    extract::FromRequest,
    response::{IntoResponse, Response},
};
use axum_valid::HasValidate;
use derive_more::Deref;

use crate::error::Error;

/// JSON extractor and responder.
#[derive(Deref, FromRequest)]
#[from_request(via(axum::Json), rejection(Error))]
pub struct Json<T>(pub T);

impl<T> HasValidate for Json<T> {
    type Validate = T;

    fn get_validate(&self) -> &Self::Validate {
        &self.0
    }
}

impl<T> IntoResponse for Json<T>
where
    axum::Json<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}
