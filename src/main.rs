use std::net::SocketAddr;

use crate::routes::{
    absolute_handler, fallback_handler, favicon_handler, favicon_png_handler, implicit_handler,
    index_handler, relative_handler,
};
use axum::{Router, http::HeaderValue, response::Response, routing::get};
use config::Configuration;
use dotenvy::dotenv;
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod abbr_tz;
mod client_ip;
mod config;
mod duration;
mod error;
mod raster;
mod render;
mod routes;
mod template;
mod utils;

#[tokio::main]
async fn main() {
    // Development-only: Parse dotenv files and expose them as environment variables
    #[cfg(debug_assertions)]
    dotenv().ok();

    let config = Configuration::load();

    // Initialize tracing with RUST_LOG support, falling back to config-based level
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.log_level().to_string()));

    if config.is_production() {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .compact()
            .with_env_filter(env_filter)
            .init();
    }

    let app = Router::new()
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
        .layer(axum::middleware::map_response(add_server_header));

    let addr = SocketAddr::from((config.socket_addr(), config.port));
    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
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
