use crate::duration::parse_time_value;
use crate::error::{get_error_response, TimeBannerError};
use crate::render::render_time_response;
use crate::template::OutputForm;
use crate::utils::parse_path;
use axum::extract::Path;
use axum::response::IntoResponse;

pub async fn index_handler() -> impl IntoResponse {
    let epoch_now = chrono::Utc::now().timestamp();

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
