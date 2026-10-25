// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv acme issue|renew|check`: certificates from an ACME server through the user's own `lego`.
//!
//! No ACME client is reimplemented and none is bundled. `lego` runs under [`crate::tool_exec`]
//! (argv only, a cleared environment, a deadline). The DNS provider's API token is read from a vault
//! entry and handed to the child as one environment variable, so it never appears in an argument or
//! in this process's own environment. Each run uses a fresh 0700 scratch folder that is deleted when
//! the run ends, which also means each run registers a new ACME account.
//!
//! The result is a `certificate` entry: the full chain in `certificate_data`, the key in
//! `cert_key_data`, and `issued_at` / `expires_at` read from the certificate the CA signed. The
//! settings that produced it are kept in a non-secret `acme` block, so `renew` needs only the entry's
//! name. `renew` does nothing until the certificate is within its renewal window, which makes it safe
//! to run from a timer. `check` connects to a host and compares the certificate it serves with the
//! entry's.
//!
//! Needs lego 5 or newer: the flags and the layout of its output folder changed in 5.0.

use crate::access::Access;
use crate::data;
use crate::error::{CliError, CliResult};
use crate::filecred::ScratchDir;
use crate::tool_exec::{self, ToolError, ToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

pub const LETS_ENCRYPT: &str = "https://acme-v02.api.letsencrypt.org/directory";
const ISSUE_TIMEOUT: Duration = Duration::from_secs(600);
const MIN_MAJOR: u64 = 5;
/// Variables lego may need to find its CA trust and proxy settings; nothing else is inherited.
const LEGO_ENV: &[&str] = &[
    "LEGO_CA_CERTIFICATES",
    "LEGO_CA_SERVER_NAME",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "NO_PROXY",
];
/// The variable each common DNS provider reads its API token from (checked with `lego dnshelp`).
/// Any other provider needs `--token-var`.
const TOKEN_VARS: &[(&str, &str)] = &[
    ("cloudflare", "CF_DNS_API_TOKEN"),
    ("digitalocean", "DO_AUTH_TOKEN"),
    ("hetzner", "HETZNER_API_TOKEN"),
    ("duckdns", "DUCKDNS_TOKEN"),
];

/// Everything needed to ask for a certificate again. Holds no secret: the token lives in the entry
/// named by `token_entry`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub domains: Vec<String>,
    pub email: String,
    pub server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns: Option<String>,
    /// `:port` or `host:port` lego listens on for an HTTP-01 challenge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_entry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_var: Option<String>,
    /// A PEM file of extra CAs lego should trust when it talks to the ACME server (a private CA).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_file: Option<String>,
}

fn host_ok(name: &str) -> bool {
    let body = name.strip_prefix("*.").unwrap_or(name);
    !body.is_empty()
        && body.len() <= 253
        && body.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

fn env_name_ok(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

impl Settings {
    /// The variable name the DNS token goes in.
    fn resolved_token_var(&self) -> CliResult<Option<String>> {
        let Some(dns) = &self.dns else {
            return Ok(None);
        };
        if self.token_entry.is_none() {
            return Ok(None);
        }
        let var = self.token_var.clone().or_else(|| {
            TOKEN_VARS
                .iter()
                .find(|(p, _)| p == dns)
                .map(|(_, v)| v.to_string())
        });
        var.map(Some).ok_or_else(|| {
            CliError::invalid(format!(
                "name the variable `{dns}` reads its token from with --token-var (see `lego dnshelp -c {dns}`)"
            ))
        })
    }

    pub fn validate(&self) -> CliResult<()> {
        if self.domains.is_empty() {
            return Err(CliError::invalid("name at least one --domain"));
        }
        if let Some(bad) = self.domains.iter().find(|d| !host_ok(d)) {
            return Err(CliError::invalid(format!("`{bad}` is not a domain name")));
        }
        if !self.email.contains('@') || self.email.contains(char::is_whitespace) {
            return Err(CliError::invalid("--email must be an address"));
        }
        if !self.server.starts_with("https://") {
            return Err(CliError::invalid(
                "--directory must be an https:// ACME directory",
            ));
        }
        match (&self.dns, &self.http_address) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(CliError::invalid(
                    "choose one challenge: --dns <provider> or --http <address>",
                ))
            }
            (Some(dns), None) => {
                if dns.is_empty()
                    || !dns
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                {
                    return Err(CliError::invalid(format!(
                        "`{dns}` is not a DNS provider code"
                    )));
                }
            }
            (None, Some(addr)) => {
                if self.token_entry.is_some() {
                    return Err(CliError::invalid(
                        "--token-entry belongs to a --dns challenge",
                    ));
                }
                if !addr.contains(':')
                    || !addr
                        .rsplit(':')
                        .next()
                        .is_some_and(|p| p.parse::<u16>().is_ok())
                {
                    return Err(CliError::invalid("--http must be `:port` or `host:port`"));
                }
            }
        }
        if let Some(var) = &self.token_var {
            if !env_name_ok(var) {
                return Err(CliError::invalid(format!(
                    "`{var}` is not an environment variable name"
                )));
            }
            if self.token_entry.is_none() {
                return Err(CliError::invalid("--token-var needs --token-entry"));
            }
        }
        self.resolved_token_var().map(|_| ())
    }
}

