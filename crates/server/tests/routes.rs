//! End-to-end route tests, exercising the router as an HTTP client would
//! without binding a socket.

use assert2::{assert, check};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use rstest::{fixture, rstest};
use tower::ServiceExt;

/// A fresh router per test. Cheap to build (no I/O, no geoip database), so
/// no `#[once]` sharing.
#[fixture]
fn router() -> Router {
    time_banner::build_router(None)
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

/// The favicon routes need a client address, which only a proxy header or a
/// real socket supplies. `X-Real-IP` since that's the trusted source.
async fn get_as_client(router: Router, uri: &str) -> axum::http::Response<Body> {
    router
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("X-Real-IP", "127.0.0.1")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible")
}

fn content_type(response: &axum::http::Response<Body>) -> &str {
    header_value(response, header::CONTENT_TYPE.as_str())
}

fn header_value<'a>(response: &'a axum::http::Response<Body>, name: &str) -> &'a str {
    response
        .headers()
        .get(name)
        .unwrap_or_else(|| panic!("{name} header present"))
        .to_str()
        .expect("header is ASCII")
}

async fn body_of(response: axum::http::Response<Body>) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes()
        .to_vec()
}

async fn body_text(response: axum::http::Response<Body>) -> String {
    String::from_utf8(body_of(response).await).expect("body is UTF-8")
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
#[case::before_reference_now_renders_ago("/relative/0?now=1000000000&text=live", "ago")]
#[case::after_reference_now_renders_from_now(
    "/relative/2000000000?now=1000000000&text=live",
    "from now"
)]
#[tokio::test]
async fn now_override_shifts_relative_rendering(
    router: Router,
    #[case] uri: &str,
    #[case] expected_phrase: &str,
) {
    let response = get(router, uri).await;
    check!(response.status() == StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains(expected_phrase));
}

#[rstest]
#[tokio::test]
async fn malformed_now_override_is_bad_request(router: Router) {
    let response = get(router, "/relative/0?now=not-a-time").await;
    check!(response.status() == StatusCode::BAD_REQUEST);
}

