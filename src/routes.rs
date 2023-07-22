use std::io::Read;
use std::process::Output;
use std::time::SystemTime;

use axum::{http::StatusCode, response::IntoResponse};
use axum::body::{Bytes, Full};
use axum::extract::{Path};
use axum::http::{header};
use axum::response::Response;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Offset, Utc};


use crate::parse::split_on_extension;
use crate::raster::Rasterizer;
use crate::template::{OutputForm, render_template, RenderContext};


fn parse_path(path: &str) -> (&str, &str) {
    split_on_extension(path)
        .or_else(|| Some((path, "svg")))
        .unwrap()
}

enum TimeBannerError {
    ParseError(String),
    RenderError(String),
    RasterizeError(String),
}

fn handle_rasterize(data: String, extension: &str) -> Result<(&str, Bytes), TimeBannerError> {
    match extension {
        "svg" => Ok(("image/svg+xml", Bytes::from(data))),
        "png" => {
            let renderer = Rasterizer::new();
            let raw_image = renderer.render(data.into_bytes());
            if raw_image.is_err() {
                return Err(TimeBannerError::RasterizeError(raw_image.unwrap_err().message.unwrap_or("Unknown error".to_string())));
            }

            Ok(("image/x-png", Bytes::from(raw_image.unwrap())))
        }
        _ => Err(TimeBannerError::ParseError(format!("Unsupported extension: {}", extension)))
    }
}

pub async fn relative_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (raw_time, extension) = parse_path(path.as_str());
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
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Full::from(format!("Failed to parse epoch :: {}", parsed_epoch.unwrap_err())))
            .unwrap();
    }

    // Convert epoch to DateTime
    let naive_time = NaiveDateTime::from_timestamp_opt(parsed_epoch.unwrap(), 0);
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
            .body(Full::from(
                format!("Template Could Not Be Rendered :: {}", rendered_template.err().unwrap())
            ))
            .unwrap();
    }

    let rasterize_result = handle_rasterize(rendered_template.unwrap(), extension);
    match rasterize_result {
        Ok((mime_type, bytes)) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .body(Full::from(bytes))
                .unwrap()
        }
        Err(e) => {
            match e {
                TimeBannerError::RenderError(msg) => Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Full::from(format!("Template Could Not Be Rendered :: {}", msg)))
                    .unwrap(),
                TimeBannerError::ParseError(msg) => Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Full::from(format!("Failed to parse epoch :: {}", msg)))
                    .unwrap(),
                TimeBannerError::RasterizeError(msg) => Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Full::from(format!("Failed to rasterize :: {}", msg)))
                    .unwrap(),
            }
        }
    }
}