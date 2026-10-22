// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Import from Git-based secret stores: SOPS files, a `pass` store and a gopass store.
//!
//! None of these reimplement a key system. The user's own tool does the decryption, through
//! [`crate::tool_exec`] (argv only, a cleared environment, a deadline, an output cap, scrubbed
//! stderr), so the keys stay with `sops`, `gpg` and `gopass` and the passphrase prompt stays with
//! the user's agent. Nothing here writes to the source: the store and every file in it are only read.
//!
//! Each reader returns the same [`Parsed`] shape as the password-manager sources, so planning,
//! reconciliation, the report and the encrypted source archive are one code path.

use crate::password_sources::{
    base_entry, classify, extra, hash, source_item, Archive, ArchivedItem, Item, Parsed,
    SourceFormat,
};
use crate::tool_exec::{self, ToolError, ToolSpec};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Names of the programs to run. The tests point these at stub scripts.
#[derive(Debug, Clone)]
pub struct Tools {
    pub sops: String,
    pub gpg: String,
    pub gopass: String,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            sops: "sops".into(),
            gpg: "gpg".into(),
            gopass: "gopass".into(),
        }
    }
}

/// Environment names a tool may need to find its keys. Nothing else is inherited.
const SOPS_ENV: &[&str] = &[
    "SOPS_AGE_KEY_FILE",
    "SOPS_AGE_KEY_CMD",
    "SOPS_AGE_RECIPIENTS",
    "GNUPGHOME",
    "GPG_TTY",
    "AWS_PROFILE",
    "AWS_REGION",
    "AWS_SHARED_CREDENTIALS_FILE",
    "AZURE_CONFIG_DIR",
    "GOOGLE_APPLICATION_CREDENTIALS",
];
const GPG_ENV: &[&str] = &[
    "GNUPGHOME",
    "GPG_TTY",
    "GPG_AGENT_INFO",
    "DISPLAY",
    "WAYLAND_DISPLAY",
];
const GOPASS_ENV: &[&str] = &[
    "GNUPGHOME",
    "GPG_TTY",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "GOPASS_CONFIG",
    "GOPASS_HOMEDIR",
    "PASSWORD_STORE_DIR",
    "XDG_CONFIG_HOME",
];

/// A tool waiting for a passphrase is the user's to answer; give them time, but not forever.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_PLAINTEXT: usize = 8 * 1024 * 1024;
const MAX_FILES: usize = 20_000;

fn tool_error(program: &str, e: ToolError) -> String {
    format!("{program}: {e}")
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn walk_files(root: &Path, keep: impl Fn(&Path) -> bool) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || e.file_name() != ".git")
    {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", root.display()))?;
        if entry.file_type().is_file() && keep(entry.path()) {
            out.push(entry.into_path());
            if out.len() > MAX_FILES {
                return Err(format!(
                    "more than {MAX_FILES} files under {}",
                    root.display()
                ));
            }
        }
    }
    Ok(out)
}

pub(crate) fn parse(format: SourceFormat, path: &Path, tools: &Tools) -> Result<Parsed, String> {
    match format {
        SourceFormat::Sops => parse_sops(path, tools),
        SourceFormat::Pass => parse_pass(path, tools),
        SourceFormat::Gopass => parse_gopass(path, tools),
        _ => Err("not a Git-store source".into()),
    }
}

// ── shared pieces ───────────────────────────────────────────────────────────

/// Walks a decrypted JSON tree into `path.to.leaf = value` pairs. Arrays use the index as the key
/// segment. Containers with no leaves contribute nothing; `null` is skipped (it carries no value).
fn flatten(prefix: &str, value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, v, out);
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                let key = if prefix.is_empty() {
                    i.to_string()
                } else {
                    format!("{prefix}.{i}")
                };
                flatten(&key, v, out);
            }
        }
        Value::String(s) => out.push((prefix.to_string(), s.clone())),
        Value::Number(n) => out.push((prefix.to_string(), n.to_string())),
        Value::Bool(b) => out.push((prefix.to_string(), b.to_string())),
        Value::Null => {}
    }
}

fn sorted_manifest(files: &[(String, String)]) -> Vec<u8> {
    let items: Vec<Value> = files
        .iter()
        .map(|(path, sha)| json!({ "path": path, "sha256": sha }))
        .collect();
    serde_json::to_vec(&json!({ "files": items })).unwrap_or_default()
}

