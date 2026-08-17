//! Client IP extraction from trusted proxy headers.
//!
//! Priority: `CF-Connecting-IP` (Cloudflare) -> rightmost `X-Forwarded-For`
//! (Railway) -> socket peer address.

use crate::error::TimeBannerError;
use crate::utils::HeaderMapExt;
use axum::extract::{ConnectInfo, FromRequestParts};
use http::HeaderMap;
use http::request::Parts;
use std::net::{IpAddr, SocketAddr};

/// The resolved client IP address.
pub struct ClientIp(pub IpAddr);

/// `CF-Connecting-IP` -> rightmost `X-Forwarded-For` -> socket peer address.
/// Pulled out of the `FromRequestParts` impl so middleware that only has a
/// `Request` (not the typed extractor) can resolve the same address, e.g.
/// for geoip lookup ahead of the handler.
pub fn resolve(headers: &HeaderMap, connect_info: Option<SocketAddr>) -> Option<IpAddr> {
    // CF-Connecting-IP -- set by Cloudflare, most trustworthy.
    if let Some(ip) = headers
        .get_str("cf-connecting-ip")
        .and_then(|s| s.parse::<IpAddr>().ok())
    {
        return Some(ip);
    }

    // Rightmost X-Forwarded-For -- appended by Railway's edge proxy.
    if let Some(xff) = headers.get_str("x-forwarded-for")
        && let Some(ip) = xff
            .rsplit(',')
            .next()
            .map(str::trim)
            .and_then(|s| s.parse::<IpAddr>().ok())
    {
        return Some(ip);
    }

    // Socket peer address (local dev fallback).
    connect_info.map(|addr| addr.ip())
}

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = TimeBannerError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let connect_info = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| *addr);

        resolve(&parts.headers, connect_info)
            .map(ClientIp)
            .ok_or_else(|| TimeBannerError::Internal("Unable to determine client IP".to_string()))
    }
}
