use crate::error::{get_error_response, TimeBannerError};
use axum::body::{Body, Bytes};
use axum::extract::Path;
use axum::http::header;
use axum::response::{Redirect, Response};
use axum::{http::StatusCode, response::IntoResponse};
use chrono::{DateTime, NaiveDateTime, Offset, Utc};

use crate::raster::Rasterizer;
use crate::template::{render_template, OutputForm, RenderContext};

pub fn split_on_extension(path: &str) -> Option<(&str, &str)> {
    let split = path.rsplit_once('.')?;

    // Check that the file is not a dotfile (.env)
    if split.0.is_empty() {
        return None;
    }

    Some(split)
}

fn parse_path(path: &str) -> (&str, &str) {
    split_on_extension(path).or(Some((path, "svg"))).unwrap()
}

fn handle_rasterize(data: String, extension: &str) -> Result<(&str, Bytes), TimeBannerError> {
    match extension {
        "svg" => Ok(("image/svg+xml", Bytes::from(data))),
        "png" => {
            let renderer = Rasterizer::new();
            let raw_image = renderer.render(data.into_bytes());
            if let Err(err) = raw_image {
                return Err(TimeBannerError::RasterizeError(
                    err.message.unwrap_or_else(|| "Unknown error".to_string()),
                ));
            }

            Ok(("image/x-png", Bytes::from(raw_image.unwrap())))
        }
        _ => Err(TimeBannerError::RasterizeError(format!(
            "Unsupported extension: {}",
            extension
        ))),
    }
}

pub async fn index_handler() -> impl IntoResponse {
    let epoch_now = Utc::now().timestamp();

    Redirect::temporary(&format!("/relative/{epoch_now}")).into_response()
}

pub async fn relative_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (_raw_time, _extension) = parse_path(path.as_str());

    get_error_response(TimeBannerError::NotFound).into_response()
}

pub async fn fallback_handler() -> impl IntoResponse {
    get_error_response(TimeBannerError::NotFound).into_response()
}

pub async fn absolute_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (_raw_time, _extension) = parse_path(path.as_str());

    get_error_response(TimeBannerError::NotFound).into_response()
}

// basic handler that responds with a static string
pub async fn implicit_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (_raw_time, _extension) = parse_path(path.as_str());

    get_error_response(TimeBannerError::NotFound).into_response()
}

fn parse_epoch_into_datetime(epoch: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(epoch, 0)
}