// ── SOPS ────────────────────────────────────────────────────────────────────

/// Does this text look like a SOPS-encrypted document? The metadata block is the tell: a `sops`
/// key in YAML and JSON, `sops_*` lines in dotenv and INI files.
fn looks_like_sops(text: &str) -> bool {
    text.contains("\"sops\"")
        || text.contains("\nsops:")
        || text.starts_with("sops:")
        || text.contains("sops_mac=")
        || text.contains("sops_version=")
        || text.contains("[sops]")
}

/// A SOPS binary file is a JSON wrapper `{"data": "ENC[...]", "sops": {...}}` and nothing else.
fn is_sops_binary(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| {
            let map = v.as_object()?;
            (map.len() == 2 && map.contains_key("data") && map.contains_key("sops")).then_some(())
        })
        .is_some()
}

fn sops_decrypt(tools: &Tools, path: &Path, binary: bool) -> Result<Vec<u8>, String> {
    let output_type = if binary { "binary" } else { "json" };
    let spec = ToolSpec::new(&tools.sops, &["decrypt", "--output-type", output_type])
        .arg(path.to_string_lossy())
        .pass_env(SOPS_ENV)
        .timeout(PROMPT_TIMEOUT)
        .max_stdout(MAX_PLAINTEXT);
    tool_exec::run(&spec)
        .map(|o| o.stdout)
        .map_err(|e| tool_error("sops", e))
}

fn parse_sops(path: &Path, tools: &Tools) -> Result<Parsed, String> {
    let meta =
        std::fs::metadata(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let (root, files) = if meta.is_dir() {
        let files = walk_files(path, |p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("yaml" | "yml" | "json" | "env" | "ini")
            ) && std::fs::read_to_string(p)
                .map(|t| looks_like_sops(&t))
                .unwrap_or(false)
        })?;
        (path.to_path_buf(), files)
    } else {
        (
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            vec![path.to_path_buf()],
        )
    };
    if files.is_empty() {
        return Err(format!(
            "no SOPS-encrypted files found under {} (looked for yaml, yml, json, env and ini with a sops block)",
            path.display()
        ));
    }
    let mut warnings = Vec::new();
    if let Some(v) = tool_exec::version(&tools.sops, &["--version"]) {
        warnings.push(format!("read with {v}"));
    }
    let mut items = Vec::new();
    let mut archived = Vec::new();
    let mut manifest = Vec::new();
    for file in &files {
        let relpath = rel(&root, file);
        let ciphertext = std::fs::read(file).map_err(|e| format!("cannot read {relpath}: {e}"))?;
        manifest.push((relpath.clone(), hash(&ciphertext)));
        let text = String::from_utf8_lossy(&ciphertext).into_owned();
        // The encrypted file is kept as it is: its `sops` block (key groups and MAC) is
        // metadata about who can decrypt, retained for provenance and never made operational.
        let metadata_archive = Archive {
            source_id: Some(relpath.clone()),
            reason: "SOPS metadata, key groups and MAC retained as non-operational provenance"
                .into(),
            media_type: "application/octet-stream".into(),
            bytes: ciphertext,
        };
        if is_sops_binary(&text) {
            let plain = sops_decrypt(tools, file, true).map_err(|e| format!("{relpath}: {e}"))?;
            archived.push(ArchivedItem {
                source_id: relpath.clone(),
                source_hash: hash(&plain),
                name: relpath,
                archive: Archive {
                    source_id: None,
                    reason: "decrypted SOPS binary file".into(),
                    media_type: "application/octet-stream".into(),
                    bytes: plain,
                },
            });
            continue;
        }
        let plain = sops_decrypt(tools, file, false).map_err(|e| format!("{relpath}: {e}"))?;
        let tree: Value = serde_json::from_slice(&plain)
            .map_err(|_| format!("{relpath}: sops did not return JSON; nothing was imported"))?;
        let Some(map) = tree.as_object() else {
            return Err(format!(
                "{relpath}: the decrypted document is not a mapping; nothing was imported"
            ));
        };
        let mut leaves = Vec::new();
        flatten("", &Value::Object(map.clone()), &mut leaves);
        let mut reports = Vec::new();
        if leaves.is_empty() {
            reports.push("no values in the decrypted document".to_string());
        }
        let extra_vars: Vec<Value> = leaves
            .iter()
            .map(|(key, value)| extra(key.clone(), value.clone(), false))
            .collect();
        let mut entry = base_entry(relpath.clone(), String::new(), "env_var");
        if let Some(dir) = Path::new(&relpath).parent().and_then(|p| p.to_str()) {
            if !dir.is_empty() {
                entry["categories"] = json!([dir]);
            }
        }
        items.push(source_item(
            relpath.clone(),
            relpath,
            tree,
            entry,
            Vec::new(),
            vec!["sops".into()],
            extra_vars,
            None,
            Vec::new(),
            vec![metadata_archive],
            reports,
        ));
    }
    if meta.is_dir() {
        let config = root.join(".sops.yaml");
        if let Ok(bytes) = std::fs::read(&config) {
            archived.push(ArchivedItem {
                source_id: ".sops.yaml".into(),
                source_hash: hash(&bytes),
                name: ".sops.yaml".into(),
                archive: Archive {
                    source_id: None,
                    reason: "SOPS creation rules retained as non-operational provenance".into(),
                    media_type: "application/yaml".into(),
                    bytes,
                },
            });
        }
    }
    Ok(Parsed {
        format: SourceFormat::Sops,
        label: path.to_string_lossy().into_owned(),
        source_bytes: sorted_manifest(&manifest),
        items,
        archived,
        warnings,
        missing_at_source: Vec::new(),
    })
}

