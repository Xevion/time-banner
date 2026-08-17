//! Resolves the reference instant relative time values are computed
//! against: `?now=` query param, `Date-Now` request header, or the system
//! clock, first match wins.

use std::collections::HashMap;

use axum::extract::{FromRequestParts, Query};
use http::request::Parts;
use jiff::Timestamp;
use time_banner_core::parse_time_value;

use crate::error::TimeBannerError;

/// The resolved reference instant for the current request.
pub struct ReferenceNow(pub Timestamp);

impl<S: Send + Sync> FromRequestParts<S> for ReferenceNow {
    type Rejection = TimeBannerError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let wall_clock = Timestamp::now();

        let query_now = Query::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .ok()
            .and_then(|Query(params)| params.get("now").cloned());

        let raw = query_now.or_else(|| header_str(&parts.headers, "date-now").map(str::to_string));

        let Some(raw) = raw else {
            return Ok(ReferenceNow(wall_clock));
        };

        Ok(ReferenceNow(parse_time_value(&raw, wall_clock)?))
    }
}

fn header_str<'a>(headers: &'a http::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use axum::http::Request;

    use super::*;

    async fn resolve(request: Request<()>) -> Result<Timestamp, TimeBannerError> {
        let (mut parts, ()) = request.into_parts();
        ReferenceNow::from_request_parts(&mut parts, &())
            .await
            .map(|r| r.0)
    }

    #[tokio::test]
    async fn falls_back_to_the_system_clock_when_absent() {
        let before = Timestamp::now();
        let request = Request::builder().uri("/relative/0").body(()).unwrap();
        let resolved = resolve(request).await.unwrap();
        let after = Timestamp::now();

        check!(resolved >= before);
        check!(resolved <= after);
    }

    #[tokio::test]
    async fn query_param_overrides_the_system_clock() {
        let request = Request::builder()
            .uri("/relative/0?now=1700000000")
            .body(())
            .unwrap();
        check!(resolve(request).await.unwrap().as_second() == 1_700_000_000);
    }

    #[tokio::test]
    async fn header_overrides_the_system_clock() {
        let request = Request::builder()
            .uri("/relative/0")
            .header("Date-Now", "1700000000")
            .body(())
            .unwrap();
        check!(resolve(request).await.unwrap().as_second() == 1_700_000_000);
    }

    #[tokio::test]
    async fn query_param_takes_precedence_over_the_header() {
        let request = Request::builder()
            .uri("/relative/0?now=1700000000")
            .header("Date-Now", "1800000000")
            .body(())
            .unwrap();
        check!(resolve(request).await.unwrap().as_second() == 1_700_000_000);
    }

    #[tokio::test]
    async fn malformed_override_is_rejected() {
        let request = Request::builder()
            .uri("/relative/0?now=not-a-time")
            .body(())
            .unwrap();
        check!(let Err(TimeBannerError::ParseError(_)) = resolve(request).await);
    }
}
