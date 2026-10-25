// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Phase 35 — the config time machine's one dispatcher (ADR-0141).
//!
//! The CLI (local), `unv-server` (`POST /api/history`) and the desktop app (the
//! `history_call` command) all reach the history through [`call`], so the three
//! cannot disagree about what a list, a diff or a prune does. Each caller only
//! supplies a connection and the JSON arguments.
//!
//! Rendering lives here rather than in `vault-core` because it needs the
//! exporters. Every snapshot is rendered twice, once as it would be deployed and
//! once with each resolved secret replaced by its fingerprint; see
//! `vault_core::config_history` for why both are stored.

use crate::chunks::{default_format_for, render_project_as};
use crate::data::projects;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use vault_core::config_history as ch;
use vault_core::SqlConnection as Connection;

const RENDER_REVISIONS_META: &str = "history_render_revisions_v1";

/// The exact row revisions that influenced each rendered project. This lives in
/// SQLCipher's `vault_meta`, not beside a user's project files, and contains no
/// secret material: only opaque row hashes and stable ids.
#[derive(Debug, Default, Serialize, Deserialize)]
struct RenderRevisionState {
    projects: BTreeMap<String, ProjectRevision>,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct ProjectRevision {
    project_rev: String,
    dependencies: BTreeMap<String, String>,
}

/// `(project id, project name, exporter)` for every config a project renders.
fn streams(vault: &Value) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for p in projects(vault) {
        let id = p.get("id").and_then(Value::as_str).unwrap_or("");
        let name = p.get("name").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let ptype = p
            .get("project_type")
            .and_then(Value::as_str)
            .unwrap_or("generic");
        let fmt = default_format_for(ptype);
        out.push((id.into(), name.into(), fmt.into()));
        // Compose substitutes ${VAR} from the `.env` beside it, so the pair is
        // one deliverable and the `.env` is a stream of its own.
        if fmt == "compose" {
            out.push((id.into(), name.into(), "compose-env".into()));
        }
    }
    out
}

/// Renders one project with one exporter and records it if it changed. The
/// hub calls this with the exact target a node is about to be sent, so a file
/// that goes to a host is always in the history, whether or not a save came first.
pub fn snapshot_stream(
    conn: &Connection,
    vault: &Value,
    project: &str,
    exporter: &str,
    cause: &str,
    actor: Option<&str>,
) -> Result<Option<i64>, String> {
    let pi = crate::data::find_project_index(vault, project).map_err(|e| e.to_string())?;
    let p = &projects(vault)[pi];
    let id = p.get("id").and_then(Value::as_str).unwrap_or("");
    let name = p.get("name").and_then(Value::as_str).unwrap_or("");
    let real = render_project_as(vault, id, exporter, false).map_err(|e| e.to_string())?;
    let masked = render_project_as(vault, id, exporter, true).map_err(|e| e.to_string())?;
    let exposed = serde_json::to_string(&crate::exposure::Matcher::from_vault(vault).find(&real))
        .unwrap_or_else(|_| "[]".into());
    let stream = ch::Stream {
        project: id,
        project_name: name,
        exporter,
    };
    ch::record(conn, &stream, &real, &masked, &exposed, cause, actor)
}

#[derive(Debug, Default, PartialEq)]
pub struct SnapshotReport {
    /// Streams whose renderer was invoked. This makes the incremental work
    /// observable in tests without treating "unchanged" as "unrendered".
    pub considered: usize,
    pub recorded: usize,
    pub unchanged: usize,
    /// Streams that render to nothing or fail to render (a generic project with
    /// no `.env` chunk, say). Not an error: most projects have one real config.
    pub skipped: usize,
    pub errors: Vec<String>,
}

