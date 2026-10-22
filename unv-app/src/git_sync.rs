// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! After a Git-store import: refresh one way from the source, see where the source has moved, and
//! hand authority to UnENVerse (cutover) or take it back (uncutover).
//!
//! While an import is the source's, the source is where a secret is changed. `Access::save` refuses
//! an edit to a secret field of such an entry (`vault_core::SOURCE_OWNED_ERR`), the importer
//! refreshes the value on the next run, and [`drift`] says what changed at the source without
//! importing anything. [`cutover`] is the explicit step that makes the vault authoritative.
//!
//! Cutover is reversible until a credential changes: [`uncutover`] refuses when any secret differs
//! from what it was at cutover, because handing authority back would then discard the newer value.

use crate::access::Access;
use crate::error::{CliError, CliResult};
use crate::password_sources::{self, SourceFormat, SourceOpts};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

fn local(access: &Access) -> CliResult<vault_core::SqlConnection> {
    access.local_conn()?.ok_or_else(|| {
        CliError::invalid(
            "Cutover and drift need direct local vault access and cannot run in remote mode",
        )
    })
}

/// The migration, and every migration of the same source (same kind and label), so an entry that a
/// later sync refreshed is still found under the migration the user names.
fn same_source_ids(
    conn: &vault_core::SqlConnection,
    migration: &str,
) -> CliResult<(vault_core::migration::MigrationInfo, Vec<String>)> {
    let record = vault_core::migration::migration(conn, migration)
        .map_err(CliError::from)?
        .ok_or_else(|| CliError::not_found(format!("No migration {migration}")))?;
    let ids = vault_core::migration::migrations(conn)
        .map_err(CliError::from)?
        .into_iter()
        .filter(|m| m.source_kind == record.source_kind && m.source_label == record.source_label)
        .map(|m| m.id)
        .collect();
    Ok((record, ids))
}

fn entries_mut(vault: &mut Value) -> CliResult<&mut Vec<Value>> {
    vault
        .get_mut("api_keys")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| CliError::invalid("vault has no api_keys list"))
}

fn authority(entry: &Value) -> Option<&Value> {
    entry.get("authority")
}

fn owner(entry: &Value) -> Option<&str> {
    authority(entry)?.get("owner")?.as_str()
}

fn auth_str<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    authority(entry)?.get(key)?.as_str()
}

#[derive(Debug, Serialize)]
pub struct Handover {
    pub migration: String,
    pub source_kind: String,
    pub entries: Vec<String>,
}

/// Makes the vault the authority for every entry imported from this source.
pub fn cutover(access: &Access, migration: &str) -> CliResult<Handover> {
    let conn = local(access)?;
    let (record, ids) = same_source_ids(&conn, migration)?;
    let mut vault = access.load_vault_or_empty()?;
    let now = vault_core::iso_now();
    let mut done: Vec<(String, String)> = Vec::new();
    for entry in entries_mut(&mut vault)?.iter_mut() {
        let mine = owner(entry) == Some("source")
            && auth_str(entry, "source_kind") == Some(record.source_kind.as_str())
            && auth_str(entry, "migration_id").is_some_and(|m| ids.iter().any(|i| i == m));
        if !mine {
            continue;
        }
        let digest = vault_core::secret_digest(entry);
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let name = entry
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let auth = entry["authority"]
            .as_object_mut()
            .expect("authority is an object");
        auth.insert("owner".into(), json!("unenverse"));
        auth.insert("cutover_at".into(), json!(now));
        auth.insert("cutover_from".into(), json!(migration));
        auth.insert("cutover_digest".into(), json!(digest));
        done.push((id, name));
    }
    if done.is_empty() {
        return Err(CliError::not_found(format!(
            "No source-owned entries to cut over for {migration} (already cut over, or nothing was imported)"
        )));
    }
    access.save(&vault)?;
    if !crate::out::dry_run() {
        for (id, _) in &done {
            let _ = vault_core::lifecycle::record_event(
                &conn,
                id,
                "cutover",
                &now,
                "migration",
                json!({ "migration": migration, "source_kind": record.source_kind }),
            );
        }
    }
    Ok(Handover {
        migration: migration.into(),
        source_kind: record.source_kind,
        entries: done.into_iter().map(|(_, name)| name).collect(),
    })
}

