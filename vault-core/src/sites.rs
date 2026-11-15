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

// ── Entry level: the `sites` list, families, and "is this entry offered here" ─────────────────────

/// What a site is for. Free text is refused so the list stays something a UI can offer.
pub const KINDS: [&str; 5] = ["web", "api", "local", "other", "android_app"];

/// One address on an entry, as stored in the entry's `sites` array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySite {
    pub site: Site,
    pub mode: Mode,
    pub kind: String,
    pub label: String,
    pub source: String,
}

/// A named set of hosts that share a credential (for example MetaBrainz and its sister sites).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    pub name: String,
    pub hosts: Vec<String>,
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Exact => "exact",
        Mode::AnyPort => "any_port",
        Mode::Host => "host",
        Mode::Never => "never",
    }
}

/// The kind a bare address most likely has: a private host is `local`, an `api.` host is `api`,
/// anything else is `web`.
pub fn guess_kind(site: &Site) -> &'static str {
    if is_private_host(&site.host) {
        "local"
    } else if site.host.starts_with("api.") || site.host.contains(".api.") {
        "api"
    } else {
        "web"
    }
}

/// Builds the stored form of one site. The address is normalised, so what is stored is what is
/// matched; the path of a URL is dropped (a credential belongs to a service, not a page).
pub fn site_value(
    input: &str,
    kind: Option<&str>,
    mode: Option<&str>,
    label: &str,
    source: &str,
) -> Result<serde_json::Value, String> {
    let site = parse_site(input)?;
    let kind = match kind {
        Some(k) if KINDS.contains(&k) => k,
        Some(k) => return Err(format!("`{k}` is not a site kind ({})", KINDS.join(", "))),
        None => guess_kind(&site),
    };
    let mode = match mode {
        Some(m) => Mode::parse(m)
            .ok_or_else(|| format!("`{m}` is not a mode (exact, any_port, host, never)"))?,
        None => Mode::Exact,
    };
    Ok(serde_json::json!({
        "url": site.display(),
        "kind": kind,
        "mode": mode_name(mode),
        "label": label.trim(),
        "source": source,
        "added_at": crate::iso_now(),
    }))
}

/// Every address an entry answers to: its `sites`, plus the older `api_url` read as one more site
/// (a `web` site for a password, `api` for anything else), so nothing needs a migration.
pub fn entry_sites(entry: &serde_json::Value) -> Vec<EntrySite> {
    let mut out = Vec::new();
    if let Some(list) = entry.get("sites").and_then(|v| v.as_array()) {
        for s in list {
            let Some(url) = s.get("url").and_then(|v| v.as_str()) else {
                continue;
            };
            let Ok(site) = parse_site(url) else { continue };
            let text = |k: &str| s.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            out.push(EntrySite {
                mode: Mode::parse(&text("mode")).unwrap_or(Mode::Exact),
                kind: if text("kind").is_empty() {
                    guess_kind(&site).into()
                } else {
                    text("kind")
                },
                label: text("label"),
                source: text("source"),
                site,
            });
        }
    }
    if let Some(url) = entry.get("api_url").and_then(|v| v.as_str()) {
        if let Ok(site) = parse_site(url) {
            if !out.iter().any(|e| e.site == site) {
                let password = entry.get("secretType").and_then(|v| v.as_str()) == Some("password");
                out.push(EntrySite {
                    site,
                    mode: Mode::Exact,
                    kind: if password { "web" } else { "api" }.into(),
                    label: String::new(),
                    source: "api_url".into(),
                });
            }
        }
    }
    out
}

/// Adds `value` (from [`site_value`]) to the entry unless the same address is already there.
/// Returns whether it was added.
pub fn add_site(entry: &mut serde_json::Value, value: serde_json::Value) -> bool {
    let url = value.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let have = entry
        .get("sites")
        .and_then(|v| v.as_array())
        .is_some_and(|l| {
            l.iter()
                .any(|s| s.get("url").and_then(|v| v.as_str()) == Some(url))
        });
    if have {
        return false;
    }
    match entry.get_mut("sites").and_then(|v| v.as_array_mut()) {
        Some(list) => list.push(value),
        None => entry["sites"] = serde_json::json!([value]),
    }
    true
}