#[rstest]
#[tokio::test]
async fn date_now_header_shifts_relative_rendering(router: Router) {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/relative/2000000000?text=live")
                .header("Date-Now", "1000000000")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible");
    check!(response.status() == StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("from now"));
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

/// Section 6 requires the resolved zone on every response, not just on
/// rendered banners.
#[rstest]
#[case::banner("/absolute/1700000000")]
#[case::health("/health")]
#[case::index("/")]
#[case::not_found("/this/has/too/many/segments")]
#[tokio::test]
async fn every_response_reports_the_resolved_timezone(router: Router, #[case] uri: &str) {
    let response = get(router, uri).await;
    check!(header_value(&response, "timezone") == "UTC");
}

#[rstest]
#[case::iana("/absolute/1700000000?tz=America/Chicago&text=live")]
#[case::tilde_substituted("/absolute/1700000000?tz=America~Chicago&text=live")]
#[case::abbreviation("/absolute/1700000000?tz=CST&text=live")]
#[tokio::test]
async fn timezone_shifts_absolute_rendering(router: Router, #[case] uri: &str) {
    let response = get(router, uri).await;
    check!(response.status() == StatusCode::OK);
    check!(header_value(&response, "timezone") == "America/Chicago");

    assert!(
        body_text(response)
            .await
            .contains("2023-11-14 16:13:20 CST")
    );
}

#[rstest]
#[tokio::test]
async fn timezone_header_is_honored(router: Router) {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/absolute/1700000000?text=live")
                .header("Timezone", "Asia/Tokyo")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible");

    check!(header_value(&response, "timezone") == "Asia/Tokyo");
    assert!(
        body_text(response)
            .await
            .contains("2023-11-15 07:13:20 JST")
    );
}

#[rstest]
#[tokio::test]
async fn malformed_timezone_is_bad_request(router: Router) {
    let response = get(router, "/absolute/1700000000?tz=Mars/Olympus").await;
    check!(response.status() == StatusCode::BAD_REQUEST);
}

#[rstest]
#[tokio::test]
async fn format_query_overrides_the_default_absolute_pattern(router: Router) {
    let response = get(router, "/absolute/1700000000?format=%25Y&text=live").await;
    check!(response.status() == StatusCode::OK);
    check!(body_text(response).await.contains(">2023<"));
}

#[rstest]
#[tokio::test]
async fn invalid_format_directive_is_bad_request(router: Router) {
    let response = get(router, "/absolute/1700000000?format=%25K").await;
    check!(response.status() == StatusCode::BAD_REQUEST);
}

#[rstest]
#[tokio::test]
async fn oversized_format_string_is_payload_too_large(router: Router) {
    let format: String = "%25Y".repeat(40); // decodes to 80 bytes, past the 64-byte input cap
    let response = get(router, &format!("/absolute/1700000000?format={format}")).await;
    check!(response.status() == StatusCode::PAYLOAD_TOO_LARGE);
}

#[rstest]
#[tokio::test]
async fn relative_renders_in_the_negotiated_locale(router: Router) {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/relative/0?now=3600&text=live")
                .header("Accept-Language", "fr")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible");

    check!(response.status() == StatusCode::OK);
    check!(header_value(&response, "content-language") == "fr");
    check!(header_value(&response, "vary") == "Accept-Language");
    assert!(body_text(response).await.contains("heure"));
}

#[rstest]
#[tokio::test]
async fn locale_query_param_overrides_the_accept_language_header(router: Router) {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/relative/0?now=1000000000&locale=de")
                .header("Accept-Language", "fr")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible");

    check!(header_value(&response, "content-language") == "de");
}

#[rstest]
#[tokio::test]
async fn unsupported_locale_falls_back_to_english(router: Router) {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/relative/0?now=1000000000")
                .header("Accept-Language", "xx")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible");

    check!(header_value(&response, "content-language") == "en");
}

/// SPEC 7 scopes `Content-Language`/`Vary: Accept-Language` to the modes
/// that actually render words; `absolute` doesn't, unlike `Timezone`, which
/// every response carries regardless of mode.
#[rstest]
#[tokio::test]
async fn absolute_output_carries_no_locale_headers(router: Router) {
    let response = get(router, "/absolute/1700000000").await;
    check!(response.headers().get("content-language").is_none());
    // `Vary: Timezone` is global (added by `resolve_request` for every
    // response); only `Accept-Language` is scoped to word-rendering modes.
    check!(
        !response
            .headers()
            .get_all("vary")
            .iter()
            .any(|v| v == "Accept-Language")
    );
}

#[rstest]
#[tokio::test]
async fn favicon_hands_follow_the_resolved_timezone(router: Router) {
    let utc = get_as_client(router.clone(), "/favicon.png?now=1700000000").await;
    let tokyo = get_as_client(router, "/favicon.png?now=1700000000&tz=Asia/Tokyo").await;

    check!(utc.status() == StatusCode::OK);
    check!(body_of(utc).await != body_of(tokyo).await);
}

#[rstest]
#[tokio::test]
async fn favicon_renders_ico_for_a_known_client_ip(router: Router) {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/favicon.ico")
                .header("X-Real-IP", "127.0.0.1")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("router is infallible");

    check!(response.status() == StatusCode::OK);
    check!(content_type(&response) == "image/x-icon");
}

#[rstest]
#[case::ico("/favicon.ico")]
#[case::png("/favicon.png")]
#[tokio::test]
async fn favicon_responses_are_never_cached(router: Router, #[case] uri: &str) {
    let response = get_as_client(router, uri).await;
    check!(header_value(&response, "cache-control").contains("no-store"));
}

#[rstest]
#[tokio::test]
async fn favicon_and_index_default_to_geolocated_privacy(router: Router) {
    let response = get_as_client(router, "/favicon.ico").await;
    check!(header_value(&response, "cache-control").contains("private"));
}

#[rstest]
#[case::absolute("/absolute/1700000000", "Roboto Mono")]
#[case::relative("/relative/-3600", "Inter")]
#[case::implicit("/1700000000", "Roboto Mono")]
#[tokio::test]
async fn text_responses_report_the_face_that_drew_them(
    router: Router,
    #[case] uri: &str,
    #[case] expected: &str,
) {
    let response = get(router, uri).await;
    check!(response.status() == StatusCode::OK);
    check!(header_value(&response, "font") == expected);
}

#[rstest]
#[case("inter", "Inter")]
#[case("roboto-mono", "Roboto Mono")]
#[case("arimo", "Arimo")]
#[tokio::test]
async fn font_query_selects_the_face(router: Router, #[case] value: &str, #[case] expected: &str) {
    let response = get(router, &format!("/absolute/1700000000?font={value}")).await;
    check!(response.status() == StatusCode::OK);
    check!(header_value(&response, "font") == expected);
}

#[rstest]
#[case("/absolute/1700000000?font=comic-sans")]
#[case("/absolute/1700000000?text=paths")]
#[tokio::test]
async fn an_unknown_presentation_value_is_rejected(router: Router, #[case] uri: &str) {
    let response = get(router, uri).await;
    check!(response.status() == StatusCode::BAD_REQUEST);
}

/// The favicon is an analog clock with no text, so it names no face.
#[rstest]
#[tokio::test]
async fn the_favicon_reports_no_face(router: Router) {
    let response = get_as_client(router, "/favicon.png").await;
    check!(response.status() == StatusCode::OK);
    check!(response.headers().get("font") == None);
}

#[rstest]
#[case::outline("outline", false)]
#[case::embed("embed", true)]
#[case::live("live", true)]
#[tokio::test]
async fn text_query_selects_how_glyphs_are_delivered(
    router: Router,
    #[case] mode: &str,
    #[case] keeps_live_text: bool,
) {
    let response = get(router, &format!("/absolute/1700000000?text={mode}")).await;
    check!(response.status() == StatusCode::OK);

    let body = body_text(response).await;
    check!(body.contains("<text") == keeps_live_text);
    check!(body.contains("@font-face") == (mode == "embed"));
}

/// Served as `image/svg+xml`, so a caller who opens the URL directly renders
/// it as a document. A `?format=` value carrying markup must not become part
/// of that document.
#[rstest]
#[tokio::test]
async fn a_format_string_cannot_inject_markup(router: Router) {
    let response = get(
        router,
        "/absolute/1700000000?text=live&format=%3C%2Ftext%3E%3Cscript%3Ealert(1)%3C%2Fscript%3E",
    )
    .await;
    check!(response.status() == StatusCode::OK);

    let body = body_text(response).await;
    check!(!body.contains("<script"));
    check!(body.contains("&lt;script&gt;"));
}

/// No bundled face covers CJK, so the chain runs out and the glyphs are boxes.
/// A caller looking at those boxes has no other way to tell that the face was
/// missing the glyphs rather than the styling being wrong.
#[rstest]
#[tokio::test]
async fn an_uncoverable_script_is_reported_as_partial_coverage(router: Router) {
    let response = get(router, "/absolute/1700000000?format=%E4%BB%8A%E6%97%A5").await;
    check!(response.status() == StatusCode::OK);
    check!(header_value(&response, "font").ends_with("coverage=partial"));
}
