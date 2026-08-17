use crate::client_ip::ClientIp;
use crate::error::TimeBannerError;
use crate::resolve::Resolution;
use crate::utils::parse_path;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use time_banner_core::parse_time_value;
use time_banner_render::{
    OutputForm, OutputFormat, RenderContext, convert_png_to_ico, generate_favicon_png_bytes,
    render_time, template::render_index_page,
};

/// Renders a time value, moving actual rasterization off the async executor
/// for PNG output. SVG output never rasterizes, so it stays inline.
async fn render_time_async(context: RenderContext) -> Result<Vec<u8>, TimeBannerError> {
    match context.output_format {
        OutputFormat::Svg => render_time(context).map_err(TimeBannerError::from),
        OutputFormat::Png => tokio::task::spawn_blocking(move || render_time(context))
            .await
            .map_err(|e| TimeBannerError::Internal(format!("render task panicked: {}", e)))?
            .map_err(TimeBannerError::from),
    }
}

/// Generates the favicon clock PNG off the async executor.
async fn generate_favicon_png_bytes_async(
    time: Timestamp,
    tz: TimeZone,
) -> Result<Vec<u8>, TimeBannerError> {
    tokio::task::spawn_blocking(move || generate_favicon_png_bytes(time, tz))
        .await
        .map_err(|e| TimeBannerError::Internal(format!("render task panicked: {}", e)))?
        .map_err(TimeBannerError::from)
}

/// Renders one banner, shared by every mode that draws a single time value.
async fn banner(
    path: String,
    output_form: OutputForm,
    resolution: Resolution,
) -> Result<impl IntoResponse, TimeBannerError> {
    let (raw_time, extension) = parse_path(&path);
    let output_format = OutputFormat::from_extension(extension);
    let mime_type = output_format.mime_type();

    let value = parse_time_value(raw_time, resolution.now, &resolution.tz)?;
    let bytes = render_time_async(RenderContext {
        value,
        output_form,
        output_format,
        tz: resolution.tz,
        now: resolution.now,
    })
    .await?;

    Ok((StatusCode::OK, [(header::CONTENT_TYPE, mime_type)], bytes))
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
    resolution: Resolution,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::debug!(path, "Relative time request");
    banner(path, OutputForm::Relative, resolution).await
}

/// Handles `/absolute/{time}` - displays time in absolute format ("2025-01-17 14:30:00 UTC").
pub async fn absolute_handler(
    Path(path): Path<String>,
    resolution: Resolution,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::debug!(path, "Absolute time request");
    banner(path, OutputForm::Absolute, resolution).await
}

/// Handles `/{time}` - implicit absolute time display (same as absolute_handler).
pub async fn implicit_handler(
    Path(path): Path<String>,
    resolution: Resolution,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::debug!(path, "Implicit time request");
    banner(path, OutputForm::Absolute, resolution).await
}

/// Handles `/favicon.ico` - generates a dynamic clock favicon showing the current time.
pub async fn favicon_handler(
    ClientIp(addr): ClientIp,
    resolution: Resolution,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::info!(client_ip = %addr, "Favicon request");

    let png_bytes = generate_favicon_png_bytes_async(resolution.now, resolution.tz).await?;
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
    resolution: Resolution,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::info!(client_ip = %addr, "Favicon PNG request");

    let png_bytes = generate_favicon_png_bytes_async(resolution.now, resolution.tz).await?;

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
