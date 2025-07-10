use axum::{http::StatusCode, response::Json};
use serde::Serialize;

/// Application-specific errors that can occur during request processing.
#[derive(Debug)]
pub enum TimeBannerError {
    /// Input parsing errors (invalid time formats, bad parameters, etc.)
    ParseError(String),
    /// Template rendering failures
    RenderError(String),
    /// SVG to PNG conversion failures  
    RasterizeError(String),
    /// 404 Not Found
    NotFound,
}

/// JSON error response format for HTTP clients.
#[derive(Serialize)]
pub struct ErrorResponse {
    error: String,
    message: String,
}

/// Converts application errors into standardized HTTP responses with JSON bodies.
///
/// Returns appropriate status codes:
/// - 400 Bad Request: ParseError
/// - 500 Internal Server Error: RenderError, RasterizeError  
/// - 404 Not Found: NotFound
pub fn get_error_response(error: TimeBannerError) -> (StatusCode, Json<ErrorResponse>) {
    match error {
        TimeBannerError::ParseError(msg) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ParseError".to_string(),
                message: msg,
            }),
        ),
        TimeBannerError::RenderError(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "RenderError".to_string(),
                message: msg,
            }),
        ),
        TimeBannerError::RasterizeError(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "RasterizeError".to_string(),
                message: msg,
            }),
        ),
        TimeBannerError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "NotFound".to_string(),
                message: "The requested resource was not found".to_string(),
            }),
        ),
    }
}
