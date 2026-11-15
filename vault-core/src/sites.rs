// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Sites: the places a credential is used (roadmap R40).
//!
//! A password manager keys a login to one URL. This vault keys it to an entry (a stable id and a
//! name) and lets the entry list several places: a login page, an API address, a service on the
//! LAN with its own port. A [`Site`] is only a normalised address. Nothing here reads a secret.
//!
//! Three rules decide whether a page may be offered a credential, and each is here because a wrong
//! answer fills a password into the wrong place:
//!
//! * The host is compared after normalisation (lower case, no trailing dot, punycode), so
//!   `Example.COM.` and `example.com` are one site and `paypa1.com` is not `paypal.com`.
//! * Every loopback spelling is one host (`localhost`, `*.localhost`, `127.0.0.0/8`, `::1`), so a
//!   password saved for `localhost:8989` also answers at `127.0.0.1:8989`. The port stays
//!   significant: Sonarr and Radarr share a host and are different services.
//! * A site stored without a scheme matches `https`, and matches `http` only on a private or
//!   loopback host, so a public site is never downgraded to plain text by a missing scheme.
//!
//! The Rust side is the only implementation (invariant 10): the desktop reaches it over IPC and the
//! extension host calls it directly.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::{Host, Url};

/// The longest address accepted. A URL in a vault is a hint, not a document.
const MAX_LEN: usize = 2048;

/// A normalised address: scheme if one was given, host, port if one was given. The path, query and
/// fragment are dropped, because a credential belongs to a service and not to a page of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// How strictly an origin must equal a stored site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Same host, same scheme and same port (the default).
    Exact,
    /// Same host and scheme, any port: a service that moves between ports.
    AnyPort,
    /// Same host, any scheme and any port. The downgrade rule still applies.
    Host,
    /// Never offered here, whatever else matches.
    Never,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exact" => Some(Self::Exact),
            "any_port" => Some(Self::AnyPort),
            "host" => Some(Self::Host),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

impl Site {
    /// `scheme://host:port` as far as each part is known; an IPv6 host is bracketed.
    pub fn display(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let mut out = match &self.scheme {
            Some(s) => format!("{s}://{host}"),
            None => host,
        };
        if let Some(p) = self.port {
            out.push_str(&format!(":{p}"));
        }
        out
    }
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" => Some(80),
        "https" => Some(443),
        "ssh" | "sftp" => Some(22),
        "ftp" => Some(21),
        "postgres" | "postgresql" => Some(5432),
        "mysql" | "mariadb" => Some(3306),
        "redis" | "rediss" => Some(6379),
        "mongodb" => Some(27017),
        _ => None,
    }
}

fn valid_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Normalises `input`: a full URL (`https://sonarr.home:8989/app`), an address and port
/// (`192.168.1.20:8989`), or a bare host (`localhost`).
///
/// A user name or password inside the address is refused: a credential in a URL is what this
/// vault exists to keep out of URLs.
pub fn parse_site(input: &str) -> Result<Site, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("the address is empty".into());
    }
    if s.len() > MAX_LEN {
        return Err(format!("the address is over {MAX_LEN} characters"));
    }
    if s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("an address holds no spaces or control characters".into());
    }
    let (scheme, rest) = match s.split_once("://") {
        Some((scheme, rest)) => {
            if !valid_scheme(scheme) {
                return Err(format!("`{scheme}` is not a scheme"));
            }
            (Some(scheme.to_ascii_lowercase()), rest)
        }
        None => (None, s),
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() {
        return Err("the address has no host".into());
    }
    if authority.contains('@') {
        return Err("an address with a user name or password is not stored".into());
    }
    // The port is read from the text: the URL parser drops a port that is the default of http.
    let (host_part, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !authority.ends_with(']') && !h.is_empty() && !h.ends_with(':') => {
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err("the port is not a number".into());
            }
            let port: u16 = p
                .parse()
                .map_err(|_| "the port is over 65535".to_string())?;
            if port == 0 {
                return Err("port 0 is not an address".into());
            }
            (h, Some(port))
        }
        _ => (authority, None),
    };
    let parsed =
        Url::parse(&format!("http://{host_part}")).map_err(|e| format!("not an address: {e}"))?;
    let host = match parsed.host() {
        Some(Host::Domain(d)) => d.trim_end_matches('.').to_string(),
        Some(Host::Ipv4(a)) => a.to_string(),
        Some(Host::Ipv6(a)) => a.to_string(),
        None => return Err("the address has no host".into()),
    };
    if host.is_empty() {
        return Err("the address has no host".into());
    }
    Ok(Site { scheme, host, port })
}