fn tool_error(e: ToolError) -> CliError {
    match e {
        ToolError::Missing(_) => CliError::not_found(format!("lego: {e}")),
        _ => CliError::invalid(format!("lego: {e}")),
    }
}

/// `lego --version` prints `lego version 5.5.2 linux/amd64`.
pub fn check_version(tool: &str) -> CliResult<String> {
    let line = tool_exec::version(tool, &["--version"])
        .ok_or_else(|| tool_error(ToolError::Missing(tool.to_string())))?;
    let major = line.split_whitespace().find_map(|w| {
        w.trim_start_matches('v')
            .split('.')
            .next()?
            .parse::<u64>()
            .ok()
    });
    match major {
        Some(m) if m >= MIN_MAJOR => Ok(line),
        _ => Err(CliError::invalid(format!(
            "lego {MIN_MAJOR} or newer is required (found `{line}`)"
        ))),
    }
}

/// The file lego names after the first domain: a wildcard's `*` becomes `_`.
fn lego_file_stem(first: &str) -> String {
    first.replace('*', "_")
}

struct Obtained {
    chain_pem: String,
    key_pem: String,
    info: vault_core::certinfo::CertInfo,
}

fn obtain(access: &Access, s: &Settings, lego: &str) -> CliResult<Obtained> {
    s.validate()?;
    check_version(lego)?;
    let token = match (&s.token_entry, s.resolved_token_var()?) {
        (Some(entry), Some(var)) => Some((
            var,
            crate::service_sources::credential(access, &[], Some(entry), "api_key")?,
        )),
        _ => None,
    };
    let scratch = ScratchDir::new()?;
    let mut spec = ToolSpec::new(lego, &["run", "--accept-tos", "--no-random-sleep"]);
    for (flag, value) in [
        ("--email", s.email.as_str()),
        ("--server", s.server.as_str()),
        ("--path", &scratch.path().to_string_lossy()),
    ] {
        spec = spec.arg(flag).arg(value);
    }
    for d in &s.domains {
        spec = spec.arg("--domains").arg(d.as_str());
    }
    match (&s.dns, &s.http_address) {
        (Some(dns), _) => spec = spec.arg("--dns").arg(dns.as_str()),
        (None, Some(addr)) => spec = spec.arg("--http").arg("--http.address").arg(addr.as_str()),
        (None, None) => {}
    }
    spec = spec.pass_env(LEGO_ENV).timeout(ISSUE_TIMEOUT);
    if let Some((var, value)) = token {
        spec.set_env.push((var, value));
    }
    if let Some(ca) = &s.ca_file {
        spec.set_env
            .push(("LEGO_CA_CERTIFICATES".into(), ca.clone()));
    }
    tool_exec::run(&spec).map_err(tool_error)?;
    let stem = lego_file_stem(&s.domains[0]);
    let read = |ext: &str| {
        let path: PathBuf = scratch
            .path()
            .join("certificates")
            .join(format!("{stem}.{ext}"));
        std::fs::read_to_string(&path)
            .map_err(|e| CliError::invalid(format!("lego did not write {}: {e}", path.display())))
    };
    let chain_pem = read("crt")?;
    let key_pem = read("key")?;
    let info = vault_core::certinfo::leaf(&chain_pem).map_err(CliError::invalid)?;
    let covered = |d: &String| {
        let d = d.to_ascii_lowercase();
        info.dns_names.contains(&d)
    };
    if let Some(missing) = s.domains.iter().find(|d| !covered(d)) {
        return Err(CliError::invalid(format!(
            "the issued certificate does not cover {missing}"
        )));
    }
    Ok(Obtained {
        chain_pem,
        key_pem,
        info,
    })
}

fn summary(entry: &Value, info: &vault_core::certinfo::CertInfo, renewed: bool) -> Value {
    json!({
        "entry": entry["provider"],
        "id": entry["id"],
        "renewed": renewed,
        "domains": info.dns_names,
        "issued_at": info.not_before,
        "expires_at": info.not_after,
        "fingerprint": vault_core::tls::fingerprint_of_pem(
            entry["certificate_data"].as_str().unwrap_or_default()).unwrap_or_default(),
    })
}

