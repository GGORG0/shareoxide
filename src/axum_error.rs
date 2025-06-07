use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use color_eyre::Report;
use tracing::error;

#[derive(Debug)]
pub struct AxumError(pub Report);

impl<E: Into<Report>> From<E> for AxumError {
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for AxumError {
    fn into_response(self) -> Response {
        error!(error = ?self.0, "An error occurred in an axum handler");
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
    }
}

pub type AxumResult<T, E = AxumError> = std::result::Result<T, E>;
