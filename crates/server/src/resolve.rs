//! Resolves the request axes carried outside the path: the reference instant
//! relative values are computed against, and the timezone values are drawn
//! in. Query parameters win over headers, since a URL is the only thing a
//! README author controls.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, FromRequestParts, Query, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::request::Parts;
use http::{HeaderMap, HeaderValue, Uri, header};
use jiff::{Timestamp, tz::TimeZone};
use memmap2::Mmap;
use time_banner_core::geoip::GeoipDb;
use time_banner_core::{parse_time_value, resolve_timezone};

use crate::client_ip;
use crate::error::TimeBannerError;
use crate::utils::HeaderMapExt;

/// Shared geoip state threaded into the resolve middleware via `State`,
/// independent of the router's own (stateless) type parameter. `None` when
/// no database was configured or it failed to load at startup.
pub type GeoipState = Option<Arc<GeoipDb<Mmap>>>;

/// The request axes resolved once, ahead of the handler.
#[derive(Clone)]
pub struct Resolution {
    pub now: Timestamp,
    pub tz: TimeZone,
    /// `?format=` pattern for absolute output, unvalidated. `None` draws
    /// with the default pattern.
    pub format: Option<String>,
    /// Negotiated language code, always one `time_banner_render::locale`
    /// actually has words for.
    pub locale: String,
    /// Whether `tz` was resolved via IP geolocation. Responses that consumed
    /// the client address this way carry `Cache-Control: private`, per
    /// SPEC.md section 6.2.
    pub used_geolocation: bool,
}

/// Routes with no shared-cache audience, so `tz` defaults to `auto` here
/// instead of `UTC`.
fn defaults_to_auto_tz(path: &str) -> bool {
    matches!(path, "/" | "/favicon.ico" | "/favicon.png")
}

impl Resolution {
    /// How the resolved zone is reported back. An IANA identifier where the
    /// zone has one, and the numeric offset where it does not, which is the
    /// case only for a spec written as a fixed offset.
    pub fn timezone_label(&self) -> String {
        if let Some(name) = self.tz.iana_name() {
            return name.to_string();
        }

        let seconds = self.tz.to_offset(self.now).seconds();
        let sign = if seconds < 0 { '-' } else { '+' };
        let magnitude = seconds.abs();
        format!("{sign}{:02}:{:02}", magnitude / 3600, magnitude % 3600 / 60)
    }

    /// Resolves both axes, timezone first, since a civil date's meaning
    /// depends on the zone it is read in. `geo_tz` is the already-resolved
    /// geolocation result (or `None` if unavailable), computed by the
    /// caller ahead of time so this function stays a pure, synchronous
    /// parse of the request axes.
    fn resolve(
        uri: &Uri,
        headers: &HeaderMap,
        geo_tz: Option<TimeZone>,
    ) -> Result<Resolution, TimeBannerError> {
        let params = Query::<HashMap<String, String>>::try_from_uri(uri)
            .map(|Query(params)| params)
            .unwrap_or_default();
        let param_or_header = |name: &str, header_name: &str| {
            params
                .get(name)
                .map(String::as_str)
                .or_else(|| headers.get_str(header_name))
        };

        let (tz, used_geolocation) = match param_or_header("tz", "timezone") {
            Some(spec) if spec.trim().eq_ignore_ascii_case("auto") => {
                (geo_tz.unwrap_or(TimeZone::UTC), true)
            }
            Some(spec) => (resolve_timezone(spec)?, false),
            None if defaults_to_auto_tz(uri.path()) => (geo_tz.unwrap_or(TimeZone::UTC), true),
            None => (TimeZone::UTC, false),
        };

        let wall_clock = Timestamp::now();
        let now = match param_or_header("now", "date-now") {
            Some(raw) => parse_time_value(raw, wall_clock, &tz)?,
            None => wall_clock,
        };

        let format = params.get("format").cloned();
        let locale = crate::locale::resolve(
            params.get("locale").map(String::as_str),
            headers.get_str("accept-language"),
        );

        Ok(Resolution {
            now,
            tz,
            format,
            locale,
            used_geolocation,
        })
    }
}

