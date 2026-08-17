use crate::client_ip::ClientIp;
use crate::duration::parse_time_value;
use crate::error::TimeBannerError;
use crate::render::{convert_png_to_ico, generate_favicon_png_bytes, render_time_response};
use crate::template::OutputForm;
use crate::utils::parse_path;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;

/// Root handler - redirects to current time in relative format.
pub async fn index_handler() -> impl IntoResponse {
    let epoch_now = chrono::Utc::now().timestamp();
    axum::response::Redirect::temporary(&format!("/relative/{epoch_now}"))
}

/// Handles `/relative/{time}` - displays time in relative format ("2 hours ago").
pub async fn relative_handler(
    Path(path): Path<String>,
) -> Result<impl IntoResponse, TimeBannerError> {
    let (raw_time, extension) = parse_path(&path);
    let time = parse_time_value(raw_time)?;
    Ok(render_time_response(time, OutputForm::Relative, extension))
}

/// Handles `/absolute/{time}` - displays time in absolute format ("2025-01-17 14:30:00 UTC").
pub async fn absolute_handler(
    Path(path): Path<String>,
) -> Result<impl IntoResponse, TimeBannerError> {
    let (raw_time, extension) = parse_path(&path);
    let time = parse_time_value(raw_time)?;
    Ok(render_time_response(time, OutputForm::Absolute, extension))
}

/// Handles `/{time}` - implicit absolute time display (same as absolute_handler).
pub async fn implicit_handler(
    Path(path): Path<String>,
) -> Result<impl IntoResponse, TimeBannerError> {
    let (raw_time, extension) = parse_path(&path);
    let time = parse_time_value(raw_time)?;
    Ok(render_time_response(time, OutputForm::Absolute, extension))
}

/// Handles `/favicon.ico` - generates a dynamic clock favicon showing the current time.
pub async fn favicon_handler(
    ClientIp(addr): ClientIp,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::info!(client_ip = %addr, "Favicon request");
    let now = chrono::Utc::now();

    let png_bytes = generate_favicon_png_bytes(now)?;

    let ico_bytes = convert_png_to_ico(&png_bytes).map_err(|e| {
        TimeBannerError::RenderError(format!("Failed to convert PNG to ICO: {}", e))
    })?;

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
    let now = chrono::Utc::now();

    let png_bytes = generate_favicon_png_bytes(now)?;

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
