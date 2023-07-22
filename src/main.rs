use std::net::SocketAddr;

use axum::{Router, routing::get};
use dotenvy::dotenv;
use config::Configuration;
use crate::routes::root_handler;

mod config;
mod svg;
mod abbr;
mod routes;


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
        .route("/:path", get(root_handler));
    let addr = SocketAddr::from((config.socket_addr(), config.port));
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}