/// Resolves the request axes before the handler runs, and reports the zone
/// afterwards. Wrapping the whole router rather than individual handlers is
/// what puts `Timezone` on every response, `404`s included.
pub async fn resolve_request(
    State(geoip): State<GeoipState>,
    mut request: Request,
    next: Next,
) -> Result<Response, TimeBannerError> {
    let connect_info = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| *addr);
    let client_ip = client_ip::resolve(request.headers(), connect_info, client_ip::TRUST);
    let geo_tz = client_ip
        .and_then(|ip| geoip.as_deref().and_then(|db| db.lookup(ip)))
        .and_then(|name| TimeZone::get(name).ok());

    let resolution = Resolution::resolve(request.uri(), request.headers(), geo_tz)?;
    let label = resolution.timezone_label();
    let used_geolocation = resolution.used_geolocation;
    request.extensions_mut().insert(resolution);

    let mut response = next.run(request).await;

    if let Ok(value) = HeaderValue::from_str(&label) {
        response.headers_mut().insert("timezone", value);
    }
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("Timezone"));

    // Appended, not `insert`ed, so a handler's own `Cache-Control` survives.
    if used_geolocation {
        let value = match response.headers().get(header::CACHE_CONTROL) {
            Some(existing) => {
                let combined = format!("{}, private", existing.to_str().unwrap_or_default());
                HeaderValue::from_str(&combined).ok()
            }
            None => Some(HeaderValue::from_static("private")),
        };
        if let Some(value) = value {
            response.headers_mut().insert(header::CACHE_CONTROL, value);
        }
    }

    Ok(response)
}