fn store(access: &Access, name: &str, s: &Settings, got: &Obtained) -> CliResult<Value> {
    let mut vault = access.load_vault_or_empty()?;
    let existing = data::entries(&vault)
        .iter()
        .position(|e| e["provider"].as_str() == Some(name));
    let now = vault_core::iso_now();
    let mut entry = match existing {
        Some(i) => {
            let e = data::entries(&vault)[i].clone();
            if e["secretType"].as_str() != Some("certificate") {
                return Err(CliError::conflict(format!(
                    "'{name}' exists and is not a certificate entry"
                )));
            }
            e
        }
        None => json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "provider": name,
            "secretType": "certificate",
            "projectIds": ["Universal"],
            "categories": [],
            "scopes": [],
            "price_type": "local",
            "created_at": now,
        }),
    };
    entry["certificate_data"] = json!(got.chain_pem);
    entry["cert_key_data"] = json!(got.key_pem);
    entry["issued_at"] = json!(got.info.not_before);
    entry["issued_at_precision"] = json!("exact");
    entry["expires_at"] = json!(got.info.not_after);
    entry["acme"] = serde_json::to_value(s).map_err(|e| CliError::from(e.to_string()))?;
    let out = summary(&entry, &got.info, existing.is_some());
    match existing {
        Some(i) => data::entries_mut(&mut vault)[i] = entry,
        None => data::entries_mut(&mut vault).push(entry),
    }
    access.save(&vault)?;
    Ok(out)
}

/// Gets a certificate and saves it as the entry `name`, replacing the one already there.
pub fn issue(access: &Access, name: &str, s: &Settings, lego: &str) -> CliResult<Value> {
    let got = obtain(access, s, lego)?;
    store(access, name, s, &got)
}

/// Renews the entry `name` when it expires within `days`, or always with `force`. Returns
/// `renewed: false` and the days left when it is not due.
pub fn renew(access: &Access, name: &str, lego: &str, days: i64, force: bool) -> CliResult<Value> {
    let vault = access.load_vault()?;
    let idx = data::find_entry_index(&vault, name)?;
    let entry = &data::entries(&vault)[idx];
    let settings: Settings = entry
        .get("acme")
        .cloned()
        .ok_or_else(|| CliError::invalid(format!("'{name}' was not issued by `unv acme issue`")))
        .and_then(|v| {
            serde_json::from_value(v).map_err(|e| CliError::invalid(format!("acme settings: {e}")))
        })?;
    let current =
        vault_core::certinfo::leaf(entry["certificate_data"].as_str().unwrap_or_default())
            .map_err(CliError::invalid)?;
    let left =
        (current.not_after_epoch - time::OffsetDateTime::now_utc().unix_timestamp()) / 86_400;
    if !force && left > days {
        return Ok(json!({
            "entry": entry["provider"],
            "renewed": false,
            "days_left": left,
            "expires_at": current.not_after,
        }));
    }
    let provider = entry["provider"].as_str().unwrap_or(name).to_string();
    let got = obtain(access, &settings, lego)?;
    store(access, &provider, &settings, &got)
}

/// The certificate a TLS server presents for `sni` at `host:port`: no credential is sent and
/// nothing is verified, only its fingerprint is read.
pub fn served_fingerprint(host: &str, port: u16, sni: &str) -> CliResult<String> {
    use vault_core::tls::rustls;
    let (config, seen) = vault_core::tls::capturing_config().map_err(CliError::from)?;
    let name = rustls::pki_types::ServerName::try_from(sni.to_string())
        .map_err(|_| CliError::invalid(format!("`{sni}` is not a valid server name")))?;
    let mut conn = rustls::ClientConnection::new(std::sync::Arc::new(config), name)
        .map_err(|e| CliError::from(e.to_string()))?;
    let addr = std::net::ToSocketAddrs::to_socket_addrs(&(host, port))
        .map_err(|e| CliError::unavailable(format!("Cannot resolve {host}: {e}")))?
        .next()
        .ok_or_else(|| CliError::unavailable(format!("{host} has no address")))?;
    let mut tcp = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(10))
        .map_err(|e| CliError::unavailable(format!("Cannot reach {host}:{port}: {e}")))?;
    tcp.set_read_timeout(Some(Duration::from_secs(10))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(10))).ok();
    // The handshake is all that is needed: the verifier records the certificate as it arrives.
    while conn.is_handshaking() {
        conn.complete_io(&mut tcp).map_err(|e| {
            CliError::unavailable(format!("TLS handshake with {host}:{port} failed: {e}"))
        })?;
    }
    let fp = seen.lock().ok().and_then(|g| g.clone());
    fp.ok_or_else(|| CliError::unavailable(format!("{host}:{port} presented no certificate")))
}

/// Compares the certificate served at `host:port` with the entry's. A mismatch is a conflict: the
/// deployment did not take, or something else answers on that port.
pub fn check(
    access: &Access,
    name: &str,
    host: &str,
    port: u16,
    sni: Option<&str>,
) -> CliResult<Value> {
    let vault = access.load_vault()?;
    let idx = data::find_entry_index(&vault, name)?;
    let entry = &data::entries(&vault)[idx];
    let expected =
        vault_core::tls::fingerprint_of_pem(entry["certificate_data"].as_str().unwrap_or_default())
            .map_err(CliError::invalid)?;
    let sni = sni.unwrap_or(host);
    let served = served_fingerprint(host, port, sni)?;
    if served != expected {
        return Err(CliError::conflict(format!(
            "{host}:{port} serves {served}, the entry holds {expected}"
        )));
    }
    Ok(
        json!({ "entry": entry["provider"], "host": host, "port": port, "fingerprint": served, "matches": true }),
    )
}