/// True for every spelling of this machine: `localhost`, `*.localhost` (RFC 6761), `127.0.0.0/8`,
/// `::1` and its IPv4-mapped form.
pub fn is_loopback(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => a.is_loopback(),
        Ok(IpAddr::V6(a)) => a.is_loopback() || a.to_ipv4_mapped().is_some_and(|v| v.is_loopback()),
        Err(_) => false,
    }
}

fn private_v4(a: Ipv4Addr) -> bool {
    a.is_private() || a.is_link_local() || a.is_loopback()
}

fn private_v6(a: Ipv6Addr) -> bool {
    let s = a.segments();
    a.is_loopback() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
}

/// A host that is not on the public internet: loopback, RFC 1918 and link-local addresses, a
/// single-label name, and the names a home network uses (`.local`, `.lan`, `.home`, `.internal`).
pub fn is_private_host(host: &str) -> bool {
    if is_loopback(host) {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => private_v4(a),
        Ok(IpAddr::V6(a)) => private_v6(a),
        Err(_) => {
            !host.contains('.')
                || [
                    ".local",
                    ".lan",
                    ".home",
                    ".internal",
                    ".home.arpa",
                    ".localdomain",
                ]
                .iter()
                .any(|suffix| host.ends_with(suffix))
        }
    }
}

/// The host as it is compared: every loopback spelling becomes one marker that no real host can
/// equal.
fn canonical_host(host: &str) -> &str {
    if is_loopback(host) {
        "@loopback"
    } else {
        host
    }
}

