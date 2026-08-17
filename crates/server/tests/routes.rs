//! End-to-end route tests, exercising the router as an HTTP client would
//! without binding a socket.

use assert2::{assert, check};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use rstest::{fixture, rstest};
use tower::ServiceExt;

/// A fresh router per test. Cheap to build (no I/O), so no `#[once]` sharing.
#[fixture]
fn router() -> Router {
    time_banner::build_router()
}

async fn get(router: Router, uri: &str) -> axum::http::Response<Body> {
    router
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

#[rstest]
#[case::implicit_absolute_defaults_to_svg("/1700000000", StatusCode::OK, Some("image/svg+xml"))]
#[case::absolute_png_extension_selects_png(
    "/absolute/1700000000.png",
    StatusCode::OK,
    Some("image/png")
)]
#[case::relative_alias_renders_svg("/rel/-3600", StatusCode::OK, Some("image/svg+xml"))]
#[case::unrecognized_value_is_bad_request("/absolute/not-a-time", StatusCode::BAD_REQUEST, None)]
#[case::unmatched_path_falls_back_to_not_found(
    "/this/has/too/many/segments",
    StatusCode::NOT_FOUND,
    None
)]
#[tokio::test]
async fn route_returns_expected_status_and_content_type(
    router: Router,
    #[case] uri: &str,
    #[case] expected_status: StatusCode,
    #[case] expected_content_type: Option<&str>,
) {
    let response = get(router, uri).await;
    check!(response.status() == expected_status);
    if let Some(expected) = expected_content_type {
        check!(content_type(&response) == expected);
    }
}

#[rstest]
#[tokio::test]
async fn health_reports_ok_and_version(router: Router) {
    let response = get(router, "/health").await;
    check!(response.status() == StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.starts_with("OK time-banner/"));
}

#[rstest]
#[tokio::test]
async fn favicon_renders_ico_for_a_known_client_ip(router: Router) {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/favicon.ico")
                .header("CF-Connecting-IP", "127.0.0.1")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible");

    check!(response.status() == StatusCode::OK);
    check!(content_type(&response) == "image/x-icon");
}
