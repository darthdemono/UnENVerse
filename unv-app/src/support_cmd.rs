// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv support bundle` and `unv demo-vault` (R25): the two things that make a bug report cheap for
//! a solo maintainer. The bundle is a small JSON file the person reads before sending; it holds
//! versions, counts and check results, and never a value, a name, a path or a host. The demo vault
//! is a synthetic vault with invented values, for reproductions.

use crate::access::Access;
use crate::doctor;
use crate::error::{CliError, CliResult};
use crate::{data, out};
use serde_json::{json, Map, Value};
use std::path::Path;

/// Versions, counts and check outcomes of the local vault. Nothing here is read from a field that
/// can hold a name, a path or a value: the doctor checks are reduced to their id and level.
pub fn build(access: &Access) -> CliResult<Value> {
    let conn = access.local_conn()?.ok_or_else(|| {
        CliError::invalid(
            "A support bundle describes the local vault; it cannot run in remote mode",
        )
    })?;
    let vault = access.load_vault_or_empty()?;
    let entries = data::entries(&vault).len();
    let mut project_types: Map<String, Value> = Map::new();
    for p in data::projects(&vault) {
        let t = p
            .get("project_type")
            .and_then(|v| v.as_str())
            .unwrap_or("generic");
        let n = project_types.get(t).and_then(|v| v.as_u64()).unwrap_or(0);
        project_types.insert(t.to_string(), json!(n + 1));
    }
    let mut checks: Vec<Value> = doctor::file_findings(access);
    checks.extend(doctor::document_findings(&vault));
    let doctor: Vec<Value> = checks
        .iter()
        .map(|c| json!({ "check": c["check"], "level": c["level"] }))
        .collect();

    let one = |sql: &str| -> Option<String> { conn.query_row(sql, [], |r| r.get(0)).ok() };
    let schema = conn
        .query_row(
            "SELECT COALESCE(max(id), 0) FROM schema_migrations",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);

    vault_core::ops::init_schema(&conn).map_err(CliError::from)?;
    let ops: Vec<Value> = {
        let mut st = conn
            .prepare(
                "SELECT kind, state, error_code FROM ops_runs ORDER BY created_at DESC LIMIT 50",
            )
            .map_err(|e| CliError::from(e.to_string()))?;
        let rows = st
            .query_map([], |r| {
                Ok(json!({
                    "kind": r.get::<_, String>(0)?,
                    "state": r.get::<_, String>(1)?,
                    "error_code": r.get::<_, Option<String>>(2)?,
                }))
            })
            .map_err(|e| CliError::from(e.to_string()))?;
        rows.filter_map(Result::ok).collect()
    };

    Ok(json!({
        "bundle_version": 1,
        "generated": vault_core::iso_now().chars().take(10).collect::<String>(),
        "unv_version": env!("CARGO_PKG_VERSION"),
        "contract_version": crate::agentio::CONTRACT_VERSION,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "sqlite_version": one("SELECT sqlite_version()"),
        "sqlcipher_version": one("PRAGMA cipher_version"),
        "schema_version": schema,
        "entries": entries,
        "project_types": project_types,
        "doctor": doctor,
        "ops": ops,
    }))
}

pub fn bundle(access: &Access, out_file: Option<&Path>, print: bool) -> CliResult {
    let b = build(access)?;
    let text = serde_json::to_string_pretty(&b).map_err(|e| CliError::from(e.to_string()))?;
    if let Some(p) = out_file {
        std::fs::write(p, format!("{text}\n"))
            .map_err(|e| CliError::from(format!("Cannot write {}: {e}", p.display())))?;
    }
    let show = print || out_file.is_none();
    out::ok(
        "support.bundle",
        json!({ "file": out_file.map(|p| p.display().to_string()), "bundle": b }),
        || {
            if show {
                println!("{text}");
            }
            if let Some(p) = out_file {
                println!("Wrote {}. Read it before you send it: it holds versions, counts and check results only.", p.display());
            }
        },
    );
    Ok(())
}

/// A vault for reproductions: invented projects and values that match no provider's key format,
/// so a bug report can include it without leaking anything and push protection stays quiet.
pub fn demo_vault(dir: &Path) -> CliResult {
    let db = dir.join("vault.db");
    let salt_path = dir.join("vault.salt");
    if db.exists() || salt_path.exists() {
        return Err(CliError::conflict(format!(
            "{} already holds a vault; choose an empty directory",
            dir.display()
        )));
    }
    std::fs::create_dir_all(dir)
        .map_err(|e| CliError::from(format!("Cannot create {}: {e}", dir.display())))?;
    let salt = vault_core::read_or_create_salt(&salt_path).map_err(CliError::from)?;
    // A fresh password for every demo vault, printed once: nothing fixed to look like a real key.
    let password = format!("demo-{}", &vault_core::new_uuid()[..13]);
    let key = vault_core::derive_key(&password, &salt).map_err(CliError::from)?;
    let conn = vault_core::open_db(&db, &key).map_err(CliError::from)?;
    vault_core::init_schema(&conn).map_err(CliError::from)?;
    let entry = |n: u32, name: &str, project: &str, value: &str| {
        json!({
            "id": format!("00000000-0000-4000-8000-{n:012}"),
            "provider": name,
            "api_key": value,
            "price_type": "free",
            "secretType": "api_key",
            "categories": [],
            "projectIds": [project],
            "scopes": [],
            "created_at": "2026-01-01T00:00:00Z",
        })
    };
    let doc = json!({
        "projects": [
            { "id": "Universal", "name": "Universal" },
            { "id": "demo-web", "name": "Demo Web", "project_type": "nginx" },
            { "id": "demo-stack", "name": "Demo Stack", "project_type": "docker" }
        ],
        "api_keys": [
            entry(1, "Demo Service A", "demo-web", "demo-not-a-real-value-0001"),
            entry(2, "Demo Service B", "demo-web", "demo-not-a-real-value-0002"),
            entry(3, "Demo Database", "demo-stack", "demo-not-a-real-value-0003"),
        ],
    });
    vault_core::save_vault(
        &conn,
        doc,
        vault_core::SaveCtx {
            actor: None,
            expect_version: None,
        },
    )
    .map_err(CliError::from)?;
    vault_core::ensure_owner_user(&conn).map_err(CliError::from)?;
    drop(conn);
    out::ok(
        "demo-vault",
        json!({ "dir": dir.display().to_string(), "password": password, "entries": 3 }),
        || {
            println!("Demo vault written to {}", dir.display());
            println!("Password: {password}");
            println!(
                "Use it with: unv --db-path {} --password {password} list",
                db.display()
            );
        },
    );
    Ok(())
}
