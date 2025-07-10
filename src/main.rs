use std::net::SocketAddr;

use crate::routes::{
    absolute_handler, fallback_handler, implicit_handler, index_handler, relative_handler,
};
use axum::{routing::get, Router};
use config::Configuration;
use dotenvy::dotenv;

mod abbr_tz;
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

    // Envy uses our Configuration struct to parse environment variables
    let config = envy::from_env::<Configuration>().expect("Failed to parse environment variables");

    // Initialize tracing
    tracing_subscriber::fmt()
        // With the log_level from our config
        .with_max_level(config.log_level())
        .init();

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/{path}", get(implicit_handler))
        .route("/rel/{path}", get(relative_handler))
        .route("/relative/{path}", get(relative_handler))
        .route("/absolute/{path}", get(absolute_handler))
        .route("/abs/{path}", get(absolute_handler))
        .fallback(fallback_handler);

    let addr = SocketAddr::from((config.socket_addr(), config.port));
    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