/// Hands authority back to the source, unless a credential has changed since cutover.
pub fn uncutover(access: &Access, migration: &str) -> CliResult<Handover> {
    let conn = local(access)?;
    let (record, ids) = same_source_ids(&conn, migration)?;
    let mut vault = access.load_vault_or_empty()?;
    let now = vault_core::iso_now();
    let mut done: Vec<(String, String)> = Vec::new();
    let mut changed: Vec<String> = Vec::new();
    for entry in entries_mut(&mut vault)?.iter_mut() {
        let cut = owner(entry) == Some("unenverse")
            && auth_str(entry, "cutover_at").is_some()
            && auth_str(entry, "source_kind") == Some(record.source_kind.as_str())
            && auth_str(entry, "cutover_from").is_some_and(|m| ids.iter().any(|i| i == m));
        if !cut {
            continue;
        }
        let name = entry
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if auth_str(entry, "cutover_digest") != Some(vault_core::secret_digest(entry).as_str()) {
            changed.push(name);
            continue;
        }
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let auth = entry["authority"]
            .as_object_mut()
            .expect("authority is an object");
        auth.insert("owner".into(), json!("source"));
        for key in ["cutover_at", "cutover_from", "cutover_digest"] {
            auth.remove(key);
        }
        done.push((id, name));
    }
    if !changed.is_empty() {
        return Err(CliError::conflict(format!(
            "Not reversible: {} changed after cutover ({}). Handing authority back would discard the newer value.",
            if changed.len() == 1 { "a credential" } else { "credentials" },
            changed.join(", ")
        )));
    }
    if done.is_empty() {
        return Err(CliError::not_found(format!(
            "Nothing to hand back for {migration} (no entry was cut over from it)"
        )));
    }
    access.save(&vault)?;
    if !crate::out::dry_run() {
        for (id, _) in &done {
            let _ = vault_core::lifecycle::record_event(
                &conn,
                id,
                "uncutover",
                &now,
                "migration",
                json!({ "migration": migration, "source_kind": record.source_kind }),
            );
        }
    }
    Ok(Handover {
        migration: migration.into(),
        source_kind: record.source_kind,
        entries: done.into_iter().map(|(_, name)| name).collect(),
    })
}

#[derive(Debug, Serialize, Default)]
pub struct Drift {
    pub source_kind: String,
    pub source: String,
    /// The source's copy changed since the last import.
    pub source_drift: Vec<String>,
    /// Present at the source, never imported.
    pub new_at_source: Vec<String>,
    /// Imported before, gone from the source now.
    pub missing_at_source: Vec<String>,
    pub unchanged: usize,
}

/// Compares the source, as it is now, with what was last imported. Read-only on both sides:
/// nothing is imported and nothing in the vault changes.
pub fn drift(access: &Access, migration: &str) -> CliResult<Drift> {
    let conn = local(access)?;
    let (record, _) = same_source_ids(&conn, migration)?;
    let format = SourceFormat::parse(&record.source_kind).map_err(CliError::invalid)?;
    if !matches!(
        format,
        SourceFormat::Sops | SourceFormat::Pass | SourceFormat::Gopass
    ) {
        return Err(CliError::invalid(
            "Drift checks apply to the Git sources (sops, pass, gopass); re-run the import to see other sources' changes",
        ));
    }
    let seen = password_sources::source_state(
        format,
        Path::new(&record.source_label),
        &SourceOpts {
            project: None,
            category: None,
            keep_folders: false,
            key_file: None,
            no_password: false,
        },
    )?;
    let known = vault_core::migration::known_source_ids(&conn, &record.source_kind)
        .map_err(CliError::from)?;
    let mut report = Drift {
        source_kind: record.source_kind.clone(),
        source: record.source_label.clone(),
        ..Drift::default()
    };
    for (id, hash) in &seen {
        match vault_core::migration::previous_item(&conn, &record.source_kind, id)
            .map_err(CliError::from)?
        {
            None => report.new_at_source.push(id.clone()),
            Some(previous) if previous.source_hash == *hash => report.unchanged += 1,
            Some(_) => report.source_drift.push(id.clone()),
        }
    }
    for id in known {
        if !seen.iter().any(|(s, _)| *s == id) {
            report.missing_at_source.push(id);
        }
    }
    Ok(report)
}

/// One-way refresh: runs the import again from the source this migration read, so values the
/// source changed are written to the vault and nothing flows back. Preview unless `apply`.
pub fn sync(access: &Access, migration: &str, apply: bool) -> CliResult {
    let conn = local(access)?;
    let (record, _) = same_source_ids(&conn, migration)?;
    let format = SourceFormat::parse(&record.source_kind).map_err(CliError::invalid)?;
    let opts = record.manifest.get("options");
    let text = |key: &str| {
        opts.and_then(|o| o.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let project = text("project");
    let category = text("category");
    let keep_folders = opts
        .and_then(|o| o.get("keep_folders"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    password_sources::run(
        access,
        format,
        Path::new(&record.source_label),
        &SourceOpts {
            project: project.as_deref(),
            category: category.as_deref(),
            keep_folders,
            key_file: None,
            no_password: false,
        },
        apply,
    )
}