/// Removes the site that normalises to `input`. Returns whether one was removed.
pub fn remove_site(entry: &mut serde_json::Value, input: &str) -> Result<bool, String> {
    let want = parse_site(input)?.display();
    let Some(list) = entry.get_mut("sites").and_then(|v| v.as_array_mut()) else {
        return Ok(false);
    };
    let before = list.len();
    list.retain(|s| s.get("url").and_then(|v| v.as_str()) != Some(want.as_str()));
    let removed = list.len() != before;
    if list.is_empty() {
        if let Some(o) = entry.as_object_mut() {
            o.remove("sites");
        }
    }
    Ok(removed)
}

/// The families kept in the vault: `site_families: [{name, hosts}]`.
pub fn families(vault: &serde_json::Value) -> Vec<Family> {
    vault
        .get("site_families")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|f| {
                    let name = f.get("name")?.as_str()?.to_string();
                    let hosts = f
                        .get("hosts")?
                        .as_array()?
                        .iter()
                        .filter_map(|h| h.as_str().map(String::from))
                        .collect();
                    Some(Family { name, hosts })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Adds hosts to the named family (creating it). Hosts are normalised to a bare host; a scheme,
/// port or path in the input is dropped, because a family is a set of hosts.
pub fn family_add(
    vault: &mut serde_json::Value,
    name: &str,
    hosts: &[String],
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 64 || name.chars().any(|c| c.is_control()) {
        return Err("a family name is 1 to 64 characters".into());
    }
    let mut clean = Vec::new();
    for h in hosts {
        let host = parse_site(h)?.host;
        if !clean.contains(&host) {
            clean.push(host);
        }
    }
    if vault
        .get("site_families")
        .and_then(|v| v.as_array())
        .is_none()
    {
        vault["site_families"] = serde_json::json!([]);
    }
    let list = vault["site_families"].as_array_mut().expect("just made");
    match list
        .iter_mut()
        .find(|f| f.get("name").and_then(|v| v.as_str()) == Some(name))
    {
        Some(f) => {
            let mut have: Vec<String> = f["hosts"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|h| h.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            for h in clean {
                if !have.contains(&h) {
                    have.push(h);
                }
            }
            f["hosts"] = serde_json::json!(have);
        }
        None => list.push(serde_json::json!({ "name": name, "hosts": clean })),
    }
    Ok(())
}

/// Removes a host from a family, or the whole family when `host` is `None`. Returns whether
/// anything was removed.
pub fn family_remove(
    vault: &mut serde_json::Value,
    name: &str,
    host: Option<&str>,
) -> Result<bool, String> {
    let host = match host {
        Some(h) => Some(parse_site(h)?.host),
        None => None,
    };
    let Some(list) = vault
        .get_mut("site_families")
        .and_then(|v| v.as_array_mut())
    else {
        return Ok(false);
    };
    let mut changed = false;
    match host {
        None => {
            let before = list.len();
            list.retain(|f| f.get("name").and_then(|v| v.as_str()) != Some(name));
            changed = list.len() != before;
        }
        Some(h) => {
            for f in list.iter_mut() {
                if f.get("name").and_then(|v| v.as_str()) != Some(name) {
                    continue;
                }
                if let Some(hosts) = f.get_mut("hosts").and_then(|v| v.as_array_mut()) {
                    let before = hosts.len();
                    hosts.retain(|x| x.as_str() != Some(h.as_str()));
                    changed |= hosts.len() != before;
                }
            }
        }
    }
    Ok(changed)
}

/// Whether `origin` is a host of the family. The family host itself and its subdomains match; a
/// look-alike (`evilmusicbrainz.org`) does not. A public host is matched over `https` only.
pub fn family_has(family: &Family, origin: &Site) -> bool {
    if origin.scheme.as_deref() == Some("http") && !is_private_host(&origin.host) {
        return false;
    }
    family.hosts.iter().any(|h| {
        canonical_host(h) == canonical_host(&origin.host)
            || origin
                .host
                .strip_suffix(h.as_str())
                .is_some_and(|p| p.ends_with('.'))
    })
}

/// Whether the entry may be offered at `origin`: one of its sites matches, or the family it names
/// has the origin's host. A result of `true` still only means "offer it".
pub fn entry_offered(entry: &serde_json::Value, families: &[Family], origin: &Site) -> bool {
    if entry_sites(entry)
        .iter()
        .any(|e| site_matches(&e.site, e.mode, origin))
    {
        return true;
    }
    let Some(name) = entry.get("site_family").and_then(|v| v.as_str()) else {
        return false;
    };
    families
        .iter()
        .any(|f| f.name == name && family_has(f, origin))
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
    #[test]
    fn an_entry_lists_several_sites_and_reads_api_url_as_one_more() {
        let mut e =
            serde_json::json!({ "secretType": "password", "api_url": "https://login.example.org" });
        assert!(add_site(
            &mut e,
            site_value("localhost:8989", None, None, "tv", "manual").unwrap()
        ));
        let sites = entry_sites(&e);
        assert!(sites
            .iter()
            .any(|s| s.source == "api_url" && s.kind == "web"));
        assert!(sites.iter().any(|s| s.kind == "local" && s.label == "tv"));
    }

    #[test]
    fn a_loopback_spelling_is_offered_the_entry_but_another_port_is_not() {
        let mut e = serde_json::json!({});
        add_site(
            &mut e,
            site_value("localhost:8989", None, None, "", "manual").unwrap(),
        );
        assert!(entry_offered(&e, &[], &site("http://127.0.0.1:8989")));
        assert!(!entry_offered(&e, &[], &site("http://127.0.0.1:7878")));
    }

    #[test]
    fn a_family_offers_the_entry_on_every_host_and_not_on_a_lookalike() {
        let mut v = serde_json::json!({});
        family_add(
            &mut v,
            "metabrainz",
            &[
                "musicbrainz.org".into(),
                "https://listenbrainz.org/x".into(),
            ],
        )
        .unwrap();
        let fams = families(&v);
        assert_eq!(fams[0].hosts, ["musicbrainz.org", "listenbrainz.org"]);
        let e = serde_json::json!({ "site_family": "metabrainz" });
        assert!(entry_offered(&e, &fams, &site("https://listenbrainz.org")));
        assert!(entry_offered(
            &e,
            &fams,
            &site("https://beta.musicbrainz.org")
        ));
        assert!(!entry_offered(
            &e,
            &fams,
            &site("https://evilmusicbrainz.org")
        ));
        assert!(!entry_offered(&e, &fams, &site("http://listenbrainz.org")));
        assert!(family_remove(&mut v, "metabrainz", Some("listenbrainz.org")).unwrap());
        assert!(!entry_offered(
            &e,
            &families(&v),
            &site("https://listenbrainz.org")
        ));
        assert!(family_remove(&mut v, "metabrainz", None).unwrap());
    }

    #[test]
    fn a_site_with_a_credential_or_a_bad_kind_is_refused_and_remove_clears_the_field() {
        assert!(site_value("https://u:p@x.test", None, None, "", "manual").is_err());
        assert!(site_value("x.test", Some("blog"), None, "", "manual").is_err());
        let mut e = serde_json::json!({});
        add_site(
            &mut e,
            site_value("x.test", None, None, "", "manual").unwrap(),
        );
        assert!(remove_site(&mut e, "x.test").unwrap());
        assert!(e.get("sites").is_none());
    }
}
