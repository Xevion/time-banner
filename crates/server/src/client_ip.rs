//! Client IP extraction from trusted proxy headers.
//!
//! Priority: `CF-Connecting-IP` (Cloudflare) -> `X-Real-IP` (Railway) ->
//! socket peer address, gated per-source by [`TRUST`]. Not `X-Forwarded-For`:
//! Railway appends its own relay hop as the rightmost entry, so that header
//! doesn't identify the client.

use crate::error::TimeBannerError;
use crate::utils::HeaderMapExt;
use axum::extract::{ConnectInfo, FromRequestParts};
use http::HeaderMap;
use http::request::Parts;
use std::net::{IpAddr, SocketAddr};

/// The resolved client IP address.
pub struct ClientIp(pub IpAddr);

/// Which upstream proxies' client-IP headers are safe to trust. An
/// untrusted header is just something any client can forge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyTrust {
    pub cloudflare: bool,
    pub railway: bool,
}

/// `time.xevion.dev` isn't proxied through Cloudflare today (DNS points
/// straight at Railway's edge, no `cf-ray` in responses), so
/// `CF-Connecting-IP` is spoofable and untrusted.
pub const TRUST: ProxyTrust = ProxyTrust {
    cloudflare: false,
    railway: true,
};

/// Pulled out of the `FromRequestParts` impl so middleware that only has a
/// `Request` can resolve the same address ahead of the handler, e.g. for
/// geoip lookup.
pub fn resolve(
    headers: &HeaderMap,
    connect_info: Option<SocketAddr>,
    trust: ProxyTrust,
) -> Option<IpAddr> {
    if trust.cloudflare
        && let Some(ip) = headers
            .get_str("cf-connecting-ip")
            .and_then(|s| s.parse::<IpAddr>().ok())
    {
        return Some(ip);
    }

    if trust.railway
        && let Some(ip) = headers
            .get_str("x-real-ip")
            .and_then(|s| s.parse::<IpAddr>().ok())
    {
        return Some(ip);
    }

    connect_info.map(|addr| addr.ip())
}

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = TimeBannerError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let connect_info = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| *addr);

        resolve(&parts.headers, connect_info, TRUST)
            .map(ClientIp)
            .ok_or_else(|| TimeBannerError::Internal("Unable to determine client IP".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use http::HeaderValue;

    use super::*;

    const NONE: ProxyTrust = ProxyTrust {
        cloudflare: false,
        railway: false,
    };
    const BOTH: ProxyTrust = ProxyTrust {
        cloudflare: true,
        railway: true,
    };

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    fn socket(ip: &str) -> SocketAddr {
        SocketAddr::new(ip.parse().unwrap(), 443)
    }

    #[test]
    fn falls_back_to_the_socket_peer_when_no_header_is_trusted() {
        let h = headers(&[("cf-connecting-ip", "1.1.1.1"), ("x-real-ip", "2.2.2.2")]);
        check!(resolve(&h, Some(socket("9.9.9.9")), NONE) == Some("9.9.9.9".parse().unwrap()));
    }

    #[test]
    fn cloudflare_header_is_used_only_when_trusted() {
        let h = headers(&[("cf-connecting-ip", "1.1.1.1")]);
        check!(resolve(&h, None, TRUST) == None);
        let trust = ProxyTrust {
            cloudflare: true,
            railway: false,
        };
        check!(resolve(&h, None, trust) == Some("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn x_real_ip_is_used_only_when_railway_is_trusted() {
        let h = headers(&[("x-real-ip", "2.2.2.2")]);
        check!(resolve(&h, None, NONE) == None);
        check!(resolve(&h, None, TRUST) == Some("2.2.2.2".parse().unwrap()));
    }

    #[test]
    fn cloudflare_takes_priority_over_x_real_ip_when_both_are_trusted() {
        let h = headers(&[("cf-connecting-ip", "1.1.1.1"), ("x-real-ip", "2.2.2.2")]);
        check!(resolve(&h, None, BOTH) == Some("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn an_untrusted_cloudflare_header_does_not_block_the_railway_fallback() {
        let h = headers(&[("cf-connecting-ip", "1.1.1.1"), ("x-real-ip", "2.2.2.2")]);
        let railway_only = ProxyTrust {
            cloudflare: false,
            railway: true,
        };
        check!(resolve(&h, None, railway_only) == Some("2.2.2.2".parse().unwrap()));
    }

    #[test]
    fn a_malformed_trusted_header_falls_through_to_the_socket_peer() {
        let h = headers(&[("cf-connecting-ip", "not-an-ip")]);
        check!(resolve(&h, Some(socket("9.9.9.9")), TRUST) == Some("9.9.9.9".parse().unwrap()));
    }

    #[test]
    fn no_header_and_no_socket_resolves_to_nothing() {
        let h = HeaderMap::new();
        check!(resolve(&h, None, TRUST) == None);
    }

    #[test]
    fn trust_constant_reflects_the_current_deployment_topology() {
        check!(!TRUST.cloudflare);
        check!(TRUST.railway);
    }
}
