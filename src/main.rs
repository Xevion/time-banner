mod config;

use std::net::SocketAddr;

use axum::{http::StatusCode, Json, response::IntoResponse, Router, routing::{get, post}};
use axum::body::{Bytes, Full};
use axum::extract::ConnectInfo;
use axum::http::header;
use axum::response::Response;
use dotenvy::dotenv;
use config::Configuration;

mod svg;


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

    // build our application with a route
    let app = Router::new().route("/", get(root_handler));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from((config.socket_addr(), config.port));

    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

// basic handler that responds with a static string
async fn root_handler(connect_info: ConnectInfo<SocketAddr>) -> impl IntoResponse {
    let renderer = svg::Renderer::new();
    let data = include_bytes!("./templates/basic.svg");
    let raw_image = renderer.render(data);

    if raw_image.is_err() {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Full::from(
                format!("Internal Server Error :: {}", raw_image.err().unwrap())
            ))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/x-png")
        .body(Full::from(raw_image.unwrap()))
        .unwrap()
}