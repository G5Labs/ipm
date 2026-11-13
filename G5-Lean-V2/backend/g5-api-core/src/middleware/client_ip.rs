//! One client-address owner for throttling and request attribution.
//!
//! Transport identity is mandatory before trusting a proxy-owned XFF suffix.
//! Configuration must match every path through a trusted peer: a peer configured
//! for two entries must always append/overwrite both, including on retries.

use std::{
    convert::Infallible,
    fmt,
    net::{IpAddr, SocketAddr},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts, Request, State},
    middleware::Next,
    response::Response,
};
use http::{request::Parts, Extensions, HeaderMap};
use ipnet::IpNet;

/// Derived once by the boundary layer. Consumers never re-read forwarding headers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClientIp(Option<IpAddr>);

impl ClientIp {
    pub fn address(self) -> Option<IpAddr> {
        self.0
    }

    /// Standalone consumers without the boundary layer still ignore all headers.
    pub fn from_extensions(extensions: &Extensions) -> Self {
        extensions
            .get::<Self>()
            .copied()
            .unwrap_or_else(|| Self(peer_ip(extensions)))
    }
}

impl fmt::Display for ClientIp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(ip) => ip.fmt(f),
            None => f.write_str("unknown"),
        }
    }
}

#[async_trait::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self::from_extensions(&parts.extensions))
    }
}

fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ip)),
        ip => ip,
    }
}

fn peer_ip(extensions: &Extensions) -> Option<IpAddr> {
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|peer| canonical(peer.0.ip()))
}

fn canonical_network(network: IpNet) -> IpNet {
    if let IpNet::V6(ipv6) = network {
        // Only subnets wholly inside ::ffff:0:0/96 have an IPv4 equivalent.
        // Wider IPv6 ranges retain their address family and trust boundary.
        if ipv6.prefix_len() >= 96 {
            if let Some(ipv4) = ipv6.addr().to_ipv4_mapped() {
                return IpNet::new(IpAddr::V4(ipv4), ipv6.prefix_len() - 96)
                    .expect("mapped IPv6 prefix produces a valid IPv4 prefix");
            }
        }
    }
    network
}

/// Immutable startup policy. No private/loopback range is implicitly trusted.
#[derive(Clone, Debug)]
pub struct ClientIpPolicy {
    trusted_proxies: Vec<IpNet>,
    suffix_length: u8,
}

impl ClientIpPolicy {
    pub fn new(trusted_proxies: Vec<IpNet>, suffix_length: u8) -> Self {
        assert!(
            (1..=16).contains(&suffix_length),
            "G5_TRUSTED_PROXY_XFF_HOPS must be 1..=16"
        );
        Self {
            trusted_proxies: trusted_proxies.into_iter().map(canonical_network).collect(),
            suffix_length,
        }
    }

    fn resolve(&self, headers: &HeaderMap, extensions: &Extensions) -> ClientIp {
        let Some(peer) = peer_ip(extensions) else {
            return ClientIp(None);
        };
        if !self
            .trusted_proxies
            .iter()
            .any(|network| network.contains(&peer))
        {
            return ClientIp(Some(peer));
        }

        // Repeated field lines have the same order as a comma-joined value.
        // Parse only the attested suffix: Google preserves unverified prefixes,
        // which need not even contain valid IP literals (or UTF-8).
        let fields: Vec<_> = headers.get_all("x-forwarded-for").iter().collect();
        let mut entries = fields
            .iter()
            .rev()
            .flat_map(|field| field.as_bytes().rsplit(|b| *b == b','));
        let mut client = peer;
        for _ in 0..self.suffix_length {
            let Some(ip) = entries
                .next()
                .and_then(|entry| std::str::from_utf8(entry).ok())
                .and_then(|entry| entry.trim().parse::<IpAddr>().ok())
            else {
                return ClientIp(Some(peer));
            };
            client = canonical(ip);
        }
        ClientIp(Some(client))
    }
}

