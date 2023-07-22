use axum::body::Full;
use axum::http::{StatusCode};
use axum::response::{IntoResponse, Response};

pub enum TimeBannerError {
    ParseError(String),
    RenderError(String),
    RasterizeError(String),
}

pub fn get_error_response(error: TimeBannerError) -> (StatusCode, Full<String>) {
    match error {
        TimeBannerError::RenderError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, Full::from(format!("Template Could Not Be Rendered :: {}", msg))),
        TimeBannerError::ParseError(msg) => (StatusCode::BAD_REQUEST, Full::from(format!("Failed to parse epoch :: {}", msg))),
        TimeBannerError::RasterizeError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, Full::from(format!("Failed to rasterize :: {}", msg)))
    }
}