use crate::error::{get_error_response, TimeBannerError};
use axum::body::{Bytes, Full};
use axum::extract::Path;
use axum::http::header;
use axum::response::{Redirect, Response};
use axum::{http::StatusCode, response::IntoResponse};
use chrono::{DateTime, NaiveDateTime, Offset, Utc};

use crate::raster::Rasterizer;
use crate::template::{render_template, OutputForm, RenderContext};

fn split_on_extension(path: &str) -> Option<(&str, &str)> {
    let split = path.rsplit_once('.');
    if split.is_none() {
        return None;
    }

    // Check that the file is not a dotfile (.env)
    if split.unwrap().0.len() == 0 {
        return None;
    }

    Some(split.unwrap())
}

fn parse_path(path: &str) -> (&str, &str) {
    split_on_extension(path)
        .or_else(|| Some((path, "svg")))
        .unwrap()
}

fn handle_rasterize(data: String, extension: &str) -> Result<(&str, Bytes), TimeBannerError> {
    match extension {
        "svg" => Ok(("image/svg+xml", Bytes::from(data))),
        "png" => {
            let renderer = Rasterizer::new();
            let raw_image = renderer.render(data.into_bytes());
            if raw_image.is_err() {
                return Err(TimeBannerError::RasterizeError(
                    raw_image
                        .unwrap_err()
                        .message
                        .unwrap_or("Unknown error".to_string()),
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
    return Redirect::temporary(&*format!("/relative/{epoch_now}")).into_response();
}

pub async fn relative_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (raw_time, extension) = parse_path(path.as_str());
}

pub async fn fallback_handler() -> impl IntoResponse {
    return get_error_response(TimeBannerError::NotFound).into_response();
}

pub async fn absolute_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (raw_time, extension) = parse_path(path.as_str());
}

// basic handler that responds with a static string
pub async fn implicit_handler(Path(path): Path<String>) -> impl IntoResponse {
    // Get extension if available
    let (raw_time, extension) = parse_path(path.as_str());

    // Parse epoch
    let parsed_epoch = raw_time.parse::<i64>();
    if parsed_epoch.is_err() {
        return get_error_response(TimeBannerError::ParseError(
            "Input could not be parsed into integer.".to_string(),
        ))
        .into_response();
    }

    // Convert epoch to DateTime
    let naive_time = NaiveDateTime::from_timestamp_opt(parsed_epoch.unwrap(), 0);
    if naive_time.is_none() {
        return get_error_response(TimeBannerError::ParseError(
            "Input was not a valid DateTime".to_string(),
        ))
        .into_response();
    }

    let utc_time = DateTime::<Utc>::from_utc(naive_time.unwrap(), Utc);

    // Build context for rendering
    let context = RenderContext {
        output_form: OutputForm::Relative,
        value: utc_time,
        tz_offset: utc_time.offset().fix(),
        tz_name: "UTC",
        view: "basic",
    };

    let rendered_template = render_template(context);

    if rendered_template.is_err() {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Full::from(format!(
                "Template Could Not Be Rendered :: {}",
                rendered_template.err().unwrap()
            )))
            .unwrap()
            .into_response();
    }

    let rasterize_result = handle_rasterize(rendered_template.unwrap(), extension);
    match rasterize_result {
        Ok((mime_type, bytes)) => {
            (StatusCode::OK, [(header::CONTENT_TYPE, mime_type)], bytes).into_response()
        }
        Err(e) => get_error_response(e).into_response(),
    }
}
