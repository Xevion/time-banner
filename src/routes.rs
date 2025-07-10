use crate::duration::parse_duration;
use crate::error::{get_error_response, TimeBannerError};
use axum::body::Bytes;
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use chrono::{DateTime, FixedOffset, Utc};

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
    split_on_extension(path).unwrap_or((path, "svg"))
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

            Ok(("image/png", Bytes::from(raw_image.unwrap())))
        }
        _ => Err(TimeBannerError::RasterizeError(format!(
            "Unsupported extension: {}",
            extension
        ))),
    }
}

fn parse_epoch_into_datetime(epoch: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(epoch, 0)
}

fn parse_time_value(raw_time: &str) -> Result<DateTime<Utc>, TimeBannerError> {
    // Handle relative time values (starting with + or -, or duration strings like "1y2d")
    if raw_time.starts_with('+') || raw_time.starts_with('-') {
        let now = Utc::now();

        // Try parsing as simple offset seconds first
        if let Ok(offset_seconds) = raw_time.parse::<i64>() {
            return Ok(now + chrono::Duration::seconds(offset_seconds));
        }

        // Try parsing as duration string (e.g., "+1y2d", "-3h30m")
        if let Ok(duration) = parse_duration(raw_time) {
            return Ok(now + duration);
        }

        return Err(TimeBannerError::ParseError(format!(
            "Could not parse relative time: {}",
            raw_time
        )));
    }

    // Try to parse as epoch timestamp
    if let Ok(epoch) = raw_time.parse::<i64>() {
        return parse_epoch_into_datetime(epoch)
            .ok_or_else(|| TimeBannerError::ParseError("Invalid timestamp".to_string()));
    }

    Err(TimeBannerError::ParseError(format!(
        "Could not parse time value: {}",
        raw_time
    )))
}

fn render_time_response(
    time: DateTime<Utc>,
    output_form: OutputForm,
    extension: &str,
) -> impl IntoResponse {
    // Build context for rendering
    let context = RenderContext {
        output_form,
        value: time,
        tz_offset: FixedOffset::east_opt(0).unwrap(), // UTC offset
        tz_name: "UTC",
        view: "basic",
    };

    // Render template
    let rendered_template = match render_template(context) {
        Ok(template) => template,
        Err(e) => {
            return get_error_response(TimeBannerError::RenderError(format!(
                "Template rendering failed: {}",
                e
            )))
            .into_response()
        }
    };

    // Handle rasterization
    match handle_rasterize(rendered_template, extension) {
        Ok((mime_type, bytes)) => {
            (StatusCode::OK, [(header::CONTENT_TYPE, mime_type)], bytes).into_response()
        }
        Err(e) => get_error_response(e).into_response(),
    }
}

pub async fn index_handler() -> impl IntoResponse {
    let epoch_now = Utc::now().timestamp();

    axum::response::Redirect::temporary(&format!("/relative/{epoch_now}")).into_response()
}

pub async fn relative_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (raw_time, extension) = parse_path(&path);

    let time = match parse_time_value(raw_time) {
        Ok(t) => t,
        Err(e) => return get_error_response(e).into_response(),
    };

    render_time_response(time, OutputForm::Relative, extension).into_response()
}

pub async fn absolute_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (raw_time, extension) = parse_path(&path);

    let time = match parse_time_value(raw_time) {
        Ok(t) => t,
        Err(e) => return get_error_response(e).into_response(),
    };

    render_time_response(time, OutputForm::Absolute, extension).into_response()
}

// Handler for implicit absolute time (no /absolute/ prefix)
pub async fn implicit_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (raw_time, extension) = parse_path(&path);

    let time = match parse_time_value(raw_time) {
        Ok(t) => t,
        Err(e) => return get_error_response(e).into_response(),
    };

    render_time_response(time, OutputForm::Absolute, extension).into_response()
}

pub async fn fallback_handler() -> impl IntoResponse {
    get_error_response(TimeBannerError::NotFound).into_response()
}
