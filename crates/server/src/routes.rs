use crate::client_ip::ClientIp;
use crate::error::TimeBannerError;
use crate::resolve::Resolution;
use crate::utils::parse_path;
use axum::extract::Path;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use time_banner_core::parse_time_value;
use time_banner_render::{
    OutputForm, OutputFormat, RenderContext, convert_png_to_ico, generate_favicon_png_bytes,
    template::render_index_page,
};

/// Renders a time value, moving actual rasterization off the async executor
/// for PNG output. SVG output never rasterizes, so it stays inline.
async fn render_time_async(context: RenderContext) -> Result<Vec<u8>, TimeBannerError> {
    match context.output_format {
        OutputFormat::Svg => context.render().map_err(TimeBannerError::from),
        OutputFormat::Png => tokio::task::spawn_blocking(move || context.render())
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
    let output_format = OutputFormat::from(extension);
    let mime_type = output_format.mime_type();

    // Only `relative` renders words today; `Content-Language` and
    // `Vary: Accept-Language` are scoped to responses that actually depend
    // on the negotiated locale, unlike `Timezone`, which SPEC 6 requires on
    // every response regardless of mode.
    let renders_words = matches!(output_form, OutputForm::Relative);
    let locale = resolution.locale.clone();

    let value = parse_time_value(raw_time, resolution.now, &resolution.tz)?;
    let bytes = render_time_async(RenderContext {
        value,
        output_form,
        output_format,
        tz: resolution.tz,
        now: resolution.now,
        format: resolution.format,
        locale: time_banner_render::locale::language_for(&locale),
    })
    .await?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime_type));
    if renders_words && let Ok(value) = HeaderValue::from_str(&locale) {
        headers.insert(header::CONTENT_LANGUAGE, value);
        headers.append(header::VARY, HeaderValue::from_static("Accept-Language"));
    }

    Ok((StatusCode::OK, headers, bytes))
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
        [
            (header::CONTENT_TYPE, "image/x-icon"),
            (header::CACHE_CONTROL, "no-store"),
        ],
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
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        png_bytes,
    ))
}

/// Fallback handler for unmatched routes.
pub async fn fallback_handler() -> TimeBannerError {
    TimeBannerError::NotFound
}