pub async fn resolve_client_ip(
    State(policy): State<ClientIpPolicy>,
    mut req: Request,
    next: Next,
) -> Response {
    let client = policy.resolve(req.headers(), req.extensions());
    req.extensions_mut().insert(client);
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    fn resolve(peer: Option<&str>, trusted: &[&str], hops: u8, fields: &[&[u8]]) -> String {
        let mut extensions = Extensions::new();
        if let Some(peer) = peer {
            extensions.insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        }
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("192.0.2.99"));
        for field in fields {
            headers.append("x-forwarded-for", HeaderValue::from_bytes(field).unwrap());
        }
        ClientIpPolicy::new(trusted.iter().map(|s| s.parse().unwrap()).collect(), hops)
            .resolve(&headers, &extensions)
            .to_string()
    }

    #[test]
    fn untrusted_and_missing_peers_cannot_assert_headers() {
        for fields in [
            vec![],
            vec![b"192.0.2.1".as_slice()],
            vec![b"anything".as_slice()],
        ] {
            assert_eq!(
                resolve(Some("203.0.113.7:10"), &[], 1, &fields),
                "203.0.113.7"
            );
            assert_eq!(
                resolve(Some("203.0.113.7:10"), &["10.0.0.0/24"], 1, &fields),
                "203.0.113.7"
            );
            assert_eq!(resolve(None, &["0.0.0.0/0"], 1, &fields), "unknown");
        }
    }

    #[test]
    fn selects_only_the_proxy_owned_suffix_across_field_lines() {
        let peer = Some("35.191.1.1:10");
        let trusted = &["35.191.0.0/16"];
        for fields in [
            vec![b"203.0.113.7, 192.0.2.8".as_slice()],
            vec![b"spoof, 203.0.113.7, 192.0.2.8".as_slice()],
            vec![
                b"198.51.100.1, nonsense".as_slice(),
                b"203.0.113.7, 192.0.2.8".as_slice(),
            ],
            vec![b"spoof, 203.0.113.7".as_slice(), b"192.0.2.8".as_slice()],
            vec![b"\xff, 203.0.113.7, 192.0.2.8".as_slice()],
        ] {
            assert_eq!(resolve(peer, trusted, 2, &fields), "203.0.113.7");
        }
        // A client in a proxy CIDR is still the client, not permission to walk
        // farther left into the caller's prefix.
        assert_eq!(
            resolve(peer, trusted, 2, &[b"spoof, 35.191.2.2, 192.0.2.8"]),
            "35.191.2.2"
        );
        assert_eq!(
            resolve(
                Some("10.0.0.2:10"),
                &["10.0.0.2/32"],
                1,
                &[b"spoof, 203.0.113.7"]
            ),
            "203.0.113.7"
        );
    }

    #[test]
    fn malformed_or_short_suffix_never_falls_back_to_other_headers() {
        for fields in [
            vec![],
            vec![b"203.0.113.7".as_slice()],
            vec![b"spoof, , 192.0.2.8".as_slice()],
            vec![b"spoof, 203.0.113.7, invalid".as_slice()],
            vec![b"203.0.113.7:80, 192.0.2.8".as_slice()],
            vec![b"[2001:db8::1], 192.0.2.8".as_slice()],
            vec![b"unknown, 192.0.2.8".as_slice()],
            vec![b"\xff, 192.0.2.8".as_slice()],
            vec![b"203.0.113.7, 192.0.2.8,".as_slice()],
        ] {
            assert_eq!(
                resolve(Some("10.0.0.2:10"), &["10.0.0.2/32"], 2, &fields),
                "10.0.0.2"
            );
        }
    }

    #[test]
    fn normalizes_ipv6_and_mapped_ipv4_for_trust_and_identity() {
        assert_eq!(
            resolve(
                Some("[::ffff:10.0.0.2]:10"),
                &["10.0.0.2/32"],
                1,
                &[b"::ffff:203.0.113.7"]
            ),
            "203.0.113.7"
        );
        assert_eq!(
            resolve(
                Some("[2001:db8::2]:10"),
                &["2001:db8::/64"],
                1,
                &[b"2001:0DB8:0000:0::7"]
            ),
            "2001:db8::7"
        );
        assert_eq!(
            resolve(Some("[::ffff:203.0.113.7]:10"), &[], 1, &[b"spoof"]),
            "203.0.113.7"
        );
    }

    #[test]
    fn mapped_proxy_networks_match_the_same_ipv4_range() {
        for (cidr, inside, outside) in [
            ("::ffff:10.0.0.2/128", "10.0.0.2", "10.0.0.3"),
            ("::ffff:10.0.0.2/120", "10.0.0.255", "10.0.1.0"),
            ("::ffff:0:0/96", "203.0.113.7", "2001:db8::7"),
        ] {
            let inside: std::net::Ipv4Addr = inside.parse().unwrap();
            for peer in [
                SocketAddr::from((inside, 10)),
                SocketAddr::from((inside.to_ipv6_mapped(), 10)),
            ] {
                assert_eq!(
                    resolve(Some(&peer.to_string()), &[cidr], 1, &[b"192.0.2.8"]),
                    "192.0.2.8",
                    "cidr={cidr}, peer={peer}"
                );
            }
            let outside: IpAddr = outside.parse().unwrap();
            assert_eq!(
                resolve(
                    Some(&SocketAddr::from((outside, 10)).to_string()),
                    &[cidr],
                    1,
                    &[b"192.0.2.8"]
                ),
                outside.to_string()
            );
        }
        // A wider IPv6 network is not a mapped-only subnet and must not
        // accidentally become an all-IPv4 trust rule.
        assert_eq!(
            resolve(
                Some("10.0.0.2:10"),
                &["::ffff:10.0.0.2/95"],
                1,
                &[b"192.0.2.8"]
            ),
            "10.0.0.2"
        );
    }

    #[test]
    fn absent_boundary_extension_is_conservative() {
        let mut extensions = Extensions::new();
        assert_eq!(
            ClientIp::from_extensions(&extensions).to_string(),
            "unknown"
        );
        extensions.insert(ConnectInfo(
            "[::ffff:203.0.113.7]:10".parse::<SocketAddr>().unwrap(),
        ));
        assert_eq!(
            ClientIp::from_extensions(&extensions).to_string(),
            "203.0.113.7"
        );
    }
}