fn snapshot_matching(
    conn: &Connection,
    vault: &Value,
    include: impl Fn(&str, &str) -> bool,
    cause: &str,
    actor: Option<&str>,
) -> SnapshotReport {
    let mut rep = SnapshotReport::default();
    if !ch::policy(conn).enabled {
        return rep;
    }
    // Built on first use: most saves change no config and never need it.
    let matcher = std::cell::OnceCell::new();
    for (id, name, exporter) in streams(vault) {
        if !include(&id, &name) {
            continue;
        }
        rep.considered += 1;
        let real = render_project_as(vault, &id, &exporter, false);
        let masked = render_project_as(vault, &id, &exporter, true);
        let (Ok(real), Ok(masked)) = (real, masked) else {
            rep.skipped += 1;
            continue;
        };
        if real.trim().is_empty() {
            rep.skipped += 1;
            continue;
        }
        let stream = ch::Stream {
            project: &id,
            project_name: &name,
            exporter: &exporter,
        };
        let exposed = serde_json::to_string(
            &matcher
                .get_or_init(|| crate::exposure::Matcher::from_vault(vault))
                .find(&real),
        )
        .unwrap_or_else(|_| "[]".into());
        match ch::record(conn, &stream, &real, &masked, &exposed, cause, actor) {
            Ok(Some(_)) => rep.recorded += 1,
            Ok(None) => rep.unchanged += 1,
            Err(e) => rep.errors.push(e),
        }
    }
    rep
}

/// Renders every project's config and records the ones that changed.
pub fn snapshot_all(
    conn: &Connection,
    vault: &Value,
    only_project: Option<&str>,
    cause: &str,
    actor: Option<&str>,
) -> SnapshotReport {
    snapshot_matching(
        conn,
        vault,
        |id, name| only_project.is_none_or(|query| query == id || query == name),
        cause,
        actor,
    )
}

fn entry_revisions(conn: &Connection) -> Result<BTreeMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT data, rev FROM vault_rows WHERE kind = 'entry'")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut revisions = BTreeMap::new();
    for row in rows {
        let (data, rev) = row.map_err(|error| error.to_string())?;
        let entry: Value = serde_json::from_str(&data).map_err(|error| error.to_string())?;
        if let Some(id) = entry.get("id").and_then(Value::as_str) {
            revisions.insert(id.to_string(), rev);
        }
    }
    Ok(revisions)
}

fn project_revisions(conn: &Connection) -> Result<BTreeMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT data, rev FROM vault_rows WHERE kind = 'project'")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut revisions = BTreeMap::new();
    for row in rows {
        let (data, rev) = row.map_err(|error| error.to_string())?;
        let project: Value = serde_json::from_str(&data).map_err(|error| error.to_string())?;
        if let Some(id) = project.get("id").and_then(Value::as_str) {
            revisions.insert(id.to_string(), rev);
        }
    }
    Ok(revisions)
}

/// Entry ids whose current values can affect one project's rendered streams.
/// Complex cross-project forms are deliberately conservative: rendering an
/// extra project is preferable to missing a new config dependency.
fn project_dependencies(vault: &Value, project: &Value) -> BTreeSet<String> {
    let entries = vault
        .get("api_keys")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut bindings: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for binding in project
        .get("bindings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(name), Some(id)) = (
            binding.get("name").and_then(Value::as_str),
            binding.get("entry_id").and_then(Value::as_str),
        ) {
            bindings
                .entry(name.to_string())
                .or_default()
                .push(id.to_string());
        }
    }
    let mut dependencies = BTreeSet::new();
    for field in project
        .get("chunks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|chunk| {
            chunk
                .get("fields")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
    {
        let Some(value) = field.get("value").and_then(Value::as_str) else {
            continue;
        };
        let Some(inner) = value
            .strip_prefix("${")
            .and_then(|value| value.strip_suffix('}'))
        else {
            continue;
        };
        if inner.starts_with("bundle:") || inner.starts_with("chunk:") {
            dependencies.extend(
                entries.iter().filter_map(|entry| {
                    entry.get("id").and_then(Value::as_str).map(str::to_string)
                }),
            );
            continue;
        }
        if let Some(ids) = bindings.get(inner) {
            if ids.len() == 1 {
                dependencies.insert(ids[0].clone());
            }
            continue;
        }
        let provider = inner.split_once('/').map_or(inner, |(name, _)| name);
        if let Some(entry) = crate::refs::find_entry(&entries, provider) {
            if let Some(id) = entry.get("id").and_then(Value::as_str) {
                dependencies.insert(id.to_string());
            }
        }
    }
    dependencies
}

