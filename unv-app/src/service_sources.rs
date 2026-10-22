// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Import from secrets services: OpenBao (KV v2), Infisical and Bitwarden Secrets Manager.
//!
//! A service is read the way a person would: the API of the user's own instance, with the user's own
//! credential. HTTP goes through the issuer client (verified TLS, no redirects, so a token is never
//! forwarded to wherever a redirect points). Every reader builds the complete result before
//! anything reaches the vault, so a network drop in the middle of a listing is an error and nothing
//! is half-applied. A path the credential may not read is reported and skipped, not fatal.
//!
//! What cannot be operational is archived, not imported: OpenBao policies, auth methods and
//! non-KV engines; dynamic secrets are reported by name only. Server and API versions are recorded
//! in the report, because these APIs drift.
//!
//! Bitwarden Secrets Manager is read through the user's own `bws` program only, and the Bitwarden SDK
//! is never linked (its licence is not OSI approved).

use crate::password_sources::{
    base_entry, classify, extra, hash, source_item, Archive, ArchivedItem, Item, Parsed,
    SourceFormat,
};
use crate::tool_exec::{self, ToolSpec};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const LIST_PAGE: usize = 100;
const MAX_SECRETS: usize = 50_000;

/// Where a service's credential comes from has already been decided by the caller (an environment
/// variable or a vault entry); readers receive the value only.
#[derive(Debug, Clone)]
pub struct OpenBao {
    pub addr: String,
    pub mount: String,
    pub path: String,
    pub namespace: Option<String>,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct Infisical {
    pub addr: String,
    pub project_id: String,
    pub environment: String,
    pub secret_path: String,
    pub auth: InfisicalAuth,
}

#[derive(Debug, Clone)]
pub enum InfisicalAuth {
    /// A machine identity (universal auth): exchanged for a short-lived access token.
    Identity {
        client_id: String,
        client_secret: String,
    },
    /// An access token the user already holds.
    Token(String),
}

#[derive(Debug, Clone)]
pub struct Bws {
    pub program: String,
    pub token: String,
    pub project: Option<String>,
}

struct Http {
    client: reqwest::blocking::Client,
    base: String,
    headers: Vec<(String, String)>,
}

impl Http {
    fn new(base: &str, headers: Vec<(String, String)>) -> Result<Self, String> {
        let client = crate::tls::build_issuer_client(HTTP_TIMEOUT).map_err(|e| e.message)?;
        Ok(Self {
            client,
            base: base.trim_end_matches('/').to_string(),
            headers,
        })
    }

    /// `Ok(None)` for 404, an error naming the status for anything else that is not 2xx. A 403 is
    /// returned as `Err(Denied)` so the caller can decide it is not fatal.
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Option<Value>, HttpError> {
        let mut url = format!("{}{path}", self.base);
        if !query.is_empty() {
            let pairs: Vec<String> = query
                .iter()
                .map(|(k, v)| format!("{}={}", percent(k), percent(v)))
                .collect();
            url.push('?');
            url.push_str(&pairs.join("&"));
        }
        let mut req = self.client.get(url);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        let resp = req.send().map_err(|e| {
            HttpError::Other(format!("cannot reach {}: {}", self.base, e.without_url()))
        })?;
        let status = resp.status();
        if status.as_u16() == 404 {
            return Ok(None);
        }
        if status.as_u16() == 403 || status.as_u16() == 401 {
            return Err(HttpError::Denied(status.as_u16()));
        }
        if !status.is_success() {
            return Err(HttpError::Other(format!(
                "{} answered HTTP {status}",
                self.base
            )));
        }
        resp.json::<Value>()
            .map(Some)
            .map_err(|_| HttpError::Other(format!("{} did not return JSON", self.base)))
    }