// ── pass and gopass ─────────────────────────────────────────────────────────

/// The shape `pass` and gopass share: the first line is the password, the rest is free text. Lines
/// of the form `user: x`, `username: x`, `login: x`, `url: x` and `email: x` fill the matching
/// fields; an `otpauth://` line becomes the authenticator seed. The whole body is kept as the
/// entry's description, so nothing the user wrote is lost to the parsing.
fn pass_like_item(name: &str, source_id: &str, body: &str) -> Item {
    let mut lines = body.lines();
    let password = lines
        .next()
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string();
    let rest: Vec<&str> = lines.collect();
    let mut username = None;
    let mut url = None;
    let mut email = None;
    let mut totp = None;
    for line in &rest {
        let trimmed = line.trim();
        if trimmed.starts_with("otpauth://") && totp.is_none() {
            totp = Some(trimmed.to_string());
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match key.trim().to_ascii_lowercase().as_str() {
                "user" | "username" | "login" if username.is_none() => {
                    username = Some(value.to_string())
                }
                "url" | "website" if url.is_none() => url = Some(value.to_string()),
                "email" | "e-mail" if email.is_none() => email = Some(value.to_string()),
                _ => {}
            }
        }
    }
    let secret_type = classify(&password);
    let mut entry = base_entry(name.to_string(), password.clone(), secret_type);
    if let Some(u) = username {
        entry["username"] = json!(u);
    }
    if let Some(u) = url {
        entry["api_url"] = json!(u);
    }
    if let Some(e) = email {
        entry["email"] = json!(e);
    }
    let notes = rest.join("\n");
    if !notes.trim().is_empty() {
        entry["description"] = json!(notes);
    }
    if let Some((dir, _)) = name.rsplit_once('/') {
        entry["categories"] = json!([dir]);
    }
    let source = json!({ "name": name, "body": body });
    source_item(
        source_id.to_string(),
        name.to_string(),
        source,
        entry,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        totp,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}

fn parse_pass(root: &Path, tools: &Tools) -> Result<Parsed, String> {
    let meta =
        std::fs::metadata(root).map_err(|e| format!("cannot read {}: {e}", root.display()))?;
    if !meta.is_dir() {
        return Err(format!(
            "{} is not a directory; give the password store folder (it holds .gpg-id)",
            root.display()
        ));
    }
    let files = walk_files(root, |p| {
        p.extension().and_then(|e| e.to_str()) == Some("gpg")
    })?;
    if files.is_empty() {
        return Err(format!("no .gpg files under {}", root.display()));
    }
    let mut warnings = Vec::new();
    if let Ok(ids) = std::fs::read_to_string(root.join(".gpg-id")) {
        let ids: Vec<&str> = ids
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if !ids.is_empty() {
            warnings.push(format!("store recipients (.gpg-id): {}", ids.join(", ")));
        }
    }
    if let Some(v) = tool_exec::version(&tools.gpg, &["--version"]) {
        warnings.push(format!("read with {v}"));
    }
    let mut items = Vec::new();
    let mut archived = Vec::new();
    let mut manifest = Vec::new();
    for file in &files {
        let relpath = rel(root, file);
        let name = relpath.strip_suffix(".gpg").unwrap_or(&relpath).to_string();
        let ciphertext = std::fs::read(file).map_err(|e| format!("cannot read {relpath}: {e}"))?;
        manifest.push((relpath.clone(), hash(&ciphertext)));
        let spec = ToolSpec::new(&tools.gpg, &["--decrypt", "--batch", "--quiet"])
            .arg(file.to_string_lossy())
            .pass_env(GPG_ENV)
            .timeout(PROMPT_TIMEOUT)
            .max_stdout(MAX_PLAINTEXT);
        let plain = tool_exec::run(&spec)
            .map_err(|e| format!("{relpath}: {}", tool_error("gpg", e)))?
            .stdout;
        match String::from_utf8(plain) {
            Ok(body) => items.push(pass_like_item(&name, &relpath, &body)),
            Err(e) => archived.push(ArchivedItem {
                source_id: relpath.clone(),
                source_hash: hash(e.as_bytes()),
                name,
                archive: Archive {
                    source_id: None,
                    reason: "decrypted binary secret".into(),
                    media_type: "application/octet-stream".into(),
                    bytes: e.into_bytes(),
                },
            }),
        }
    }
    Ok(Parsed {
        format: SourceFormat::Pass,
        label: root.to_string_lossy().into_owned(),
        source_bytes: sorted_manifest(&manifest),
        items,
        archived,
        warnings,
        missing_at_source: Vec::new(),
    })
}

fn gopass_run(tools: &Tools, args: &[&str]) -> Result<Vec<u8>, String> {
    let spec = ToolSpec::new(&tools.gopass, args)
        .pass_env(GOPASS_ENV)
        .timeout(PROMPT_TIMEOUT)
        .max_stdout(MAX_PLAINTEXT);
    tool_exec::run(&spec)
        .map(|o| o.stdout)
        .map_err(|e| tool_error("gopass", e))
}

fn parse_gopass(path: &Path, tools: &Tools) -> Result<Parsed, String> {
    let listing = gopass_run(tools, &["ls", "--flat"])?;
    let names: Vec<String> = String::from_utf8_lossy(&listing)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if names.is_empty() {
        return Err("gopass listed no secrets".into());
    }
    let mut warnings = Vec::new();
    if let Some(v) = tool_exec::version(&tools.gopass, &["--version"]) {
        warnings.push(format!("read with {v}"));
    }
    if let Ok(mounts) = gopass_run(tools, &["mounts"]) {
        let text = String::from_utf8_lossy(&mounts);
        let line = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>();
        if !line.is_empty() {
            warnings.push(format!("mounts: {}", line.join("; ")));
        }
    }
    let mut items = Vec::new();
    let mut archived = Vec::new();
    let mut manifest = Vec::new();
    for name in &names {
        let plain =
            gopass_run(tools, &["show", "--unsafe", name]).map_err(|e| format!("{name}: {e}"))?;
        match String::from_utf8(plain) {
            Ok(body) => {
                manifest.push((name.clone(), hash(body.as_bytes())));
                items.push(pass_like_item(name, name, &body));
            }
            Err(_) => {
                // Not text: ask for the raw bytes.
                let bytes =
                    gopass_run(tools, &["cat", name]).map_err(|e| format!("{name}: {e}"))?;
                manifest.push((name.clone(), hash(&bytes)));
                archived.push(ArchivedItem {
                    source_id: name.clone(),
                    source_hash: hash(&bytes),
                    name: name.clone(),
                    archive: Archive {
                        source_id: None,
                        reason: "decrypted binary secret".into(),
                        media_type: "application/octet-stream".into(),
                        bytes,
                    },
                });
            }
        }
    }
    Ok(Parsed {
        format: SourceFormat::Gopass,
        label: if path.as_os_str().is_empty() {
            "gopass".into()
        } else {
            path.to_string_lossy().into_owned()
        },
        source_bytes: sorted_manifest(&manifest),
        items,
        archived,
        warnings,
        missing_at_source: Vec::new(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::password_sources::item_source_hash;
    use std::os::unix::fs::PermissionsExt;

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!("unv-git-src-{}", vault_core::new_uuid()));
        vault_core::create_private_dir(&d).unwrap();
        d
    }

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    fn stub(dir: &Path, name: &str, body: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn tools(sops: &str, gpg: &str, gopass: &str) -> Tools {
        Tools {
            sops: sops.into(),
            gpg: gpg.into(),
            gopass: gopass.into(),
        }
    }

    const SOPS_YAML: &str = "db:\n  password: ENC[AES256_GCM,data:x]\nsops:\n  version: 3.9.0\n";

    #[test]
    fn sops_files_become_one_entry_each_with_dotted_leaf_names_and_archive_the_metadata() {
        let dir = scratch();
        write(&dir.join("prod/app.yaml"), SOPS_YAML);
        write(&dir.join("notes.yaml"), "plain: not encrypted\n");
        write(
            &dir.join(".sops.yaml"),
            "creation_rules:\n  - age: age1xxx\n",
        );
        let sops = stub(
            &dir,
            "fake-sops",
            r#"if [ "$1" = "--version" ]; then echo "sops 3.9.0 (stub)"; exit 0; fi
printf '{"db":{"password":"hunter2","port":5432},"list":["a","b"],"debug":true,"gone":null}'"#,
        );
        let parsed = parse_sops(&dir, &tools(&sops, "x", "x")).unwrap();
        assert_eq!(parsed.items.len(), 1);
        let item = &parsed.items[0];
        assert_eq!(item.source_id, "prod/app.yaml");
        let vars: Vec<(String, String)> = item.entry["extra_vars"]
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
        assert!(vars.contains(&("db.password".into(), "hunter2".into())));
        assert!(vars.contains(&("db.port".into(), "5432".into())));
        assert!(vars.contains(&("list.0".into(), "a".into())));
        assert!(vars.contains(&("debug".into(), "true".into())));
        assert!(!vars.iter().any(|(k, _)| k == "gone"));
        assert_eq!(item.entry["categories"], json!(["prod"]));
        // The ciphertext is archived as non-operational provenance; plaintext is not.
        assert_eq!(item.archives.len(), 1);
        assert!(item.archives[0].reason.contains("non-operational"));
        assert!(String::from_utf8_lossy(&item.archives[0].bytes).contains("ENC[AES256_GCM"));
        assert!(parsed.archived.iter().any(|a| a.source_id == ".sops.yaml"));
        assert!(parsed.warnings.iter().any(|w| w.contains("sops 3.9.0")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sops_binary_files_are_archived_as_blobs_and_garbage_output_imports_nothing() {
        let dir = scratch();
        write(
            &dir.join("blob.json"),
            r#"{"data":"ENC[AES256_GCM,data:y]","sops":{"version":"3.9.0"}}"#,
        );
        let ok = stub(&dir, "sops-binary", r#"printf 'raw-bytes'"#);
        let parsed = parse_sops(&dir.join("blob.json"), &tools(&ok, "x", "x")).unwrap();
        assert!(parsed.items.is_empty());
        assert_eq!(parsed.archived.len(), 1);
        assert_eq!(parsed.archived[0].archive.bytes, b"raw-bytes");

        write(&dir.join("bad.yaml"), SOPS_YAML);
        let bad = stub(&dir, "sops-garbage", r#"printf 'not json at all'"#);
        let err = parse_sops(&dir.join("bad.yaml"), &tools(&bad, "x", "x")).unwrap_err();
        assert!(err.contains("did not return JSON"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_tool_names_the_program_and_a_folder_without_sops_files_is_refused() {
        let dir = scratch();
        write(&dir.join("a.yaml"), SOPS_YAML);
        let err = parse_sops(&dir, &tools("unv-no-such-sops", "x", "x")).unwrap_err();
        assert!(err.contains("not installed"));
        let empty = scratch();
        write(&empty.join("plain.yaml"), "a: b\n");
        assert!(parse_sops(&empty, &tools("x", "x", "x"))
            .unwrap_err()
            .contains("no SOPS-encrypted files"));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn a_tool_that_hangs_leaves_nothing_half_imported() {
        // The deadline is exercised in tool_exec; here the error must name the file and the tool.
        let dir = scratch();
        write(&dir.join("a.yaml"), SOPS_YAML);
        let failing = stub(
            &dir,
            "sops-fail",
            "echo 'no key could decrypt' >&2; exit 128",
        );
        let err = parse_sops(&dir.join("a.yaml"), &tools(&failing, "x", "x")).unwrap_err();
        assert!(err.contains("a.yaml") && err.contains("exit 128"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pass_reads_the_first_line_as_the_password_and_keeps_the_whole_body() {
        let dir = scratch();
        let store = dir.join("store");
        write(&store.join(".gpg-id"), "me@example.test\n");
        write(&store.join("email/gmail.gpg"), "ciphertext-1");
        write(&store.join("blob.gpg"), "ciphertext-2");
        write(&store.join(".git/ignored.gpg"), "never");
        let gpg = stub(
            &dir,
            "fake-gpg",
            r#"if [ "$1" = "--version" ]; then echo "gpg (GnuPG) 2.4.5"; exit 0; fi
for last; do :; done
case "$last" in
  *gmail.gpg) printf 'hunter2\nuser: me\nurl: https://mail.example.test\nemail: me@example.test\notpauth://totp/x?secret=JBSWY3DPEHPK3PXP\nfree text line\n';;
  *blob.gpg) printf '\377\376binary';;
esac"#,
        );
        let parsed = parse_pass(&store, &tools("x", &gpg, "x")).unwrap();
        assert_eq!(parsed.items.len(), 1);
        let item = &parsed.items[0];
        assert_eq!(item.name, "email/gmail");
        assert_eq!(item.entry["api_key"], "hunter2");
        assert_eq!(item.entry["username"], "me");
        assert_eq!(item.entry["api_url"], "https://mail.example.test");
        assert_eq!(item.entry["categories"], json!(["email"]));
        assert!(item.entry["description"]
            .as_str()
            .unwrap()
            .contains("free text line"));
        assert!(item.entry.get("totp_secret").is_some());
        assert_eq!(
            parsed.archived.len(),
            1,
            "the binary secret is archived, not parsed"
        );
        assert!(parsed
            .warnings
            .iter()
            .any(|w| w.contains("me@example.test")));
        assert!(parsed.warnings.iter().any(|w| w.contains("GnuPG")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pass_store_is_not_touched_by_an_import() {
        let dir = scratch();
        let store = dir.join("store");
        write(&store.join("a.gpg"), "ct");
        let gpg = stub(&dir, "gpg", "printf 'pw\\n'");
        let before = hash(&std::fs::read(store.join("a.gpg")).unwrap());
        let parsed = parse_pass(&store, &tools("x", &gpg, "x")).unwrap();
        assert_eq!(hash(&std::fs::read(store.join("a.gpg")).unwrap()), before);
        // The manifest that stands in for the source records each file's hash.
        let manifest: Value = serde_json::from_slice(&parsed.source_bytes).unwrap();
        assert_eq!(manifest["files"][0]["sha256"], before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gopass_lists_names_reads_each_and_records_the_mounts() {
        let dir = scratch();
        let gopass = stub(
            &dir,
            "fake-gopass",
            r#"case "$1" in
  --version) echo "gopass 1.15.15";;
  ls) printf 'web/github\nnotes/raw\n';;
  mounts) printf 'gopass (/home/u/.store)\n';;
  show) case "$3" in web/github) printf 'ghp_exampleexample\nuser: octo\n';; *) printf '\377\376';; esac;;
  cat) printf 'binary-bytes';;
esac"#,
        );
        let parsed = parse_gopass(Path::new(""), &tools("x", "x", &gopass)).unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].entry["username"], "octo");
        assert_eq!(parsed.items[0].entry["secretType"], "api_key");
        assert_eq!(parsed.archived.len(), 1);
        assert_eq!(parsed.archived[0].archive.bytes, b"binary-bytes");
        assert!(parsed.warnings.iter().any(|w| w.starts_with("mounts:")));
        assert_eq!(parsed.label, "gopass");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn flatten_names_nested_and_array_leaves() {
        let v = json!({"a": {"b": [1, {"c": "x"}]}, "d": false});
        let mut out = Vec::new();
        flatten("", &v, &mut out);
        let mut names: Vec<String> = out.into_iter().map(|(k, _)| k).collect();
        names.sort();
        assert_eq!(names, vec!["a.b.0", "a.b.1.c", "d"]);
        assert_eq!(item_source_hash(&json!({}), &[]).len(), 64);
    }
}
