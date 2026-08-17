//! Resolves the request axes carried outside the path: the reference instant
//! relative values are computed against, and the timezone values are drawn
//! in. Query parameters win over headers, since a URL is the only thing a
//! README author controls.

use std::collections::HashMap;

use axum::extract::{FromRequestParts, Query, Request};
use axum::middleware::Next;
use axum::response::Response;
use http::request::Parts;
use http::{HeaderMap, HeaderValue, Uri, header};
use jiff::{Timestamp, tz::TimeZone};
use time_banner_core::{parse_time_value, resolve_timezone};

use crate::error::TimeBannerError;

/// The request axes resolved once, ahead of the handler.
#[derive(Clone)]
pub struct Resolution {
    pub now: Timestamp,
    pub tz: TimeZone,
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
}

/// Resolves both axes, timezone first, since a civil date's meaning depends
/// on the zone it is read in.
fn resolve(uri: &Uri, headers: &HeaderMap) -> Result<Resolution, TimeBannerError> {
    let params = Query::<HashMap<String, String>>::try_from_uri(uri)
        .map(|Query(params)| params)
        .unwrap_or_default();
    let param_or_header = |name: &str, header_name: &str| {
        params
            .get(name)
            .map(String::as_str)
            .or_else(|| header_str(headers, header_name))
    };

    let tz = match param_or_header("tz", "timezone") {
        Some(spec) => resolve_timezone(spec)?,
        None => TimeZone::UTC,
    };

    let wall_clock = Timestamp::now();
    let now = match param_or_header("now", "date-now") {
        Some(raw) => parse_time_value(raw, wall_clock, &tz)?,
        None => wall_clock,
    };

    Ok(Resolution { now, tz })
}

/// Resolves the request axes before the handler runs, and reports the zone
/// afterwards. Wrapping the whole router rather than individual handlers is
/// what puts `Timezone` on every response, `404`s included.
pub async fn resolve_request(
    mut request: Request,
    next: Next,
) -> Result<Response, TimeBannerError> {
    let resolution = resolve(request.uri(), request.headers())?;
    let label = resolution.timezone_label();
    request.extensions_mut().insert(resolution);

    let mut response = next.run(request).await;

    if let Ok(value) = HeaderValue::from_str(&label) {
        response.headers_mut().insert("timezone", value);
    }
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("Timezone"));

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

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use axum::http::Request;

    use super::*;

    fn resolve_request_parts(request: Request<()>) -> Result<Resolution, TimeBannerError> {
        resolve(request.uri(), request.headers())
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
}
