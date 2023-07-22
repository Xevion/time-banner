use axum::body::Full;
use axum::http::{StatusCode};
use axum::Json;
use axum::response::{IntoResponse, Response};
use serde::{Serialize, Deserialize};

pub enum TimeBannerError {
    ParseError(String),
    RenderError(String),
    RasterizeError(String),
}

#[derive(Serialize, Deserialize)]
pub struct ErrorResponse {
    code: u16,
    message: String,
}

pub fn get_error_response(error: TimeBannerError) -> (StatusCode, Json<ErrorResponse>) {
    let (code, message) = match error {
        TimeBannerError::RenderError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("RenderError :: {}", msg)),
        TimeBannerError::ParseError(msg) => (StatusCode::BAD_REQUEST, format!("ParserError :: {}", msg)),
        TimeBannerError::RasterizeError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("RasertizeError :: {}", msg))
    };

    (code, Json(ErrorResponse { code: code.as_u16(), message }))
}