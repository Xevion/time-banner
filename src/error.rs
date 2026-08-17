use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;

/// Application-specific errors that can occur during request processing.
#[derive(Debug, thiserror::Error)]
pub enum TimeBannerError {
    /// Input parsing errors (invalid time formats, bad parameters, etc.)
    #[error("Parse error: {0}")]
    ParseError(String),
    /// Template rendering failures
    #[error("Render error: {0}")]
    RenderError(String),
    /// SVG to PNG conversion failures
    #[error("Rasterize error: {0}")]
    RasterizeError(String),
    /// 404 Not Found
    #[error("The requested resource was not found")]
    NotFound,
}

/// JSON error response format for HTTP clients.
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

impl IntoResponse for TimeBannerError {
    fn into_response(self) -> Response {
        let (status, error_name, message) = match &self {
            TimeBannerError::ParseError(msg) => {
                (StatusCode::BAD_REQUEST, "ParseError", msg.clone())
            }
            TimeBannerError::RenderError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "RenderError",
                msg.clone(),
            ),
            TimeBannerError::RasterizeError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "RasterizeError",
                msg.clone(),
            ),
            TimeBannerError::NotFound => (StatusCode::NOT_FOUND, "NotFound", self.to_string()),
        };

        (
            status,
            Json(ErrorResponse {
                error: error_name.to_string(),
                message,
            }),
        )
            .into_response()
    }
}
