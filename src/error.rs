use axum::{http::StatusCode, response::Json};
use serde::Serialize;

#[derive(Debug)]
pub enum TimeBannerError {
    ParseError(String),
    RenderError(String),
    RasterizeError(String),
    NotFound,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    error: String,
    message: String,
}

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
