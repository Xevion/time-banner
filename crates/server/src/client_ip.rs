//! Client IP extraction from trusted proxy headers.
//!
//! Priority: `CF-Connecting-IP` (Cloudflare) -> rightmost `X-Forwarded-For`
//! (Railway) -> socket peer address.

use crate::error::TimeBannerError;
use crate::utils::HeaderMapExt;
use axum::extract::{ConnectInfo, FromRequestParts};
use http::request::Parts;
use std::net::{IpAddr, SocketAddr};

/// The resolved client IP address.
pub struct ClientIp(pub IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = TimeBannerError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // CF-Connecting-IP -- set by Cloudflare, most trustworthy.
        if let Some(ip) = parts
            .headers
            .get_str("cf-connecting-ip")
            .and_then(|s| s.parse::<IpAddr>().ok())
        {
            return Ok(ClientIp(ip));
        }

        // Rightmost X-Forwarded-For -- appended by Railway's edge proxy.
        if let Some(xff) = parts.headers.get_str("x-forwarded-for")
            && let Some(ip) = xff
                .rsplit(',')
                .next()
                .map(str::trim)
                .and_then(|s| s.parse::<IpAddr>().ok())
        {
            return Ok(ClientIp(ip));
        }

        // Socket peer address (local dev fallback).
        if let Some(ConnectInfo(addr)) = parts.extensions.get::<ConnectInfo<SocketAddr>>() {
            return Ok(ClientIp(addr.ip()));
        }

        Err(TimeBannerError::Internal(
            "Unable to determine client IP".to_string(),
        ))
    }
}