impl<S: Send + Sync> FromRequestParts<S> for Resolution {
    type Rejection = TimeBannerError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Resolution>()
            .cloned()
            .ok_or_else(|| {
                TimeBannerError::Internal("request axes were never resolved".to_string())
            })
    }
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use axum::http::Request;
    use rstest::rstest;

    use super::*;

    fn resolve_request_parts(request: Request<()>) -> Result<Resolution, TimeBannerError> {
        resolve_request_parts_with_geo(request, None)
    }

    fn resolve_request_parts_with_geo(
        request: Request<()>,
        geo_tz: Option<TimeZone>,
    ) -> Result<Resolution, TimeBannerError> {
        Resolution::resolve(request.uri(), request.headers(), geo_tz)
    }

    fn now_of(request: Request<()>) -> Result<Timestamp, TimeBannerError> {
        resolve_request_parts(request).map(|resolution| resolution.now)
    }

    #[test]
    fn falls_back_to_the_system_clock_when_absent() {
        let before = Timestamp::now();
        let request = Request::builder().uri("/relative/0").body(()).unwrap();
        let resolved = now_of(request).unwrap();
        let after = Timestamp::now();

        check!(resolved >= before);
        check!(resolved <= after);
    }

    #[test]
    fn query_param_overrides_the_system_clock() {
        let request = Request::builder()
            .uri("/relative/0?now=1700000000")
            .body(())
            .unwrap();
        check!(now_of(request).unwrap().as_second() == 1_700_000_000);
    }

    #[test]
    fn header_overrides_the_system_clock() {
        let request = Request::builder()
            .uri("/relative/0")
            .header("Date-Now", "1700000000")
            .body(())
            .unwrap();
        check!(now_of(request).unwrap().as_second() == 1_700_000_000);
    }

    #[test]
    fn query_param_takes_precedence_over_the_header() {
        let request = Request::builder()
            .uri("/relative/0?now=1700000000")
            .header("Date-Now", "1800000000")
            .body(())
            .unwrap();
        check!(now_of(request).unwrap().as_second() == 1_700_000_000);
    }

    #[test]
    fn malformed_override_is_rejected() {
        let request = Request::builder()
            .uri("/relative/0?now=not-a-time")
            .body(())
            .unwrap();
        check!(let Err(TimeBannerError::ParseError(_)) = now_of(request));
    }

    #[test]
    fn timezone_defaults_to_utc() {
        let request = Request::builder().uri("/relative/0").body(()).unwrap();
        check!(resolve_request_parts(request).unwrap().timezone_label() == "UTC");
    }

    #[test]
    fn timezone_query_param_takes_precedence_over_the_header() {
        let request = Request::builder()
            .uri("/relative/0?tz=Asia/Tokyo")
            .header("Timezone", "America/Chicago")
            .body(())
            .unwrap();
        check!(resolve_request_parts(request).unwrap().timezone_label() == "Asia/Tokyo");
    }

    #[test]
    fn timezone_header_is_honored() {
        let request = Request::builder()
            .uri("/relative/0")
            .header("Timezone", "CST")
            .body(())
            .unwrap();
        check!(resolve_request_parts(request).unwrap().timezone_label() == "America/Chicago");
    }

    #[test]
    fn auto_resolves_via_the_supplied_geolocation_result() {
        let request = Request::builder()
            .uri("/relative/0?tz=auto")
            .body(())
            .unwrap();
        let resolution =
            resolve_request_parts_with_geo(request, Some(TimeZone::get("Asia/Tokyo").unwrap()))
                .unwrap();
        check!(resolution.timezone_label() == "Asia/Tokyo");
        check!(resolution.used_geolocation);
    }

    #[test]
    fn auto_falls_back_to_utc_when_geolocation_is_unavailable() {
        let request = Request::builder()
            .uri("/relative/0?tz=auto")
            .body(())
            .unwrap();
        let resolution = resolve_request_parts_with_geo(request, None).unwrap();
        check!(resolution.timezone_label() == "UTC");
        // still true: the client address was consulted and simply missed,
        // which is distinct from `used_geolocation == false` for a request
        // that never asked for `auto` at all.
        check!(resolution.used_geolocation);
    }

    #[test]
    fn explicit_timezone_is_not_reported_as_geolocated() {
        let request = Request::builder()
            .uri("/relative/0?tz=Asia/Tokyo")
            .body(())
            .unwrap();
        let resolution =
            resolve_request_parts_with_geo(request, Some(TimeZone::get("Europe/Paris").unwrap()))
                .unwrap();
        check!(resolution.timezone_label() == "Asia/Tokyo");
        check!(!resolution.used_geolocation);
    }

    #[test]
    fn absent_timezone_is_not_reported_as_geolocated() {
        let request = Request::builder().uri("/relative/0").body(()).unwrap();
        let resolution =
            resolve_request_parts_with_geo(request, Some(TimeZone::get("Europe/Paris").unwrap()))
                .unwrap();
        check!(resolution.timezone_label() == "UTC");
        check!(!resolution.used_geolocation);
    }

    #[rstest]
    #[case::favicon_ico("/favicon.ico")]
    #[case::favicon_png("/favicon.png")]
    #[case::index("/")]
    fn favicon_and_index_default_to_the_geolocated_timezone(#[case] path: &str) {
        let request = Request::builder().uri(path).body(()).unwrap();
        let resolution =
            resolve_request_parts_with_geo(request, Some(TimeZone::get("Asia/Tokyo").unwrap()))
                .unwrap();
        check!(resolution.timezone_label() == "Asia/Tokyo");
        check!(resolution.used_geolocation);
    }

    #[test]
    fn an_explicit_tz_still_overrides_the_favicon_auto_default() {
        let request = Request::builder()
            .uri("/favicon.ico?tz=America/Chicago")
            .body(())
            .unwrap();
        let resolution =
            resolve_request_parts_with_geo(request, Some(TimeZone::get("Asia/Tokyo").unwrap()))
                .unwrap();
        check!(resolution.timezone_label() == "America/Chicago");
        check!(!resolution.used_geolocation);
    }

    #[test]
    fn other_routes_keep_the_utc_default_even_with_geolocation_available() {
        let request = Request::builder().uri("/relative/0").body(()).unwrap();
        let resolution =
            resolve_request_parts_with_geo(request, Some(TimeZone::get("Asia/Tokyo").unwrap()))
                .unwrap();
        check!(resolution.timezone_label() == "UTC");
        check!(!resolution.used_geolocation);
    }

    #[test]
    fn a_fixed_offset_reports_itself_numerically() {
        let request = Request::builder()
            .uri("/relative/0?tz=UTC-6")
            .body(())
            .unwrap();
        check!(resolve_request_parts(request).unwrap().timezone_label() == "-06:00");
    }

    #[test]
    fn malformed_timezone_is_rejected() {
        let request = Request::builder()
            .uri("/relative/0?tz=Mars/Olympus")
            .body(())
            .unwrap();
        check!(let Err(TimeBannerError::ParseError(_)) = resolve_request_parts(request));
    }

    /// A civil date means midnight where the caller is, so the timezone has
    /// to be resolved before the reference instant that depends on it.
    #[test]
    fn the_reference_instant_is_read_in_the_resolved_zone() {
        let request = Request::builder()
            .uri("/relative/0?now=2027-01-01&tz=America/Chicago")
            .body(())
            .unwrap();
        check!(now_of(request).unwrap().as_second() == 1_798_761_600 + 6 * 3600);
    }

    #[test]
    fn format_defaults_to_absent() {
        let request = Request::builder().uri("/absolute/0").body(()).unwrap();
        check!(resolve_request_parts(request).unwrap().format == None);
    }

    #[test]
    fn format_is_read_from_the_query_string() {
        let request = Request::builder()
            .uri("/absolute/0?format=%25Y")
            .body(())
            .unwrap();
        check!(resolve_request_parts(request).unwrap().format == Some("%Y".to_string()));
    }

    #[test]
    fn locale_defaults_to_english() {
        let request = Request::builder().uri("/relative/0").body(()).unwrap();
        check!(resolve_request_parts(request).unwrap().locale == "en");
    }

    #[test]
    fn locale_query_param_takes_precedence_over_the_header() {
        let request = Request::builder()
            .uri("/relative/0?locale=de")
            .header("Accept-Language", "fr")
            .body(())
            .unwrap();
        check!(resolve_request_parts(request).unwrap().locale == "de");
    }
}
