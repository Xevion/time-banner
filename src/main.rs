use std::net::SocketAddr;

use axum::{Router, routing::get};
use axum::response::IntoResponse;
use dotenvy::dotenv;
use config::Configuration;
use crate::error::{get_error_response, TimeBannerError};
use crate::routes::{relative_handler, implicit_handler, absolute_handler, index_handler, fallback_handler};

mod config;
mod raster;
mod abbr;
mod routes;
mod parse;
mod template;
mod error;


#[tokio::main]
async fn main() {
    // Parse dotenv files and expose them as environment variables
    dotenv().ok();

    // envy uses our Configuration struct to parse environment variables
    let config = envy::from_env::<Configuration>().expect("Please provide PORT env var");

    // initialize tracing
    tracing_subscriber::fmt()
        // With the log_level from our config
        .with_max_level(config.log_level())
        .init();

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/:path", get(implicit_handler))
        .route("/rel/:path", get(relative_handler))
        .route("/relative/:path", get(relative_handler))
        .route("/absolute/:path", get(absolute_handler))
        .route("/abs/:path", get(absolute_handler))
        .fallback(fallback_handler);

    let addr = SocketAddr::from((config.socket_addr(), config.port));
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}