use std::time::Duration;

use axum::Router;
use axum::http::HeaderValue;
use axum::response::Response;
use axum::routing::get;
use tower_http::compression::CompressionLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::resolve::resolve_request;
use crate::routes::{
    absolute_handler, fallback_handler, favicon_handler, favicon_png_handler, implicit_handler,
    index_handler, relative_handler,
};

pub mod client_ip;
pub mod config;
pub mod error;
mod locale;
pub mod resolve;
pub mod routes;
pub mod utils;

/// Builds the application router, including routes and shared middleware.
/// Split out from `main` so integration tests can exercise it without
/// binding a socket.
pub fn build_router() -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/health", get(health_handler))
        .route("/favicon.ico", get(favicon_handler))
        .route("/favicon.png", get(favicon_png_handler))
        .route("/{path}", get(implicit_handler))
        .route("/rel/{path}", get(relative_handler))
        .route("/relative/{path}", get(relative_handler))
        .route("/absolute/{path}", get(absolute_handler))
        .route("/abs/{path}", get(absolute_handler))
        .fallback(fallback_handler)
        .layer((
            TraceLayer::new_for_http(),
            TimeoutLayer::with_status_code(
                axum::http::StatusCode::GATEWAY_TIMEOUT,
                Duration::from_secs(10),
            ),
            CompressionLayer::new()
                .zstd(true)
                .br(true)
                .gzip(true)
                .quality(tower_http::CompressionLevel::Fastest),
        ))
        .layer(axum::middleware::from_fn(resolve_request))
        .layer(axum::middleware::map_response(add_server_header))
}

/// Middleware to add server header with application version
async fn add_server_header(mut response: Response) -> Response {
    let version = env!("CARGO_PKG_VERSION");
    let server_header = format!("time-banner/{}", version);

    if let Ok(header_value) = HeaderValue::from_str(&server_header) {
        response.headers_mut().insert("Server", header_value);
    }

    response
}

/// Health check handler - reports OK along with the running version.
async fn health_handler() -> String {
    format!("OK time-banner/{}", env!("CARGO_PKG_VERSION"))
}