    fn post(&self, path: &str, body: &Value) -> Result<Value, String> {
        let mut req = self.client.post(format!("{}{path}", self.base)).json(body);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        let resp = req
            .send()
            .map_err(|e| format!("cannot reach {}: {}", self.base, e.without_url()))?;
        if !resp.status().is_success() {
            return Err(format!("{} answered HTTP {}", self.base, resp.status()));
        }
        resp.json::<Value>()
            .map_err(|_| format!("{} did not return JSON", self.base))
    }
}

/// Percent-encodes a query component (RFC 3986 unreserved characters pass through).
fn percent(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

enum HttpError {
    Denied(u16),
    Other(String),
}

impl HttpError {
    fn message(self, what: &str) -> String {
        match self {
            Self::Denied(code) => format!("{what}: HTTP {code} (this credential may not read it)"),
            Self::Other(e) => format!("{what}: {e}"),
        }
    }
}

fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn dir_category(name: &str) -> Option<String> {
    name.rsplit_once('/').map(|(d, _)| d.to_string())
}

// ── OpenBao KV v2 ───────────────────────────────────────────────────────────

fn bao_list(
    http: &Http,
    mount: &str,
    prefix: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    let mut secrets = Vec::new();
    let mut stack = vec![prefix.trim_matches('/').to_string()];
    while let Some(dir) = stack.pop() {
        let mut after = String::new();
        loop {
            let path = if dir.is_empty() {
                format!("/v1/{mount}/metadata/")
            } else {
                format!("/v1/{mount}/metadata/{dir}/")
            };
            let limit = LIST_PAGE.to_string();
            let mut query = vec![("list", "true"), ("limit", limit.as_str())];
            if !after.is_empty() {
                query.push(("after", after.as_str()));
            }
            let body = match http.get(&path, &query) {
                Ok(Some(b)) => b,
                Ok(None) => break,
                Err(HttpError::Denied(code)) => {
                    warnings.push(format!(
                        "{mount}/{dir}: HTTP {code}, not readable with this token; skipped"
                    ));
                    break;
                }
                Err(e) => return Err(e.message(&format!("listing {mount}/{dir}"))),
            };
            let keys: Vec<String> = body["data"]["keys"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|k| k.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            for key in &keys {
                let joined = if dir.is_empty() {
                    key.clone()
                } else {
                    format!("{dir}/{key}")
                };
                if let Some(folder) = joined.strip_suffix('/') {
                    stack.push(folder.to_string());
                } else {
                    secrets.push(joined);
                    if secrets.len() > MAX_SECRETS {
                        return Err(format!("more than {MAX_SECRETS} secrets under {mount}"));
                    }
                }
            }
            if keys.len() < LIST_PAGE {
                break;
            }
            after = keys.last().cloned().unwrap_or_default();
        }
    }
    secrets.sort();
    secrets.dedup();
    Ok(secrets)
}

pub(crate) fn read_openbao(cfg: &OpenBao) -> Result<Parsed, String> {
    let mut headers = vec![("X-Vault-Token".to_string(), cfg.token.clone())];
    if let Some(ns) = &cfg.namespace {
        headers.push(("X-Vault-Namespace".to_string(), ns.clone()));
    }
    let http = Http::new(&cfg.addr, headers)?;
    let mount = cfg.mount.trim_matches('/').to_string();
    let mut warnings = Vec::new();
    if let Ok(Some(health)) = http.get("/v1/sys/health", &[("standbyok", "true")]) {
        if let Some(v) = health["version"].as_str() {
            warnings.push(format!("read from OpenBao/Vault {v}"));
        }
    }
    let paths = bao_list(&http, &mount, &cfg.path, &mut warnings)?;
    let mut items = Vec::new();
    let mut archived = Vec::new();
    let mut manifest: Vec<Value> = Vec::new();
    for path in &paths {
        let Some(meta) = http
            .get(&format!("/v1/{mount}/metadata/{path}"), &[])
            .map_err(|e| e.message(&format!("metadata of {path}")))?
        else {
            continue;
        };
        let current = meta["data"]["current_version"].as_u64().unwrap_or(0);
        let versions = meta["data"]["versions"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut by_version: BTreeMap<u64, Value> = BTreeMap::new();
        let mut reports = Vec::new();
        for (num, info) in &versions {
            let Ok(version) = num.parse::<u64>() else {
                continue;
            };
            let destroyed = info["destroyed"].as_bool().unwrap_or(false);
            let deleted = info["deletion_time"]
                .as_str()
                .is_some_and(|t| !t.is_empty());
            if destroyed || deleted {
                reports.push(format!(
                    "version {version} was {}; its value is not recoverable",
                    if destroyed { "destroyed" } else { "deleted" }
                ));
                continue;
            }
            let body = http
                .get(
                    &format!("/v1/{mount}/data/{path}"),
                    &[("version", num.as_str())],
                )
                .map_err(|e| e.message(&format!("{path} version {version}")))?;
            if let Some(body) = body {
                by_version.insert(
                    version,
                    json!({
                        "data": body["data"]["data"].clone(),
                        "created_time": info["created_time"].clone(),
                    }),
                );
            }
        }
        let source_id = format!("{mount}/{path}");
        let Some(latest) = by_version.get(&current) else {
            archived.push(ArchivedItem {
                source_id: source_id.clone(),
                source_hash: hash(serde_json::to_string(&meta["data"]).unwrap_or_default().as_bytes()),
                name: path.clone(),
                archive: Archive {
                    source_id: Some(source_id),
                    reason: "OpenBao secret whose current version is deleted or destroyed: metadata only".into(),
                    media_type: "application/json".into(),
                    bytes: serde_json::to_vec(&meta["data"]).unwrap_or_default(),
                },
            });
            continue;
        };
        let data = latest["data"].as_object().cloned().unwrap_or_default();
        let extra_vars: Vec<Value> = data
            .iter()
            .map(|(k, v)| extra(k.clone(), str_of(v), false))
            .collect();
        // Older versions, newest first, each as the history record the vault already understands.
        let mut history: Vec<Value> = Vec::new();
        for (version, snapshot) in by_version.iter().rev() {
            if *version == current {
                continue;
            }
            let saved = snapshot["created_time"].as_str().unwrap_or("").to_string();
            if let Some(map) = snapshot["data"].as_object() {
                for (k, v) in map {
                    history.push(json!({
                        "value": str_of(v),
                        "saved_at": saved,
                        "field": format!("extra_vars/{k}"),
                    }));
                }
            }
        }
        manifest.push(json!({ "path": source_id, "version": current }));
        let mut entry = base_entry(path.clone(), String::new(), "env_var");
        if let Some(dir) = dir_category(path) {
            entry["categories"] = json!([dir]);
        }
        let source = json!({ "path": source_id, "version": current, "data": latest["data"] });
        items.push(source_item(
            source_id,
            path.clone(),
            source,
            entry,
            Vec::new(),
            vec!["openbao".into()],
            extra_vars,
            None,
            history,
            Vec::new(),
            reports,
        ));
    }
    // Engines, auth methods and policies are archived as provenance, never made operational.
    if let Ok(Some(mounts)) = http.get("/v1/sys/mounts", &[]) {
        let dynamic: Vec<String> = mounts["data"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter(|(_, v)| {
                        !matches!(
                            v["type"].as_str(),
                            Some("kv" | "system" | "identity" | "cubbyhole")
                        )
                    })
                    .map(|(k, v)| format!("{k}({})", v["type"].as_str().unwrap_or("?")))
                    .collect()
            })
            .unwrap_or_default();
        if !dynamic.is_empty() {
            warnings.push(format!(
                "dynamic or non-KV engines were reported, not imported: {}",
                dynamic.join(", ")
            ));
        }
        archived.push(ArchivedItem {
            source_id: "sys/mounts".into(),
            source_hash: hash(
                serde_json::to_string(&mounts["data"])
                    .unwrap_or_default()
                    .as_bytes(),
            ),
            name: "sys/mounts".into(),
            archive: Archive {
                source_id: None,
                reason: "OpenBao mounts retained as non-operational provenance".into(),
                media_type: "application/json".into(),
                bytes: serde_json::to_vec(&mounts["data"]).unwrap_or_default(),
            },
        });
    }
    if items.is_empty() && archived.iter().all(|a| a.source_id == "sys/mounts") {
        return Err(format!("no secrets found under {mount}/{}", cfg.path));
    }
    Ok(Parsed {
        format: SourceFormat::OpenBao,
        label: format!("{}/{mount}", cfg.addr.trim_end_matches('/')),
        source_bytes: serde_json::to_vec(&json!({ "secrets": manifest })).unwrap_or_default(),
        items,
        archived,
        warnings,
        missing_at_source: Vec::new(),
    })
}

// ── Infisical ───────────────────────────────────────────────────────────────

/// `${KEY}`, `${env.KEY}` and `${env.folder.KEY}` inside an unexpanded value. Returned without the
/// dollar-brace so the text can be stored where UnENVerse would otherwise try to resolve it.
fn infisical_refs(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(i) = rest.find("${") {
        let after = &rest[i + 2..];
        let Some(end) = after.find('}') else { break };
        let name = after[..end].trim();
        if !name.is_empty() && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
        rest = &after[end + 1..];
    }
    out
}

pub(crate) fn read_infisical(cfg: &Infisical) -> Result<Parsed, String> {
    let anonymous = Http::new(&cfg.addr, Vec::new())?;
    let token = match &cfg.auth {
        InfisicalAuth::Token(t) => t.clone(),
        InfisicalAuth::Identity {
            client_id,
            client_secret,
        } => {
            let body = anonymous.post(
                "/api/v1/auth/universal-auth/login",
                &json!({ "clientId": client_id, "clientSecret": client_secret }),
            )?;
            body["accessToken"]
                .as_str()
                .ok_or("Infisical login returned no access token")?
                .to_string()
        }
    };
    let http = Http::new(
        &cfg.addr,
        vec![("Authorization".into(), format!("Bearer {token}"))],
    )?;
    let fetch = |expand: bool| -> Result<Value, String> {
        http.get(
            "/api/v4/secrets",
            &[
                ("projectId", cfg.project_id.as_str()),
                ("environment", cfg.environment.as_str()),
                ("secretPath", cfg.secret_path.as_str()),
                ("recursive", "true"),
                (
                    "expandSecretReferences",
                    if expand { "true" } else { "false" },
                ),
                ("includeImports", "true"),
            ],
        )
        .map_err(|e| e.message("reading secrets"))?
        .ok_or_else(|| "Infisical project, environment or path not found".to_string())
    };
    let expanded = fetch(true)?;
    let raw = fetch(false)?;
    let secrets = expanded["secrets"].as_array().cloned().unwrap_or_default();
    if secrets.is_empty() {
        return Err("Infisical returned no secrets for that project, environment and path".into());
    }
    let raw_values: BTreeMap<(String, String), String> = raw["secrets"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| {
                    (
                        (
                            s["secretPath"].as_str().unwrap_or("/").to_string(),
                            s["secretKey"].as_str().unwrap_or("").to_string(),
                        ),
                        s["secretValue"].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    // One entry per folder path: the variables of that folder, expanded.
    let mut by_path: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for s in &secrets {
        by_path
            .entry(s["secretPath"].as_str().unwrap_or("/").to_string())
            .or_default()
            .push(s);
    }
    let mut items = Vec::new();
    let mut graph_lines = Vec::new();
    for (path, group) in &by_path {
        let name = format!(
            "{}/{}{}",
            cfg.project_id,
            cfg.environment,
            if path == "/" {
                String::new()
            } else {
                path.clone()
            }
        );
        let mut extra_vars = Vec::new();
        let mut comments = Vec::new();
        let mut reports = Vec::new();
        for s in group {
            let key = s["secretKey"].as_str().unwrap_or("").to_string();
            extra_vars.push(extra(
                key.clone(),
                s["secretValue"].as_str().unwrap_or(""),
                false,
            ));
            if let Some(c) = s["secretComment"].as_str().filter(|c| !c.is_empty()) {
                comments.push(format!("{key}: {c}"));
            }
            let raw_value = raw_values
                .get(&(path.clone(), key.clone()))
                .cloned()
                .unwrap_or_default();
            let refs = infisical_refs(&raw_value);
            if !refs.is_empty() {
                let line = format!("{key} refers to {}", refs.join(", "));
                graph_lines.push(format!("{name}: {line}"));
                reports.push(format!("{line} (values were expanded on import)"));
            }
        }
        let mut entry = base_entry(name.clone(), String::new(), "env_var");
        if !comments.is_empty() {
            entry["description"] = json!(comments.join("\n"));
        }
        let source = json!({
            "path": path,
            "secrets": group.iter().map(|s| json!({
                "key": s["secretKey"], "value": s["secretValue"], "version": s["version"],
                "type": s["type"],
            })).collect::<Vec<_>>(),
        });
        items.push(source_item(
            name.clone(),
            name,
            source,
            entry,
            Vec::new(),
            vec!["infisical".into()],
            extra_vars,
            None,
            Vec::new(),
            Vec::new(),
            reports,
        ));
    }
    let mut warnings = vec![format!(
        "read from Infisical project {}, environment {}",
        cfg.project_id, cfg.environment
    )];
    if let Some(imports) = expanded["imports"].as_array().filter(|i| !i.is_empty()) {
        warnings.push(format!(
            "{} secret import(s) were included in the expanded values",
            imports.len()
        ));
    }
    Ok(Parsed {
        format: SourceFormat::Infisical,
        label: format!(
            "{}/{}/{}",
            cfg.addr.trim_end_matches('/'),
            cfg.project_id,
            cfg.environment
        ),
        source_bytes: serde_json::to_vec(&json!({ "reference_graph": graph_lines }))
            .unwrap_or_default(),
        items,
        archived: Vec::new(),
        warnings,
        missing_at_source: Vec::new(),
    })
}

// ── Bitwarden Secrets Manager, through `bws` ────────────────────────────────

pub(crate) fn read_bws(cfg: &Bws) -> Result<Parsed, String> {
    let run = |args: &[&str]| -> Result<Vec<u8>, String> {
        let mut spec = ToolSpec::new(&cfg.program, args)
            .timeout(Duration::from_secs(60))
            .max_stdout(32 * 1024 * 1024);
        // The token reaches the child through its environment for this call only.
        spec.set_env
            .push(("BWS_ACCESS_TOKEN".into(), cfg.token.clone()));
        tool_exec::run(&spec)
            .map(|o| o.stdout)
            .map_err(|e| format!("bws: {e}"))
    };
    let mut warnings = Vec::new();
    if let Some(v) = tool_exec::version(&cfg.program, &["--version"]) {
        warnings.push(format!("read with {v}"));
    }
    let projects: Value = serde_json::from_slice(&run(&["project", "list", "-o", "json"])?)
        .map_err(|_| {
            "bws did not return JSON for the project list; nothing was imported".to_string()
        })?;
    let names: BTreeMap<String, String> = projects
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    Some((
                        p["id"].as_str()?.to_string(),
                        p["name"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let secrets: Value =
        serde_json::from_slice(&run(&["secret", "list", "-o", "json"])?).map_err(|_| {
            "bws did not return JSON for the secret list; nothing was imported".to_string()
        })?;
    let list = secrets.as_array().ok_or("bws returned no secret list")?;
    let mut items: Vec<Item> = Vec::new();
    for s in list {
        let project_id = s["projectId"].as_str().unwrap_or("");
        let project_name = names.get(project_id).cloned();
        if let Some(wanted) = &cfg.project {
            if project_id != wanted && project_name.as_deref() != Some(wanted.as_str()) {
                continue;
            }
        }
        let Some(id) = s["id"].as_str() else { continue };
        let key = s["key"].as_str().unwrap_or(id).to_string();
        let value = s["value"].as_str().unwrap_or("").to_string();
        let mut entry = base_entry(key.clone(), value.clone(), classify(&value));
        if let Some(note) = s["note"].as_str().filter(|n| !n.is_empty()) {
            entry["description"] = json!(note);
        }
        if let Some(p) = &project_name {
            entry["categories"] = json!([p]);
        }
        let source = json!({
            "id": id, "key": key, "value": value, "note": s["note"],
            "revisionDate": s["revisionDate"], "projectId": project_id,
        });
        items.push(source_item(
            id.to_string(),
            key,
            source,
            entry,
            Vec::new(),
            vec!["bitwarden-secrets-manager".into()],
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }
    if items.is_empty() {
        return Err("bws listed no secrets (check the access token and --project)".into());
    }
    Ok(Parsed {
        format: SourceFormat::Bws,
        label: "bws".into(),
        source_bytes: serde_json::to_vec(&json!({ "count": items.len() })).unwrap_or_default(),
        items,
        archived: Vec::new(),
        warnings,
        missing_at_source: Vec::new(),
    })
}

// ── credentials and the command ─────────────────────────────────────────────

/// A credential for a service: the first non-empty environment variable among `env_names`, or the
/// named vault entry's field. It never comes from an argument, which would reach `ps` and shell
/// history, and it is never printed.
pub fn credential(
    access: &crate::access::Access,
    env_names: &[&str],
    entry: Option<&str>,
    field: &str,
) -> crate::error::CliResult<String> {
    use crate::error::CliError;
    if let Some(query) = entry {
        let vault = access.load_vault()?;
        let index = crate::data::find_entry_index(&vault, query)?;
        let value = crate::data::entries(&vault)[index]
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if value.is_empty() {
            return Err(CliError::invalid(format!(
                "entry '{query}' has no {field} to use as the credential"
            )));
        }
        return Ok(value);
    }
    for name in env_names {
        if let Ok(v) = std::env::var(name) {
            if !v.is_empty() {
                return Ok(v);
            }
        }
    }
    Err(CliError::invalid(format!(
        "no credential: set {} or name a vault entry that holds it",
        env_names.join(" or ")
    )))
}

/// Which service to read, with its credential already resolved.
pub enum ServiceSpec {
    OpenBao(OpenBao),
    Infisical(Infisical),
    Bws(Bws),
    #[cfg(feature = "aws")]
    AwsSm(crate::service_aws::AwsOptions),
    #[cfg(feature = "aws")]
    AwsSsm(crate::service_aws::AwsOptions),
}

fn read(spec: &ServiceSpec) -> Result<Parsed, String> {
    match spec {
        ServiceSpec::OpenBao(c) => read_openbao(c),
        ServiceSpec::Infisical(c) => read_infisical(c),
        ServiceSpec::Bws(c) => read_bws(c),
        #[cfg(feature = "aws")]
        ServiceSpec::AwsSm(c) => crate::service_aws::read_secrets_manager(c),
        #[cfg(feature = "aws")]
        ServiceSpec::AwsSsm(c) => crate::service_aws::read_parameter_store(c),
    }
}

/// Reads the service, then previews or applies it exactly like a file import: one transaction, the
/// source identities and archives with it, a named conflict instead of a silent overwrite.
pub fn run(
    access: &crate::access::Access,
    spec: &ServiceSpec,
    options: &crate::password_sources::SourceOpts<'_>,
    apply: bool,
) -> crate::error::CliResult {
    let parsed = read(spec).map_err(crate::error::CliError::invalid)?;
    crate::password_sources::run_parsed(access, parsed, options, apply)
}

/// What the command line gave for a service, before credentials are resolved.
#[derive(Debug, Default, Clone)]
pub struct ServiceArgs {
    pub service: String,
    pub addr: Option<String>,
    pub mount: Option<String>,
    pub path: Option<String>,
    pub namespace: Option<String>,
    pub token_entry: Option<String>,
    pub identity_entry: Option<String>,
    pub project_id: Option<String>,
    pub environment: Option<String>,
    pub secret_path: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub include_previous: bool,
}

fn need(value: &Option<String>, flag: &str, service: &str) -> crate::error::CliResult<String> {
    value
        .clone()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| crate::error::CliError::invalid(format!("{service} needs --{flag}")))
}

/// Turns arguments into a [`ServiceSpec`], resolving the credential from the environment or from a
/// vault entry (never from an argument).
pub fn resolve(
    access: &crate::access::Access,
    args: &ServiceArgs,
) -> crate::error::CliResult<ServiceSpec> {
    let svc = args.service.as_str();
    match svc {
        "openbao" => Ok(ServiceSpec::OpenBao(OpenBao {
            addr: need(&args.addr, "addr", svc)?,
            mount: args.mount.clone().unwrap_or_else(|| "secret".into()),
            path: args.path.clone().unwrap_or_default(),
            namespace: args.namespace.clone(),
            token: credential(
                access,
                &["BAO_TOKEN", "VAULT_TOKEN"],
                args.token_entry.as_deref(),
                "api_key",
            )?,
        })),
        "infisical" => {
            let auth = if let Some(entry) = &args.identity_entry {
                InfisicalAuth::Identity {
                    client_id: credential(access, &[], Some(entry), "api_key")?,
                    client_secret: credential(access, &[], Some(entry), "api_secret")?,
                }
            } else if let Some(entry) = &args.token_entry {
                InfisicalAuth::Token(credential(access, &[], Some(entry), "api_key")?)
            } else if let (Ok(id), Ok(secret)) = (
                std::env::var("INFISICAL_CLIENT_ID"),
                std::env::var("INFISICAL_CLIENT_SECRET"),
            ) {
                InfisicalAuth::Identity {
                    client_id: id,
                    client_secret: secret,
                }
            } else {
                InfisicalAuth::Token(credential(access, &["INFISICAL_TOKEN"], None, "api_key")?)
            };
            Ok(ServiceSpec::Infisical(Infisical {
                addr: args
                    .addr
                    .clone()
                    .unwrap_or_else(|| "https://app.infisical.com".into()),
                project_id: need(&args.project_id, "project-id", svc)?,
                environment: need(&args.environment, "environment", svc)?,
                secret_path: args.secret_path.clone().unwrap_or_else(|| "/".into()),
                auth,
            }))
        }
        "bws" => Ok(ServiceSpec::Bws(Bws {
            program: "bws".into(),
            token: credential(
                access,
                &["BWS_ACCESS_TOKEN"],
                args.token_entry.as_deref(),
                "api_key",
            )?,
            project: args.project_id.clone(),
        })),
        #[cfg(feature = "aws")]
        "aws-sm" | "aws-ssm" => {
            let keys = if let Some(entry) = &args.identity_entry {
                crate::service_aws::AwsKeys {
                    access_key: credential(access, &[], Some(entry), "api_key")?,
                    secret_key: credential(access, &[], Some(entry), "api_secret")?,
                    session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
                }
            } else {
                crate::service_aws::AwsKeys {
                    access_key: credential(access, &["AWS_ACCESS_KEY_ID"], None, "api_key")?,
                    secret_key: credential(access, &["AWS_SECRET_ACCESS_KEY"], None, "api_secret")?,
                    session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
                }
            };
            let opts = crate::service_aws::AwsOptions {
                region: need(&args.region, "region", svc)?,
                endpoint: args.endpoint.clone(),
                keys,
                include_previous: args.include_previous,
                path: args.path.clone().unwrap_or_else(|| "/".into()),
            };
            Ok(if svc == "aws-sm" {
                ServiceSpec::AwsSm(opts)
            } else {
                ServiceSpec::AwsSsm(opts)
            })
        }
        #[cfg(not(feature = "aws"))]
        "aws-sm" | "aws-ssm" => Err(crate::error::CliError::invalid(
            "this build has no AWS support (it is optional and off by default); build with --features aws",
        )),
        other => Err(crate::error::CliError::invalid(format!(
            "unknown service '{other}'"
        ))),
    }
}

#[cfg(all(test, unix))]
pub(crate) mod mock {
    //! A small HTTP/1.1 server for the service tests: every request is answered by a closure that
    //! sees the method, the path with its query string, and the headers.
    use std::io::{Read, Write};
    use std::net::TcpListener;

    pub struct Request {
        pub target: String,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    pub struct Server {
        pub base: String,
    }

    impl Request {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        }
    }

    pub fn start(handler: impl Fn(&Request) -> (u16, String) + Send + Sync + 'static) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let header_end = loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break None;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(i);
                    }
                };
                let Some(end) = header_end else { continue };
                let head = String::from_utf8_lossy(&buf[..end]).into_owned();
                let mut lines = head.lines();
                let first = lines.next().unwrap_or("");
                let mut parts = first.split(' ');
                let method = parts.next().unwrap_or("").to_string();
                let target = parts.next().unwrap_or("").to_string();
                let headers: Vec<(String, String)> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .collect();
                let length: usize = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse().ok())
                    .unwrap_or(0);
                let mut body = buf[end + 4..].to_vec();
                while body.len() < length {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&chunk[..n]);
                }
                let _ = method;
                let req = Request {
                    target,
                    headers,
                    body: String::from_utf8_lossy(&body).into_owned(),
                };
                let (status, text) = handler(&req);
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                    text.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Server { base }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn bao(base: &str) -> OpenBao {
        OpenBao {
            addr: base.into(),
            mount: "secret".into(),
            path: String::new(),
            namespace: None,
            token: "t-test-token".into(),
        }
    }

    /// An OpenBao with `secret/` holding `app/db` (three versions, the middle one destroyed),
    /// `app/web` and a folder the token may not read.
    fn bao_server() -> mock::Server {
        mock::start(|req| {
            let (path, query) = req.target.split_once('?').unwrap_or((&req.target, ""));
            if req.header("x-vault-token") != Some("t-test-token") {
                return (403, r#"{"errors":["permission denied"]}"#.into());
            }
            match path {
                "/v1/sys/health" => (200, r#"{"version":"2.1.0"}"#.into()),
                "/v1/sys/mounts" => (
                    200,
                    r#"{"data":{"secret/":{"type":"kv"},"db/":{"type":"database"},"sys/":{"type":"system"}}}"#.into(),
                ),
                "/v1/secret/metadata/" => (200, r#"{"data":{"keys":["app/","locked/"]}}"#.into()),
                "/v1/secret/metadata/app/" => (200, r#"{"data":{"keys":["db","web"]}}"#.into()),
                "/v1/secret/metadata/locked/" => (403, "{}".into()),
                "/v1/secret/metadata/app/db" => (
                    200,
                    r#"{"data":{"current_version":3,"versions":{
                        "1":{"created_time":"2026-01-01T00:00:00Z","deletion_time":"","destroyed":false},
                        "2":{"created_time":"2026-02-01T00:00:00Z","deletion_time":"","destroyed":true},
                        "3":{"created_time":"2026-03-01T00:00:00Z","deletion_time":"","destroyed":false}}}}"#.into(),
                ),
                "/v1/secret/metadata/app/web" => (
                    200,
                    r#"{"data":{"current_version":1,"versions":{"1":{"created_time":"2026-01-01T00:00:00Z","deletion_time":"","destroyed":false}}}}"#.into(),
                ),
                "/v1/secret/data/app/db" if query.contains("version=1") => {
                    (200, r#"{"data":{"data":{"password":"old-db-pass"}}}"#.into())
                }
                "/v1/secret/data/app/db" if query.contains("version=3") => (
                    200,
                    r#"{"data":{"data":{"password":"new-db-pass","port":5432}}}"#.into(),
                ),
                "/v1/secret/data/app/web" => (200, r#"{"data":{"data":{"token":"web-token-1"}}}"#.into()),
                _ => (404, "{}".into()),
            }
        })
    }

    #[test]
    fn openbao_reads_every_live_version_as_history_and_reports_what_it_could_not_read() {
        let server = bao_server();
        let parsed = read_openbao(&bao(&server.base)).unwrap();
        assert_eq!(parsed.items.len(), 2);
        let db = parsed
            .items
            .iter()
            .find(|i| i.source_id == "secret/app/db")
            .unwrap();
        let vars: Vec<(String, String)> = db.entry["extra_vars"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["key"].as_str().unwrap().into(),
                    v["value"].as_str().unwrap().into(),
                )
            })
            .collect();
        assert!(vars.contains(&("password".into(), "new-db-pass".into())));
        assert!(vars.contains(&("port".into(), "5432".into())));
        let history = db.entry["version_history"].as_array().unwrap();
        assert_eq!(history.len(), 1, "version 2 was destroyed and has no value");
        assert_eq!(history[0]["value"], "old-db-pass");
        assert_eq!(history[0]["saved_at"], "2026-01-01T00:00:00Z");
        assert_eq!(history[0]["field"], "extra_vars/password");
        assert!(db
            .reports
            .iter()
            .any(|r| r.contains("version 2 was destroyed")));
        assert!(parsed
            .warnings
            .iter()
            .any(|w| w.contains("locked") && w.contains("403")));
        assert!(parsed
            .warnings
            .iter()
            .any(|w| w.contains("OpenBao/Vault 2.1.0")));
        assert!(
            parsed.warnings.iter().any(|w| w.contains("db/(database)")),
            "{:?}",
            parsed.warnings
        );
        assert!(parsed.archived.iter().any(|a| a.source_id == "sys/mounts"));
    }

    #[test]
    fn openbao_follows_pagination_past_the_page_size() {
        let server = mock::start(|req| {
            let (path, query) = req.target.split_once('?').unwrap_or((&req.target, ""));
            match path {
                "/v1/secret/metadata/" => {
                    // 100 keys on the first page, 3 on the second.
                    let keys: Vec<String> = if query.contains("after=") {
                        (100..103).map(|i| format!("\"k{i:03}\"")).collect()
                    } else {
                        (0..100).map(|i| format!("\"k{i:03}\"")).collect()
                    };
                    (200, format!(r#"{{"data":{{"keys":[{}]}}}}"#, keys.join(",")))
                }
                p if p.starts_with("/v1/secret/metadata/k") => (
                    200,
                    r#"{"data":{"current_version":1,"versions":{"1":{"created_time":"t","deletion_time":"","destroyed":false}}}}"#.into(),
                ),
                p if p.starts_with("/v1/secret/data/k") => (200, r#"{"data":{"data":{"v":"x"}}}"#.into()),
                _ => (404, "{}".into()),
            }
        });
        let parsed = read_openbao(&bao(&server.base)).unwrap();
        assert_eq!(parsed.items.len(), 103);
    }

    #[test]
    fn a_server_error_mid_listing_fails_the_whole_read() {
        let server = mock::start(|req| {
            if req.target.starts_with("/v1/secret/metadata/?") {
                (200, r#"{"data":{"keys":["a","b"]}}"#.into())
            } else if req.target.starts_with("/v1/secret/metadata/a") {
                (200, r#"{"data":{"current_version":1,"versions":{"1":{"created_time":"t","deletion_time":"","destroyed":false}}}}"#.into())
            } else if req.target.starts_with("/v1/secret/data/a") {
                (200, r#"{"data":{"data":{"v":"x"}}}"#.into())
            } else {
                (500, "{}".into())
            }
        });
        let err = read_openbao(&bao(&server.base)).unwrap_err();
        assert!(err.contains("HTTP 500"), "{err}");
    }

    #[test]
    fn a_wrong_token_is_a_clear_error_and_the_token_is_never_echoed() {
        let server = bao_server();
        let mut cfg = bao(&server.base);
        cfg.token = "wrong-token-do-not-print".into();
        let err = read_openbao(&cfg).unwrap_err();
        assert!(!err.contains("wrong-token-do-not-print"), "{err}");
    }

    fn infisical_server() -> mock::Server {
        mock::start(|req| {
            let (path, query) = req.target.split_once('?').unwrap_or((&req.target, ""));
            match path {
                "/api/v1/auth/universal-auth/login" => {
                    if req.body.contains("\"clientSecret\":\"cs-test\"") {
                        (200, r#"{"accessToken":"at-test"}"#.into())
                    } else {
                        (401, "{}".into())
                    }
                }
                "/api/v4/secrets" => {
                    if req.header("authorization") != Some("Bearer at-test") {
                        return (401, "{}".into());
                    }
                    if query.contains("expandSecretReferences=true") {
                        (200, r#"{"secrets":[
                            {"secretKey":"DB_HOST","secretValue":"db.internal","secretPath":"/","version":2,"type":"shared","secretComment":"primary"},
                            {"secretKey":"DB_URL","secretValue":"postgres://db.internal/app","secretPath":"/","version":1,"type":"shared"},
                            {"secretKey":"KEY","secretValue":"abc","secretPath":"/api","version":1,"type":"shared"}],
                            "imports":[{"secretPath":"/shared"}]}"#.into())
                    } else {
                        (200, r#"{"secrets":[
                            {"secretKey":"DB_HOST","secretValue":"db.internal","secretPath":"/"},
                            {"secretKey":"DB_URL","secretValue":"postgres://${DB_HOST}/app","secretPath":"/"},
                            {"secretKey":"KEY","secretValue":"abc","secretPath":"/api"}]}"#.into())
                    }
                }
                _ => (404, "{}".into()),
            }
        })
    }

    #[test]
    fn infisical_logs_in_expands_references_and_records_the_graph_without_dollar_braces() {
        let server = infisical_server();
        let parsed = read_infisical(&Infisical {
            addr: server.base.clone(),
            project_id: "proj1".into(),
            environment: "prod".into(),
            secret_path: "/".into(),
            auth: InfisicalAuth::Identity {
                client_id: "ci-test".into(),
                client_secret: "cs-test".into(),
            },
        })
        .unwrap();
        assert_eq!(parsed.items.len(), 2, "one entry per folder path");
        let root = parsed
            .items
            .iter()
            .find(|i| i.name == "proj1/prod")
            .unwrap();
        let vars: Vec<(String, String)> = root.entry["extra_vars"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["key"].as_str().unwrap().into(),
                    v["value"].as_str().unwrap().into(),
                )
            })
            .collect();
        assert!(vars.contains(&("DB_URL".into(), "postgres://db.internal/app".into())));
        assert!(root
            .reports
            .iter()
            .any(|r| r.contains("DB_URL refers to DB_HOST")));
        let graph = String::from_utf8_lossy(&parsed.source_bytes).into_owned();
        assert!(graph.contains("DB_URL refers to DB_HOST"), "{graph}");
        assert!(
            !format!("{:?}", root.entry).contains("${"),
            "Infisical reference syntax must not be stored raw"
        );
        assert!(root.entry["description"]
            .as_str()
            .unwrap()
            .contains("primary"));
        assert!(parsed.warnings.iter().any(|w| w.contains("secret import")));
        assert!(parsed.items.iter().any(|i| i.name == "proj1/prod/api"));
    }

    #[test]
    fn infisical_rejects_a_bad_identity_and_an_empty_result() {
        let server = infisical_server();
        let bad = read_infisical(&Infisical {
            addr: server.base.clone(),
            project_id: "p".into(),
            environment: "prod".into(),
            secret_path: "/".into(),
            auth: InfisicalAuth::Identity {
                client_id: "ci".into(),
                client_secret: "wrong".into(),
            },
        });
        assert!(bad.is_err());
        let empty = mock::start(|_| (200, r#"{"secrets":[]}"#.into()));
        let err = read_infisical(&Infisical {
            addr: empty.base.clone(),
            project_id: "p".into(),
            environment: "e".into(),
            secret_path: "/".into(),
            auth: InfisicalAuth::Token("t".into()),
        })
        .unwrap_err();
        assert!(err.contains("no secrets"), "{err}");
    }

    #[test]
    fn reference_extraction_handles_the_three_syntaxes_and_unterminated_text() {
        assert_eq!(
            infisical_refs("a ${X} b ${prod.KEY} c ${prod.folder.KEY} d ${X}"),
            vec!["X", "prod.KEY", "prod.folder.KEY"]
        );
        assert!(infisical_refs("${unterminated").is_empty());
        assert!(infisical_refs("no refs").is_empty());
    }

    fn stub(dir: &std::path::Path, body: &str) -> String {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("bws");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn bws_reads_projects_and_secrets_with_the_token_only_in_the_childs_environment() {
        let dir = std::env::temp_dir().join(format!("unv-bws-{}", vault_core::new_uuid()));
        let program = stub(
            &dir,
            r#"if [ "$1" = "--version" ]; then echo "bws 1.0.0"; exit 0; fi
[ "$BWS_ACCESS_TOKEN" = "tok-test" ] || { echo "no token" >&2; exit 1; }
case "$1 $2" in
  "project list") printf '[{"id":"p1","name":"Infra"}]';;
  "secret list") printf '[{"id":"s1","projectId":"p1","key":"DB_PASSWORD","value":"hunter2-from-bws","note":"prod db","revisionDate":"2026-01-01"},{"id":"s2","projectId":"other","key":"SKIP","value":"x"}]';;
esac"#,
        );
        let parsed = read_bws(&Bws {
            program: program.clone(),
            token: "tok-test".into(),
            project: Some("Infra".into()),
        })
        .unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].entry["api_key"], "hunter2-from-bws");
        assert_eq!(parsed.items[0].entry["categories"], json!(["Infra"]));
        assert_eq!(parsed.items[0].entry["description"], "prod db");
        assert!(parsed.warnings.iter().any(|w| w.contains("bws 1.0.0")));
        let wrong = read_bws(&Bws {
            program,
            token: "wrong".into(),
            project: None,
        })
        .unwrap_err();
        assert!(wrong.contains("bws") && !wrong.contains("wrong"), "{wrong}");
        let missing = read_bws(&Bws {
            program: "unv-no-such-bws".into(),
            token: "t".into(),
            project: None,
        })
        .unwrap_err();
        assert!(missing.contains("not installed"), "{missing}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
