use std::net::SocketAddr;

use axum::{http::StatusCode, Json, response::IntoResponse, Router, routing::{get, post}};
use axum::body::{Bytes, Full};
use axum::extract::ConnectInfo;
use axum::http::header;
use axum::response::Response;

mod svg;

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(root));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

// basic handler that responds with a static string
async fn root(connect_info: ConnectInfo<SocketAddr>) -> impl IntoResponse {
    let raw_image = svg::get();

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