//! Provides the [`Form`] extractor.

use axum::{
    extract::FromRequest,
    response::{IntoResponse, Response},
};

use axum_valid::HasValidate;

use derive_more::Deref;

use crate::error::Error;

/// Query string/url-encoded form extractor.
#[derive(Deref, FromRequest)]
#[from_request(via(axum::Form), rejection(Error))]
pub struct Form<T>(pub T);

impl<T> HasValidate for Form<T> {
    type Validate = T;

    fn get_validate(&self) -> &Self::Validate {
        &self.0
    }
}

impl<T> IntoResponse for Form<T>
where
    axum::Form<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        axum::Form(self.0).into_response()
    }
}
