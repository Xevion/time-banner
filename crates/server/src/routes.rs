use crate::client_ip::ClientIp;
use crate::error::TimeBannerError;
use crate::utils::parse_path;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse};
use jiff::Timestamp;
use time_banner_core::parse_time_value;
use time_banner_render::{
    OutputForm, OutputFormat, convert_png_to_ico, generate_favicon_png_bytes, render_time,
    template::render_index_page,
};

/// Renders a time value, moving actual rasterization off the async executor
/// for PNG output. SVG output never rasterizes, so it stays inline.
async fn render_time_async(
    time: Timestamp,
    now: Timestamp,
    output_form: OutputForm,
    output_format: OutputFormat,
) -> Result<Vec<u8>, TimeBannerError> {
    match output_format {
        OutputFormat::Svg => {
            render_time(time, now, output_form, output_format).map_err(TimeBannerError::from)
        }
        OutputFormat::Png => {
            tokio::task::spawn_blocking(move || render_time(time, now, output_form, output_format))
                .await
                .map_err(|e| TimeBannerError::Internal(format!("render task panicked: {}", e)))?
                .map_err(TimeBannerError::from)
        }
    }
}

/// Generates the favicon clock PNG off the async executor.
async fn generate_favicon_png_bytes_async(time: Timestamp) -> Result<Vec<u8>, TimeBannerError> {
    tokio::task::spawn_blocking(move || generate_favicon_png_bytes(time))
        .await
        .map_err(|e| TimeBannerError::Internal(format!("render task panicked: {}", e)))?
        .map_err(TimeBannerError::from)
}

/// Root handler - renders a minimal demo page with live examples and usage docs.
pub async fn index_handler() -> Result<impl IntoResponse, TimeBannerError> {
    let html = render_index_page(Timestamp::now())
        .map_err(|e| TimeBannerError::RenderError(format!("Failed to render index page: {}", e)))?;
    Ok(Html(html))
}

/// Handles `/relative/{time}` - displays time in relative format ("2 hours ago").
pub async fn relative_handler(
    Path(path): Path<String>,
) -> Result<impl IntoResponse, TimeBannerError> {
    let (raw_time, extension) = parse_path(&path);
    tracing::debug!(raw_time, extension, "Relative time request");
    let now = Timestamp::now();
    let time = parse_time_value(raw_time, now)?;
    let output_format = OutputFormat::from_extension(extension);
    let bytes = render_time_async(time, now, OutputForm::Relative, output_format.clone()).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, output_format.mime_type())],
        bytes,
    ))
}

/// Handles `/absolute/{time}` - displays time in absolute format ("2025-01-17 14:30:00 UTC").
pub async fn absolute_handler(
    Path(path): Path<String>,
) -> Result<impl IntoResponse, TimeBannerError> {
    let (raw_time, extension) = parse_path(&path);
    tracing::debug!(raw_time, extension, "Absolute time request");
    let now = Timestamp::now();
    let time = parse_time_value(raw_time, now)?;
    let output_format = OutputFormat::from_extension(extension);
    let bytes = render_time_async(time, now, OutputForm::Absolute, output_format.clone()).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, output_format.mime_type())],
        bytes,
    ))
}

/// Handles `/{time}` - implicit absolute time display (same as absolute_handler).
pub async fn implicit_handler(
    Path(path): Path<String>,
) -> Result<impl IntoResponse, TimeBannerError> {
    let (raw_time, extension) = parse_path(&path);
    tracing::debug!(raw_time, extension, "Implicit time request");
    let now = Timestamp::now();
    let time = parse_time_value(raw_time, now)?;
    let output_format = OutputFormat::from_extension(extension);
    let bytes = render_time_async(time, now, OutputForm::Absolute, output_format.clone()).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, output_format.mime_type())],
        bytes,
    ))
}

/// Handles `/favicon.ico` - generates a dynamic clock favicon showing the current time.
pub async fn favicon_handler(
    ClientIp(addr): ClientIp,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::info!(client_ip = %addr, "Favicon request");
    let now = Timestamp::now();

    let png_bytes = generate_favicon_png_bytes_async(now).await?;
    let ico_bytes = convert_png_to_ico(&png_bytes)?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/x-icon")],
        ico_bytes,
    ))
}

/// Handles `/favicon.png` - generates a dynamic clock favicon showing the current time.
pub async fn favicon_png_handler(
    ClientIp(addr): ClientIp,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::info!(client_ip = %addr, "Favicon PNG request");
    let now = Timestamp::now();

    let png_bytes = generate_favicon_png_bytes_async(now).await?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/png")],
        png_bytes,
    ))
}

/// Fallback handler for unmatched routes.
pub async fn fallback_handler() -> TimeBannerError {
    TimeBannerError::NotFound
}