fn current_render_revisions(
    conn: &Connection,
    vault: &Value,
) -> Result<RenderRevisionState, String> {
    let entries = entry_revisions(conn)?;
    let projects_by_id = project_revisions(conn)?;
    let mut current = RenderRevisionState::default();
    for project in projects(vault) {
        let Some(id) = project.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(project_rev) = projects_by_id.get(id) else {
            // A migration from an old vault can briefly have no split row.
            // Treat it as changed rather than claiming nothing depends on it.
            continue;
        };
        let dependencies = project_dependencies(vault, &project)
            .into_iter()
            .map(|entry_id| {
                let revision = entries.get(&entry_id).cloned().unwrap_or_default();
                (entry_id, revision)
            })
            .collect();
        current.projects.insert(
            id.to_string(),
            ProjectRevision {
                project_rev: project_rev.clone(),
                dependencies,
            },
        );
    }
    Ok(current)
}

fn load_render_revisions(conn: &Connection) -> Result<Option<RenderRevisionState>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM vault_meta WHERE key = ?1",
            [RENDER_REVISIONS_META],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    raw.map(|value| serde_json::from_str(&value).map_err(|error| error.to_string()))
        .transpose()
}

fn save_render_revisions(conn: &Connection, revisions: &RenderRevisionState) -> Result<(), String> {
    let value = serde_json::to_string(revisions).map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?1, ?2)",
        [RENDER_REVISIONS_META, &value],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// The differing lines of a diff reduced to their names: the text before the
