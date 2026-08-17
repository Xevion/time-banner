use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;

/// Application-specific errors that can occur during request processing.
#[derive(Debug, thiserror::Error)]
pub enum TimeBannerError {
    /// Input parsing errors (invalid time formats, bad parameters, etc.)
    #[error("Parse error: {0}")]
    ParseError(#[from] time_banner_core::ParseError),
    /// Template rendering failures
    #[error("Render error: {0}")]
    RenderError(String),
    /// SVG to PNG conversion failures
    #[error("Rasterize error: {0}")]
    RasterizeError(String),
    /// 404 Not Found
    #[error("The requested resource was not found")]
    NotFound,
    /// Internal errors not otherwise classified, such as extractor failures.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// JSON error response format for HTTP clients.
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

impl From<time_banner_render::RenderError> for TimeBannerError {
    fn from(e: time_banner_render::RenderError) -> Self {
        use time_banner_render::RenderError;
        let message = e.to_string();
        match e {
            RenderError::Template { .. } => TimeBannerError::RenderError(message),
            RenderError::Rasterize { .. } => TimeBannerError::RasterizeError(message),
            RenderError::Encode { .. } => TimeBannerError::RenderError(message),
        }
    }
}

impl IntoResponse for TimeBannerError {
    fn into_response(self) -> Response {
        let (status, error_name) = match &self {
            TimeBannerError::ParseError(_) => (StatusCode::BAD_REQUEST, "ParseError"),
            TimeBannerError::RenderError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "RenderError"),
            TimeBannerError::RasterizeError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "RasterizeError")
            }
            TimeBannerError::NotFound => (StatusCode::NOT_FOUND, "NotFound"),
            TimeBannerError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal"),
        };

        (
            status,
            Json(ErrorResponse {
                error: error_name.to_string(),
                message: self.to_string(),
            }),
        )
            .into_response()
    }
}
