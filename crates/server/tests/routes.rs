//! End-to-end route tests, exercising the router as an HTTP client would
//! without binding a socket.

use assert2::{assert, check};
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderValue, Request, Response, StatusCode, header};
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

/// A request carrying whatever headers a conditional request needs.
async fn get_with(router: Router, uri: &str, headers: &[(&str, &str)]) -> Response<Body> {
    let mut request = Request::builder().uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    router
        .oneshot(request.body(Body::empty()).expect("valid request"))
        .await
        .expect("router is infallible")
}

/// A strong `ETag` promises the bytes are reproducible. If a render ever stops
/// being, the validator has to weaken rather than the promise quietly breaking.
#[rstest]
#[case::svg("/absolute/1700000000")]
#[case::png("/absolute/1700000000.png")]
#[case::relative("/relative/1700000000?now=1700003600")]
#[tokio::test]
async fn a_render_is_byte_reproducible(router: Router, #[case] uri: &str) {
    let first = get(router.clone(), uri).await;
    let tag = header_value(&first, "etag").to_string();
    check!(
        !tag.starts_with("W/"),
        "a weak tag would be honest, but then the strong one below is wrong"
    );

    let second = get(router, uri).await;
    check!(header_value(&second, "etag") == tag);
    check!(body_of(first).await == body_of(second).await);
}

/// An instant that names itself reads the same forever, so there is nothing
/// for a cache to revalidate.
#[rstest]
#[case::absolute("/absolute/1700000000")]
#[case::implicit("/1700000000")]
#[case::iso("/2023-11-14T22:13:20Z")]
#[tokio::test]
async fn a_fixed_instant_is_immutable(router: Router, #[case] uri: &str) {
    let response = get(router, uri).await;
    check!(response.status() == StatusCode::OK);
    check!(header_value(&response, "cache-control").contains("immutable"));
}

/// Reads the `max-age` out of a `Cache-Control` value.
fn max_age(response: &Response<Body>) -> i64 {
    header_value(response, "cache-control")
        .split("max-age=")
        .nth(1)
        .and_then(|rest| rest.split([',', ' ']).next())
        .and_then(|value| value.parse().ok())
        .expect("a max-age is present")
}

/// A badge whose words move with the clock goes stale on a schedule, so it
/// must not claim otherwise.
#[rstest]
#[case::aging_in_real_time("/relative/1700000000")]
#[case::the_clock_itself("/absolute/now")]
#[tokio::test]
async fn a_moving_badge_expires(router: Router, #[case] uri: &str) {
    let response = get(router, uri).await;
    let directive = header_value(&response, "cache-control").to_string();
    check!(!directive.contains("immutable"));
    check!(directive.contains("stale-while-revalidate"));
    check!(max_age(&response) >= 1);
}

/// `/absolute/now` redraws its own seconds field, so it is only good until the
/// next one. A badge that changes this fast should say so rather than round up
/// into a minute of being wrong.
#[rstest]
#[tokio::test]
async fn a_second_resolution_badge_expires_in_a_second(router: Router) {
    let response = get(router, "/absolute/now").await;
    check!(max_age(&response) == 1);
}

/// A value written relative to the request instant renders the same words on
/// every request, forever. Discovering that is the search doing its job, not
/// missing a change: there is genuinely nothing to revalidate for.
#[rstest]
#[tokio::test]
async fn a_badge_anchored_to_the_request_never_moves(router: Router) {
    let response = get(router, "/relative/-3600").await;
    check!(max_age(&response) == 31_536_000);
    check!(!header_value(&response, "cache-control").contains("immutable"));
}

/// Pinning the reference instant makes the whole response a function of the
/// URL, whatever the value grammar did.
#[rstest]
#[tokio::test]
async fn a_pinned_reference_instant_is_immutable(router: Router) {
    let response = get(router, "/relative/+3600?now=1700000000").await;
    check!(header_value(&response, "cache-control").contains("immutable"));
}

/// The whole point of the entity tag: a client that already holds the bytes
/// gets told so instead of being sent them again.
#[rstest]
#[case::relative("/relative/-3600")]
#[case::absolute("/absolute/1700000000")]
#[case::png("/absolute/1700000000.png")]
#[tokio::test]
async fn a_held_entity_is_answered_with_304(router: Router, #[case] uri: &str) {
    let first = get(router.clone(), uri).await;
    let tag = header_value(&first, "etag").to_string();
    let directive = header_value(&first, "cache-control").to_string();

    let second = get_with(router, uri, &[("If-None-Match", &tag)]).await;
    check!(second.status() == StatusCode::NOT_MODIFIED);
    check!(header_value(&second, "etag") == tag);
    check!(header_value(&second, "cache-control") == directive);
    check!(header_value(&second, "vary").len() > 0);
    check!(body_of(second).await == Vec::<u8>::new());
}