/// first `=`, `:` or space, and "(value hidden)". Safe to print.
fn masked_summary(ops: &[vault_core::textdiff::Op<'_>]) -> String {
    use vault_core::textdiff::Op;
    let name = |l: &str| -> String {
        let t = l.trim_start();
        let end = t.find(['=', ':', ' ', '\t']).unwrap_or(t.len());
        t[..end].chars().take(48).collect()
    };
    let mut out = String::new();
    let mut line = 0usize;
    for op in ops {
        match op {
            Op::Equal(_) => line += 1,
            Op::Delete(l) => {
                line += 1;
                out.push_str(&format!("- line {line}: {} (value hidden)\n", name(l)));
            }
            Op::Insert(l) => out.push_str(&format!("+ {} (value hidden)\n", name(l))),
        }
    }
    out
}

/// The hook every writer calls after a successful save. Never fails the save: a
/// history that cannot record is reported on the log, not by losing the user's
/// edit.
pub fn after_save(conn: &Connection, vault: &Value, actor: Option<&str>) -> SnapshotReport {
    if !ch::policy(conn).enabled {
        return SnapshotReport::default();
    }
    let current = match current_render_revisions(conn, vault) {
        Ok(current) => current,
        Err(error) => {
            tracing::warn!(error = %error, "config history could not inspect row revisions");
            return SnapshotReport::default();
        }
    };
    let previous = match load_render_revisions(conn) {
        Ok(previous) => previous,
        Err(error) => {
            tracing::warn!(error = %error, "config history could not read render revisions");
            None
        }
    };
    let changed: BTreeSet<String> = match &previous {
        None => current.projects.keys().cloned().collect(),
        Some(previous) => current
            .projects
            .iter()
            .filter(|(id, revision)| previous.projects.get(*id) != Some(*revision))
            .map(|(id, _)| id.clone())
            .collect(),
    };
    let rep = snapshot_matching(conn, vault, |id, _| changed.contains(id), "save", actor);
    for e in &rep.errors {
        tracing::warn!(error = %e, "config history could not record a snapshot");
    }
    if rep.errors.is_empty() {
        if let Err(error) = save_render_revisions(conn, &current) {
            tracing::warn!(error = %error, "config history could not save render revisions");
        }
    }
    rep
}

fn s<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
}
fn n(args: &Value, k: &str) -> Option<i64> {
    args.get(k).and_then(Value::as_i64)
}
fn b(args: &Value, k: &str) -> bool {
    args.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// One history operation. `op` and `args` are the same on every surface.
pub fn call(
    conn: &Connection,
    op: &str,
    args: &Value,
    actor: Option<&str>,
) -> Result<Value, String> {
    match op {
        "list" => {
            let rows = ch::list(
                conn,
                s(args, "project"),
                s(args, "exporter"),
                s(args, "since"),
                n(args, "limit").unwrap_or(50),
            )?;
            Ok(json!({ "snapshots": rows }))
        }
        "show" => {
            let seq = n(args, "seq").ok_or("show needs a snapshot number")?;
            let snap = ch::get(conn, seq)?.ok_or_else(|| format!("No snapshot #{seq}"))?;
            let reveal = b(args, "reveal");
            Ok(json!({
                "meta": snap.meta,
                "revealed": reveal,
                "text": if reveal { snap.content } else { snap.masked },
            }))
        }
        "diff" => {
            let (from, to) = ch::resolve_pair(
                conn,
                s(args, "project"),
                s(args, "exporter"),
                n(args, "from"),
                n(args, "to"),
            )?;
            let reveal = b(args, "reveal");
            let text = ch::diff(
                conn,
                from,
                to,
                !reveal,
                n(args, "context").unwrap_or(3).clamp(0, 50) as usize,
            )?;
            let (added, removed) = {
                let sa = ch::get(conn, from)?.ok_or("missing snapshot")?;
                let sb = ch::get(conn, to)?.ok_or("missing snapshot")?;
                let pick = |x: &ch::Snapshot| {
                    if reveal {
                        x.content.clone()
                    } else {
                        x.masked.clone()
                    }
                };
                vault_core::textdiff::stat(&vault_core::textdiff::diff_ops(&pick(&sa), &pick(&sb)))
            };
            Ok(
                json!({ "from": from, "to": to, "revealed": reveal, "added": added, "removed": removed, "diff": text }),
            )
        }
        // Phase 35 leftover: a stored snapshot against text the caller supplies,
        // typically a node's live file fetched with `unv node pull`.
        "diff_text" => {
            let seq = match n(args, "seq") {
                Some(x) => x,
                None => ch::list(conn, s(args, "project"), s(args, "exporter"), None, 1)?
                    .first()
                    .map(|m| m.seq)
                    .ok_or("There is no snapshot of that config yet")?,
            };
            let snap = ch::get(conn, seq)?.ok_or_else(|| format!("No snapshot #{seq}"))?;
            let other = args
                .get("text")
                .and_then(Value::as_str)
                .ok_or("diff_text needs the text to compare")?;
            let reveal = b(args, "reveal");
            let ops = vault_core::textdiff::diff_ops(&snap.content, other);
            let (added, removed) = vault_core::textdiff::stat(&ops);
            let diff = if reveal {
                vault_core::textdiff::unified(
                    &format!("#{seq} {}", snap.meta.at),
                    "the file",
                    &snap.content,
                    other,
                    n(args, "context").unwrap_or(3).clamp(0, 50) as usize,
                )
            } else {
                // The file is real text and the snapshot's masking was made from the
                // vault as it stood then, so a masked line-by-line diff cannot be
                // promised to hide a value. Without --reveal only the *names* of the
                // lines that differ are shown, and never what follows them.
                masked_summary(&ops)
            };
            Ok(
                json!({ "seq": seq, "revealed": reveal, "added": added, "removed": removed, "identical": added + removed == 0, "diff": diff }),
            )
        }
        "snapshot" => {
            let vault = vault_core::load_vault(conn)?.ok_or("The vault is empty")?;
            let rep = snapshot_all(conn, &vault, s(args, "project"), "manual", actor);
            Ok(json!({
                "recorded": rep.recorded, "unchanged": rep.unchanged,
                "skipped": rep.skipped, "errors": rep.errors,
            }))
        }
        "prune" => {
            let pol = ch::policy(conn);
            let rep = ch::prune(
                conn,
                n(args, "keep").unwrap_or(pol.keep),
                n(args, "days").unwrap_or(pol.days),
                b(args, "dry_run"),
                actor,
            )?;
            Ok(serde_json::to_value(rep).map_err(|e| e.to_string())?)
        }
        "verify" => Ok(serde_json::to_value(ch::verify(conn)?).map_err(|e| e.to_string())?),
        "stats" => Ok(serde_json::to_value(ch::stats(conn)?).map_err(|e| e.to_string())?),
        "policy" => {
            let set = args.get("enabled").is_some()
                || args.get("keep").is_some()
                || args.get("days").is_some();
            let p = if set {
                ch::set_policy(
                    conn,
                    args.get("enabled").and_then(Value::as_bool),
                    n(args, "keep"),
                    n(args, "days"),
                )?
            } else {
                ch::policy(conn)
            };
            Ok(serde_json::to_value(p).map_err(|e| e.to_string())?)
        }
        "blast" => {
            let host = s(args, "host").ok_or("blast needs a node name")?;
            let vault = vault_core::load_vault(conn)?.unwrap_or_else(|| json!({ "api_keys": [] }));
            let current = crate::exposure::current_map(&vault);
            let deployments = node_deployments(conn, host)?;
            let report = vault_core::blast::report(
                host,
                s(args, "since"),
                &deployments,
                &|d| ch::exposed_by_sha(conn, &d.sha256).ok().flatten(),
                &|id| current.get(id).cloned(),
            );
            Ok(serde_json::to_value(report).map_err(|e| e.to_string())?)
        }
        "where" => {
            let sha = s(args, "sha256").ok_or("where needs a sha256")?;
            Ok(json!({ "snapshots": ch::find_by_sha(conn, sha, 5)? }))
        }
        other => Err(format!("Unknown history operation '{other}'")),
    }
}

/// Every apply a node reported, from the hub's audit chain. The time is the
/// audit row's (the hub's clock), not the one the node put in its own report:
/// a node being investigated is not the source for when things happened.
fn node_deployments(
    conn: &Connection,
    node: &str,
) -> Result<Vec<vault_core::blast::Deployment>, String> {
    let mut out = Vec::new();
    for row in vault_core::load_audit(conn)? {
        if row.action != "node.apply" || row.entry_provider.as_deref() != Some(node) {
            continue;
        }
        let d: Value =
            serde_json::from_str(row.details.as_deref().unwrap_or("{}")).unwrap_or_default();
        out.push(vault_core::blast::Deployment {
            at: row.timestamp,
            via: d["target"].as_str().unwrap_or("?").to_string(),
            sha256: d["sha256"].as_str().unwrap_or("").to_string(),
            ok: d["ok"].as_bool().unwrap_or(false),
            error: d["error"].as_str().map(String::from),
        });
    }
    out.reverse(); // oldest first
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        vault_core::init_schema(&c).unwrap();
        c
    }

    fn vault(secret: &str) -> Value {
        json!({
            "api_keys": [{ "id": "e1", "provider": "Stripe", "secretType": "api_key", "api_key": secret }],
            "user_categories": [],
            "projects": [{
                "id": "web", "name": "web", "project_type": "generic",
                "chunks": [{
                    "id": "c1", "name": "app.env", "chunk_type": "env_file",
                    "fields": [
                        { "key": "STRIPE_KEY", "value": "${Stripe/api_key}" },
                        { "key": "MODE", "value": "live" }
                    ]
                }]
            }, {
                "id": "empty", "name": "nothing", "project_type": "generic", "chunks": []
            }]
        })
    }

    #[test]
    fn snapshotting_records_changes_once_and_skips_projects_with_nothing_to_render() {
        let c = conn();
        let v = vault("sk_live_ONE");
        let r = snapshot_all(&c, &v, None, "save", Some("owner"));
        assert_eq!((r.recorded, r.unchanged, r.skipped), (1, 0, 1));
        let again = snapshot_all(&c, &v, None, "save", None);
        assert_eq!((again.recorded, again.unchanged), (0, 1));
        let rotated = snapshot_all(&c, &vault("sk_live_TWO"), None, "save", None);
        assert_eq!(
            rotated.recorded, 1,
            "rotating a secret changes the deployed file"
        );
    }

    #[test]
    fn the_stored_text_has_the_secret_and_the_masked_text_does_not() {
        let c = conn();
        snapshot_all(&c, &vault("sk_live_ONE"), None, "save", None);
        let shown = call(&c, "show", &json!({ "seq": 1 }), None).unwrap();
        assert!(!shown["text"].as_str().unwrap().contains("sk_live_ONE"));
        let revealed = call(&c, "show", &json!({ "seq": 1, "reveal": true }), None).unwrap();
        assert!(revealed["text"].as_str().unwrap().contains("sk_live_ONE"));
    }

    #[test]
    fn a_rotation_shows_as_a_changed_fingerprint_without_either_secret() {
        let c = conn();
        snapshot_all(&c, &vault("sk_live_ONE"), None, "save", None);
        snapshot_all(&c, &vault("sk_live_TWO"), None, "save", None);
        let d = call(&c, "diff", &json!({ "project": "web" }), None).unwrap();
        let text = d["diff"].as_str().unwrap();
        assert!(!text.contains("sk_live_"), "{text}");
        assert!(
            text.contains("-STRIPE_KEY=") && text.contains("+STRIPE_KEY="),
            "{text}"
        );
        assert_eq!(
            (d["added"].as_u64(), d["removed"].as_u64()),
            (Some(1), Some(1))
        );
        let real = call(
            &c,
            "diff",
            &json!({ "project": "web", "reveal": true }),
            None,
        )
        .unwrap();
        assert!(real["diff"].as_str().unwrap().contains("sk_live_TWO"));
    }

    #[test]
    fn a_snapshot_against_a_nodes_live_file_hides_values_unless_asked() {
        let c = conn();
        snapshot_all(&c, &vault("sk_live_ONE"), None, "save", None);
        let real = call(&c, "show", &json!({ "seq": 1, "reveal": true }), None).unwrap();
        let deployed = real["text"].as_str().unwrap().to_string();
        // The node still has exactly what was rendered.
        let same = call(
            &c,
            "diff_text",
            &json!({ "project": "web", "text": deployed }),
            None,
        )
        .unwrap();
        assert_eq!(same["identical"], true, "{same}");
        // Someone edited the live file by hand; the newest snapshot is the default.
        let drifted = deployed.replace("sk_live_ONE", "sk_live_HAND_EDITED");
        let d = call(
            &c,
            "diff_text",
            &json!({ "project": "web", "text": drifted }),
            None,
        )
        .unwrap();
        let text = d["diff"].as_str().unwrap();
        assert_eq!(d["identical"], false);
        assert!(
            !text.contains("sk_live_"),
            "a value reached the masked view: {text}"
        );
        assert!(
            text.contains("STRIPE_KEY") && text.contains("value hidden"),
            "{text}"
        );
        let r = call(
            &c,
            "diff_text",
            &json!({ "seq": 1, "text": drifted, "reveal": true }),
            None,
        )
        .unwrap();
        assert!(r["diff"].as_str().unwrap().contains("sk_live_HAND_EDITED"));
        // Nothing to compare, or no such config: refused, not an empty diff.
        call(&c, "diff_text", &json!({ "project": "web" }), None).unwrap_err();
        call(
            &c,
            "diff_text",
            &json!({ "project": "ghost", "text": "x" }),
            None,
        )
        .unwrap_err();
    }

    #[test]
    fn disabled_history_snapshots_nothing_and_manual_snapshot_says_so() {
        let c = conn();
        call(&c, "policy", &json!({ "enabled": false }), None).unwrap();
        assert_eq!(
            snapshot_all(&c, &vault("k"), None, "save", None),
            SnapshotReport::default()
        );
    }

    #[test]
    fn the_operations_cover_list_where_policy_prune_verify_and_refuse_the_unknown() {
        let c = conn();
        call(&c, "snapshot", &json!({}), None).unwrap_err(); // empty vault
        vault_core::save_vault(&c, vault("sk_live_ONE"), vault_core::SaveCtx::default()).unwrap();
        let r = call(&c, "snapshot", &json!({ "project": "web" }), Some("owner")).unwrap();
        assert_eq!(r["recorded"], 1);
        let l = call(&c, "list", &json!({ "project": "web" }), None).unwrap();
        let sha = l["snapshots"][0]["sha256"].as_str().unwrap().to_string();
        let w = call(&c, "where", &json!({ "sha256": sha }), None).unwrap();
        assert_eq!(w["snapshots"][0]["cause"], "manual");
        assert_eq!(call(&c, "policy", &json!({}), None).unwrap()["keep"], 50);
        assert_eq!(
            call(&c, "prune", &json!({ "dry_run": true }), None).unwrap()["would_delete"],
            0
        );
        assert_eq!(
            call(&c, "verify", &json!({}), None).unwrap()["problems"],
            json!([])
        );
        assert!(call(&c, "explode", &json!({}), None)
            .unwrap_err()
            .contains("Unknown"));
    }

    #[test]
    fn a_nodes_deployments_are_timed_by_the_hubs_audit_row_not_the_nodes_own_claim() {
        let c = conn();
        vault_core::record_event(
            &c,
            "node.apply",
            "vps-01",
            Some(r#"{"target":"nginx","at":"1999-01-01T00:00:00Z","sha256":"abc","ok":true,"error":null}"#),
            Some("node:vps-01"),
        )
        .unwrap();
        vault_core::record_event(&c, "node.apply", "other-node", Some("{}"), None).unwrap();
        vault_core::record_event(&c, "entry.add", "vps-01", Some("{}"), None).unwrap();
        let d = node_deployments(&c, "vps-01").unwrap();
        assert_eq!(
            d.len(),
            1,
            "another node's and another action's rows are not this node's"
        );
        assert_ne!(
            d[0].at, "1999-01-01T00:00:00Z",
            "the node's own timestamp was trusted"
        );
        assert!(d[0].at.starts_with("20"), "{}", d[0].at);
        assert_eq!(
            (d[0].via.as_str(), d[0].sha256.as_str(), d[0].ok),
            ("nginx", "abc", true)
        );
    }

    #[test]
    fn the_blast_operation_honours_since_and_reports_an_empty_host_as_empty() {
        let c = conn();
        vault_core::save_vault(&c, vault("sk_live_ONE"), vault_core::SaveCtx::default()).unwrap();
        snapshot_all(&c, &vault("sk_live_ONE"), None, "save", None);
        let sha = call(&c, "list", &json!({}), None).unwrap()["snapshots"][0]["sha256"]
            .as_str()
            .unwrap()
            .to_string();
        vault_core::record_event(
            &c,
            "node.apply",
            "vps-01",
            Some(&json!({ "target": "env", "sha256": sha, "ok": true, "error": null }).to_string()),
            Some("node:vps-01"),
        )
        .unwrap();
        let all = call(&c, "blast", &json!({ "host": "vps-01" }), None).unwrap();
        assert_eq!(all["deployments"], 1);
        assert_eq!(all["entries"][0]["provider"], "Stripe");
        assert_eq!(all["entries"][0]["still_current"], true);
        let later = call(
            &c,
            "blast",
            &json!({ "host": "vps-01", "since": "2999-01-01T00:00:00Z" }),
            None,
        )
        .unwrap();
        assert_eq!(later["deployments"], 0, "since was ignored");
        let none = call(&c, "blast", &json!({ "host": "nobody" }), None).unwrap();
        assert_eq!(
            (
                none["deployments"].as_i64(),
                none["entries"].as_array().map(Vec::len)
            ),
            (Some(0), Some(0))
        );
        assert!(call(&c, "blast", &json!({}), None).is_err());
    }

    #[test]
    fn compose_is_two_streams_because_the_env_file_is_part_of_the_deliverable() {
        let v = json!({ "api_keys": [], "projects": [{ "id": "c", "name": "c", "project_type": "docker", "chunks": [] }] });
        let ex: Vec<String> = streams(&v).into_iter().map(|s| s.2).collect();
        assert_eq!(ex, vec!["compose", "compose-env"]);
    }

    fn two_project_vault(first: &str, second: &str) -> Value {
        json!({
            "api_keys": [
                { "id": "entry-one", "provider": "One", "api_key": first },
                { "id": "entry-two", "provider": "Two", "api_key": second }
            ],
            "user_categories": [],
            "projects": [
                { "id": "one", "name": "one", "project_type": "generic", "chunks": [{ "id": "one-env", "name": "one.env", "chunk_type": "env_file", "fields": [{ "key": "ONE", "value": "${One}" }] }] },
                { "id": "two", "name": "two", "project_type": "generic", "chunks": [{ "id": "two-env", "name": "two.env", "chunk_type": "env_file", "fields": [{ "key": "TWO", "value": "${Two}" }] }] }
            ]
        })
    }

    #[test]
    fn after_save_rerenders_only_the_project_depending_on_a_changed_entry() {
        let c = conn();
        let original = two_project_vault("first", "second");
        vault_core::save_vault(&c, original.clone(), vault_core::SaveCtx::default()).unwrap();
        assert_eq!(after_save(&c, &original, None).considered, 2);

        let changed = two_project_vault("first-rotated", "second");
        vault_core::save_vault(&c, changed.clone(), vault_core::SaveCtx::default()).unwrap();
        let rerender = after_save(&c, &changed, None);

        // Regression: `after_save` rendered every project after an unrelated
        // edit. A changed entry now revisits only the project that references
        // it, while the snapshot count for the other project remains one.
        assert_eq!((rerender.considered, rerender.recorded), (1, 1));
        assert_eq!(ch::list(&c, Some("one"), None, None, 10).unwrap().len(), 2);
        assert_eq!(ch::list(&c, Some("two"), None, None, 10).unwrap().len(), 1);
        assert_eq!(after_save(&c, &changed, None).considered, 0);
    }

    fn fifty_by_twenty_vault() -> Value {
        let mut entries = Vec::new();
        let mut projects = Vec::new();
        for project in 0..50 {
            let mut fields = Vec::new();
            for field in 0..20 {
                let id = format!("entry-{project}-{field}");
                let provider = format!("Entry{project}_{field}");
                entries.push(json!({ "id": id, "provider": provider, "api_key": format!("value-{project}-{field}") }));
                fields.push(
                    json!({ "key": format!("KEY_{field}"), "value": format!("${{{provider}}}") }),
                );
            }
            projects.push(json!({
                "id": format!("project-{project}"), "name": format!("project-{project}"), "project_type": "generic",
                "chunks": [{ "id": format!("env-{project}"), "name": "app.env", "chunk_type": "env_file", "fields": fields }]
            }));
        }
        json!({ "api_keys": entries, "user_categories": [], "projects": projects })
    }

    #[test]
    fn fifty_projects_by_twenty_references_rerenders_one_stream_after_one_change() {
        let c = conn();
        let mut large = fifty_by_twenty_vault();
        vault_core::save_vault(&c, large.clone(), vault_core::SaveCtx::default()).unwrap();
        let full_started = std::time::Instant::now();
        let initial = after_save(&c, &large, None);
        let full_elapsed = full_started.elapsed();
        assert_eq!(initial.considered, 50);

        large["api_keys"][347]["api_key"] = json!("rotated-17-7");
        vault_core::save_vault(&c, large.clone(), vault_core::SaveCtx::default()).unwrap();
        let incremental_started = std::time::Instant::now();
        let incremental = after_save(&c, &large, None);
        let incremental_elapsed = incremental_started.elapsed();

        // This is the R07 50 x 20 benchmark fixture. Timing is reported for
        // the handoff, while the non-flaky assertion pins the actual contract:
        // one changed credential cannot cause the other 49 configs to render.
        eprintln!(
            "R07 50x20 history benchmark: initial={full_elapsed:?}, incremental={incremental_elapsed:?}"
        );
        assert_eq!((incremental.considered, incremental.recorded), (1, 1));
    }
}
