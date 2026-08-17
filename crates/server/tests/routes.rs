//! End-to-end route tests, exercising the router as an HTTP client would
//! without binding a socket.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn get(uri: &str) -> axum::http::Response<Body> {
    time_banner::build_router()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible")
}

fn content_type(response: &axum::http::Response<Body>) -> &str {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("content-type header present")
        .to_str()
        .expect("content-type is ASCII")
}

#[tokio::test]
async fn health_reports_ok_and_version() {
    let response = get("/health").await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.starts_with("OK time-banner/"));
}

#[tokio::test]
async fn implicit_absolute_renders_svg_by_default() {
    let response = get("/1700000000").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(content_type(&response), "image/svg+xml");
}

#[tokio::test]
async fn absolute_png_extension_selects_png() {
    let response = get("/absolute/1700000000.png").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(content_type(&response), "image/png");
}

#[tokio::test]
async fn relative_alias_renders_svg() {
    let response = get("/rel/-3600").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(content_type(&response), "image/svg+xml");
}

#[tokio::test]
async fn unrecognized_value_is_bad_request() {
    let response = get("/absolute/not-a-time").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unmatched_path_falls_back_to_not_found() {
    let response = get("/this/has/too/many/segments").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn favicon_renders_ico_for_a_known_client_ip() {
    let response = time_banner::build_router()
        .oneshot(
            Request::builder()
                .uri("/favicon.ico")
                .header("CF-Connecting-IP", "127.0.0.1")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(content_type(&response), "image/x-icon");
}