/// Whether a page at `origin` may be offered a credential stored for `stored`.
///
/// `origin` is what a browser reports: it always has a scheme. A result of `false` is final; a
/// result of `true` still only means "offer it", and the caller decides about filling.
pub fn site_matches(stored: &Site, mode: Mode, origin: &Site) -> bool {
    if mode == Mode::Never {
        return false;
    }
    let Some(origin_scheme) = origin.scheme.as_deref() else {
        return false;
    };
    if canonical_host(&stored.host) != canonical_host(&origin.host) {
        return false;
    }
    // A missing scheme never downgrades a public site to plain text.
    let plain_text = origin_scheme == "http";
    if plain_text && stored.scheme.as_deref() != Some("http") && !is_private_host(&origin.host) {
        return false;
    }
    if mode == Mode::Host {
        return true;
    }
    if let Some(stored_scheme) = stored.scheme.as_deref() {
        if stored_scheme != origin_scheme {
            return false;
        }
    }
    if mode == Mode::AnyPort {
        return true;
    }
    let scheme_for_port = stored.scheme.as_deref().unwrap_or(origin_scheme);
    let stored_port = stored.port.or_else(|| default_port(scheme_for_port));
    let origin_port = origin.port.or_else(|| default_port(origin_scheme));
    stored_port == origin_port
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(s: &str) -> Site {
        parse_site(s).unwrap()
    }

    fn matches(stored: &str, mode: Mode, origin: &str) -> bool {
        site_matches(&site(stored), mode, &site(origin))
    }

    #[test]
    fn an_address_is_reduced_to_scheme_host_and_port() {
        let s = site("HTTPS://Sonarr.Home:8989/app/login?x=1#top");
        assert_eq!(s.scheme.as_deref(), Some("https"));
        assert_eq!(s.host, "sonarr.home");
        assert_eq!(s.port, Some(8989));
        assert_eq!(s.display(), "https://sonarr.home:8989");
    }

    #[test]
    fn an_address_and_a_port_without_a_scheme_is_accepted() {
        let s = site("192.168.1.20:8989");
        assert_eq!(s.scheme, None);
        assert_eq!(s.host, "192.168.1.20");
        assert_eq!(s.port, Some(8989));
        assert_eq!(site("localhost").port, None);
    }

    #[test]
    fn a_trailing_dot_and_capitals_do_not_make_a_second_site() {
        assert_eq!(site("Example.COM.").host, "example.com");
        assert_eq!(site("https://example.com:443/").port, Some(443));
    }

    #[test]
    fn an_international_name_is_stored_as_punycode() {
        assert_eq!(site("https://b\u{fc}cher.de").host, "xn--bcher-kva.de");
    }

    #[test]
    fn ipv6_hosts_keep_their_brackets_only_for_display() {
        let s = site("[::1]:8080");
        assert_eq!(s.host, "::1");
        assert_eq!(s.port, Some(8080));
        assert_eq!(s.display(), "[::1]:8080");
    }

    #[test]
    fn what_is_not_an_address_is_refused_with_a_reason() {
        for bad in [
            "",
            "   ",
            "https://",
            "javascript:alert(1)",
            "https://user:secret@example.com",
            "https://exam ple.com",
            "example.com:notaport",
            "example.com:99999",
            "example.com:0",
            "1http://example.com",
        ] {
            assert!(parse_site(bad).is_err(), "{bad:?} should be refused");
        }
        let long = format!("https://{}.com", "a".repeat(MAX_LEN));
        assert!(parse_site(&long).is_err());
    }

    /// The disparity the maintainer named: a password saved for one spelling of this machine did
    /// not appear at another.
    #[test]
    fn every_loopback_spelling_is_one_host() {
        let stored = "http://localhost:8989";
        for origin in [
            "http://localhost:8989",
            "http://127.0.0.1:8989",
            "http://127.1.2.3:8989",
            "http://[::1]:8989",
            "http://sonarr.localhost:8989",
        ] {
            assert!(matches(stored, Mode::Exact, origin), "{origin}");
        }
        assert!(matches(
            "http://127.0.0.1:8989",
            Mode::Exact,
            "http://localhost:8989"
        ));
    }

    /// Sonarr and Radarr share a host and are different services: the port stays significant.
    #[test]
    fn a_different_port_is_a_different_service() {
        assert!(!matches(
            "http://localhost:8989",
            Mode::Exact,
            "http://localhost:7878"
        ));
        assert!(matches(
            "http://localhost:8989",
            Mode::AnyPort,
            "http://localhost:7878"
        ));
        assert!(!matches(
            "http://localhost:8989",
            Mode::AnyPort,
            "https://localhost:8989"
        ));
        assert!(matches(
            "http://localhost:8989",
            Mode::Host,
            "https://localhost:7878"
        ));
    }

    #[test]
    fn a_lookalike_or_a_neighbour_host_does_not_match() {
        assert!(!matches(
            "https://paypal.com",
            Mode::Exact,
            "https://paypa1.com"
        ));
        assert!(!matches(
            "https://example.com",
            Mode::Exact,
            "https://evil.example.com"
        ));
        assert!(!matches(
            "https://example.com",
            Mode::Exact,
            "https://example.com.evil.net"
        ));
        assert!(!matches(
            "https://localhost",
            Mode::Exact,
            "https://localhost.evil.net"
        ));
        assert!(matches(
            "https://example.com",
            Mode::Exact,
            "https://EXAMPLE.com."
        ));
    }

    #[test]
    fn default_ports_are_equal_to_no_port() {
        assert!(matches(
            "https://example.com",
            Mode::Exact,
            "https://example.com:443"
        ));
        assert!(matches(
            "https://example.com:443",
            Mode::Exact,
            "https://example.com"
        ));
        assert!(!matches(
            "https://example.com",
            Mode::Exact,
            "https://example.com:8443"
        ));
        assert!(matches("ssh://box:22", Mode::Exact, "ssh://box"));
    }

    /// A site saved without a scheme must not downgrade a public site to plain text.
    #[test]
    fn a_missing_scheme_matches_https_everywhere_and_http_only_on_a_private_host() {
        assert!(matches("example.com", Mode::Exact, "https://example.com"));
        assert!(!matches("example.com", Mode::Exact, "http://example.com"));
        assert!(!matches("example.com", Mode::Host, "http://example.com"));
        assert!(matches(
            "sonarr.home:8989",
            Mode::Exact,
            "http://sonarr.home:8989"
        ));
        assert!(matches(
            "192.168.1.20:8989",
            Mode::Exact,
            "http://192.168.1.20:8989"
        ));
        assert!(matches("nas", Mode::Exact, "http://nas"));
        // Written out, it is the maintainer's choice.
        assert!(matches(
            "http://example.com",
            Mode::Exact,
            "http://example.com"
        ));
        // Never a downgrade from a stored https.
        assert!(!matches(
            "https://example.com",
            Mode::Host,
            "http://example.com"
        ));
    }

    #[test]
    fn never_overrides_everything_and_an_origin_needs_a_scheme() {
        assert!(!matches(
            "https://example.com",
            Mode::Never,
            "https://example.com"
        ));
        let bare = site("example.com");
        assert!(!site_matches(&site("example.com"), Mode::Exact, &bare));
    }

    #[test]
    fn private_hosts_are_recognised() {
        for h in [
            "localhost",
            "10.0.0.5",
            "172.16.3.4",
            "192.168.0.1",
            "169.254.1.1",
            "::1",
            "fd00::1",
            "nas",
            "printer.local",
            "media.home.arpa",
        ] {
            assert!(is_private_host(h), "{h}");
        }
        for h in [
            "example.com",
            "8.8.8.8",
            "2001:4860:4860::8888",
            "evil.localhost.com",
        ] {
            assert!(!is_private_host(h), "{h}");
        }
    }

    #[test]
    fn modes_parse_by_name() {
        assert_eq!(Mode::parse("exact"), Some(Mode::Exact));
        assert_eq!(Mode::parse("any_port"), Some(Mode::AnyPort));
        assert_eq!(Mode::parse("host"), Some(Mode::Host));
        assert_eq!(Mode::parse("never"), Some(Mode::Never));
        assert_eq!(Mode::parse("base_domain"), None);
    }
}