#[rstest]
#[tokio::test]
async fn a_stale_entity_is_answered_with_the_bytes(router: Router) {
    let response = get_with(
        router,
        "/relative/-3600",
        &[("If-None-Match", "\"something-else\"")],
    )
    .await;
    check!(response.status() == StatusCode::OK);
    check!(!body_of(response).await.is_empty());
}

/// Two renders that draw the same words are the same representation, however
/// far apart the requests were. This is what lets a badge revalidate rather
/// than re-download for the whole hour it reads the same.
#[rstest]
#[tokio::test]
async fn the_tag_follows_the_words_not_the_clock(router: Router) {
    let early = get(router.clone(), "/relative/0?now=7200").await;
    let late = get(router.clone(), "/relative/0?now=7300").await;
    let different = get(router, "/relative/0?now=90000").await;

    check!(header_value(&early, "etag") == header_value(&late, "etag"));
    check!(header_value(&early, "etag") != header_value(&different, "etag"));
}

/// Different bytes must never share a tag, or a cache will serve one
/// representation where another was asked for.
#[rstest]
#[case::format("/absolute/1700000000.png")]
#[case::text_mode("/absolute/1700000000?text=live")]
#[case::font("/absolute/1700000000?font=inter")]
#[tokio::test]
async fn a_different_representation_gets_a_different_tag(router: Router, #[case] uri: &str) {
    let base = get(router.clone(), "/absolute/1700000000").await;
    let other = get(router, uri).await;
    check!(header_value(&base, "etag") != header_value(&other, "etag"));
}

/// `Date-Now` changes what gets drawn, so a shared cache has to key on it.
/// `Font` is only ever a response header, so listing it would fragment caches
/// on a request header nothing reads.
#[rstest]
#[tokio::test]
async fn vary_names_the_request_headers_that_matter(router: Router) {
    let response = get(router, "/relative/-3600").await;
    let vary = response
        .headers()
        .get_all("vary")
        .iter()
        .map(|value| value.to_str().expect("header is ASCII").to_lowercase())
        .collect::<Vec<_>>()
        .join(", ");

    check!(vary.contains("date-now"));
    check!(vary.contains("timezone"));
    check!(vary.contains("accept-language"));
    check!(!vary.contains("font"));
}

/// Geolocation still forbids shared caching, and now says how long the answer
/// is good for as well.
#[rstest]
#[tokio::test]
async fn a_geolocated_response_stays_private_and_still_expires(router: Router) {
    let response = get_with(
        router,
        "/relative/-3600?tz=auto",
        &[("X-Real-IP", "127.0.0.1")],
    )
    .await;
    let directive = header_value(&response, "cache-control");
    check!(directive.contains("private"));
    check!(directive.contains("max-age="));
}

/// Echoing our own `Last-Modified` back has to be recognized, or the header is
/// decoration.
#[rstest]
#[tokio::test]
async fn a_returned_last_modified_is_honored(router: Router) {
    let first = get(router.clone(), "/relative/1700000000").await;
    let modified = header_value(&first, "last-modified").to_string();

    let second = get_with(
        router,
        "/relative/1700000000",
        &[("If-Modified-Since", &modified)],
    )
    .await;
    check!(second.status() == StatusCode::NOT_MODIFIED);
}

/// A cache updates its stored response with whatever headers a `304` carries,
/// so a placeholder content type here would overwrite the real one it holds
/// and hand the reader an octet stream where an image was cached.
#[rstest]
#[tokio::test]
async fn a_304_describes_no_body_of_its_own(router: Router) {
    let first = get(router.clone(), "/absolute/1700000000.png").await;
    let tag = header_value(&first, "etag").to_string();

    let second = get_with(
        router,
        "/absolute/1700000000.png",
        &[("If-None-Match", &tag)],
    )
    .await;
    check!(second.status() == StatusCode::NOT_MODIFIED);
    check!(second.headers().get(header::CONTENT_TYPE) == None);

    // axum stamps this from the empty body's size hint, in a wrapper that sits
    // outside anything a router layer can reach. RFC 9110 permits it on a
    // `304` only when it matches what a `200` would have sent, so this pins
    // the deviation rather than endorsing it.
    check!(second.headers().get(header::CONTENT_LENGTH) == Some(&HeaderValue::from_static("0")));
}
