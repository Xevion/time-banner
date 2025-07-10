use crate::duration::parse_time_value;
use crate::error::{get_error_response, TimeBannerError};
use crate::render::render_time_response;
use crate::template::OutputForm;
use crate::utils::parse_path;
use axum::extract::ConnectInfo;
use axum::extract::Path;
use axum::response::IntoResponse;
use std::net::SocketAddr;

/// Root handler - redirects to current time in relative format.
pub async fn index_handler() -> impl IntoResponse {
    let epoch_now = chrono::Utc::now().timestamp();

    axum::response::Redirect::temporary(&format!("/relative/{epoch_now}")).into_response()
}

/// Handles `/relative/{time}` - displays time in relative format ("2 hours ago").
pub async fn relative_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (raw_time, extension) = parse_path(&path);

    let time = match parse_time_value(raw_time) {
        Ok(t) => t,
        Err(e) => return get_error_response(e).into_response(),
    };

    render_time_response(time, OutputForm::Relative, extension).into_response()
}

/// Handles `/absolute/{time}` - displays time in absolute format ("2025-01-17 14:30:00 UTC").
pub async fn absolute_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (raw_time, extension) = parse_path(&path);

    let time = match parse_time_value(raw_time) {
        Ok(t) => t,
        Err(e) => return get_error_response(e).into_response(),
    };

    render_time_response(time, OutputForm::Absolute, extension).into_response()
}

/// Handles `/{time}` - implicit absolute time display (same as absolute_handler).
pub async fn implicit_handler(Path(path): Path<String>) -> impl IntoResponse {
    let (raw_time, extension) = parse_path(&path);

    let time = match parse_time_value(raw_time) {
        Ok(t) => t,
        Err(e) => return get_error_response(e).into_response(),
    };

    render_time_response(time, OutputForm::Absolute, extension).into_response()
}

/// Handles `/favicon.ico` - generates a dynamic clock favicon showing the current time.
///
/// Logs the client IP address and returns a PNG image of an analog clock.
pub async fn favicon_handler(ConnectInfo(addr): ConnectInfo<SocketAddr>) -> impl IntoResponse {
    let now = chrono::Utc::now();

    // Log the IP address for the favicon request
    tracing::info!("Favicon request from IP: {}", addr.ip());

    // Generate clock favicon showing current time
    render_time_response(now, OutputForm::Clock, "png").into_response()
}

/// Fallback handler for unmatched routes.
pub async fn fallback_handler() -> impl IntoResponse {
    get_error_response(TimeBannerError::NotFound).into_response()
}
