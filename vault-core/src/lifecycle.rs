// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Credential generations and lifecycle facts (roadmap R07).
//!
//! Entry JSON holds current descriptive state. This module holds generations,
//! append-only lifecycle facts, pending credentials and deployment state, so
//! high-churn operational data does not bloat an entry document.

use hmac::{Hmac, KeyInit, Mac};
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::BTreeMap;

type HmacSha256 = Hmac<Sha256>;

const FINGERPRINT_ROOT_META: &str = "credential_fingerprint_root_v1";
const FINGERPRINT_SALT: &[u8] = b"unenverse/credential-generation/v1";
const FINGERPRINT_INFO: &[u8] = b"fingerprint-key";

/// Stable paths of entry values whose replacement creates a generation.
pub const HISTORIED_SECRET_FIELDS: [&str; 5] = [
    "api_key",
    "api_secret",
    "totp_secret",
    "certificate_data",
    "cert_key_data",
];

/// Create R07's relational side tables. Entry metadata remains optional JSON so
/// a prior desktop preserves it during a `formToEntry` round trip.
pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS credential_generations (
             entry_id            TEXT NOT NULL,
             field               TEXT NOT NULL,
             generation          INTEGER NOT NULL,
             fingerprint         TEXT NOT NULL,
             source              TEXT NOT NULL,
             issued_at           TEXT,
             issued_at_precision TEXT NOT NULL DEFAULT 'unknown',
             activated_at        TEXT,
             revoked_at          TEXT,
             state               TEXT NOT NULL DEFAULT 'active',
             PRIMARY KEY (entry_id, field, generation)
         );
         CREATE INDEX IF NOT EXISTS credential_generations_active
             ON credential_generations (entry_id, field, state, generation DESC);
         CREATE TABLE IF NOT EXISTS pending_credentials (
             id         TEXT PRIMARY KEY,
             entry_id   TEXT NOT NULL,
             field      TEXT NOT NULL,
             created_at TEXT NOT NULL,
             state      TEXT NOT NULL,
             detail     TEXT NOT NULL DEFAULT '{}'
         );
         CREATE INDEX IF NOT EXISTS pending_credentials_entry
             ON pending_credentials (entry_id, state);
         CREATE TABLE IF NOT EXISTS lifecycle_events (
             id           INTEGER PRIMARY KEY AUTOINCREMENT,
             entry_id     TEXT NOT NULL,
             kind         TEXT NOT NULL,
             at           TEXT NOT NULL,
             at_precision TEXT NOT NULL DEFAULT 'second',
             source       TEXT NOT NULL,
             detail       TEXT NOT NULL DEFAULT '{}'
         );
         CREATE INDEX IF NOT EXISTS lifecycle_events_entry_at
             ON lifecycle_events (entry_id, at DESC, id DESC);
         CREATE TABLE IF NOT EXISTS deployment_state (
             entry_id       TEXT NOT NULL,
             field          TEXT NOT NULL,
             target_id      TEXT NOT NULL,
             generation     INTEGER NOT NULL,
             applied_sha256 TEXT,
             applied_at     TEXT,
             checked_at     TEXT,
             result         TEXT NOT NULL,
             run_id         TEXT,
             PRIMARY KEY (entry_id, field, target_id)
         );
         CREATE INDEX IF NOT EXISTS deployment_state_entry
             ON deployment_state (entry_id, field, target_id);",
    )
    .map_err(|e| e.to_string())
}

fn entry_id(entry: &Value) -> Option<&str> {
    entry
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
}

fn secret_values(entry: &Value) -> BTreeMap<String, String> {
    let cookie = entry.get("secretType").and_then(Value::as_str) == Some("cookie");
    let mut values = BTreeMap::new();
    for field in HISTORIED_SECRET_FIELDS {
        let public = match field {
            "api_key" => entry
                .get("primary_public")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            "api_secret" => entry
                .get("secret_public")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            _ => false,
        };
        if !cookie && public {
            continue;
        }
        if let Some(value) = entry
            .get(field)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
        {
            values.insert(field.to_string(), value.to_string());
        }
    }
    if let Some(vars) = entry.get("extra_vars").and_then(Value::as_array) {
        for var in vars {
            if !cookie && var.get("public").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let (Some(name), Some(value)) = (
                var.get("key")
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty()),
                var.get("value")
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty()),
            ) else {
                continue;
            };
            values.insert(format!("extra_vars/{name}"), value.to_string());
        }
    }
    values
}

fn root(conn: &Connection) -> Result<[u8; 32], String> {
    let have: Option<String> = conn
        .query_row(
            "SELECT value FROM vault_meta WHERE key = ?1",
            [FINGERPRINT_ROOT_META],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if have.is_none() {
        let mut generated = [0_u8; 32];
        rand::rng().fill_bytes(&mut generated);
        conn.execute(
            "INSERT OR IGNORE INTO vault_meta (key, value) VALUES (?1, ?2)",
            params![FINGERPRINT_ROOT_META, hex::encode(generated)],
        )
        .map_err(|e| e.to_string())?;
    }
    let encoded: String = conn
        .query_row(
            "SELECT value FROM vault_meta WHERE key = ?1",
            [FINGERPRINT_ROOT_META],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let decoded =
        hex::decode(encoded).map_err(|_| "Credential fingerprint root is invalid".to_string())?;
    decoded
        .try_into()
        .map_err(|_| "Credential fingerprint root has the wrong length".to_string())
}

fn hkdf(root: &[u8; 32]) -> Result<[u8; 32], String> {
    let mut extract = HmacSha256::new_from_slice(FINGERPRINT_SALT).map_err(|e| e.to_string())?;
    extract.update(root);
    let prk = extract.finalize().into_bytes();
    let mut expand = HmacSha256::new_from_slice(&prk).map_err(|e| e.to_string())?;
    expand.update(FINGERPRINT_INFO);
    expand.update(&[1]);
    Ok(expand.finalize().into_bytes().into())
}

fn fingerprint(conn: &Connection, field: &str, value: &str) -> Result<String, String> {
    let key = hkdf(&root(conn)?)?;
    let mut mac = HmacSha256::new_from_slice(&key).map_err(|e| e.to_string())?;
    mac.update(field.as_bytes());
    mac.update(&[0]);
    mac.update(value.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn next_generation(conn: &Connection, entry_id: &str, field: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT COALESCE(MAX(generation), 0) + 1 FROM credential_generations WHERE entry_id = ?1 AND field = ?2",
        params![entry_id, field],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

fn insert_generation(
    conn: &Connection,
    entry_id: &str,
    field: &str,
    value: &str,
    source: &str,
    at: Option<&str>,
    issued_at: Option<&str>,
    issued_precision: &str,
) -> Result<i64, String> {
    let generation = next_generation(conn, entry_id, field)?;
    conn.execute(
        "INSERT INTO credential_generations
         (entry_id, field, generation, fingerprint, source, issued_at, issued_at_precision, activated_at, state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active')",
        params![
            entry_id,
            field,
            generation,
            fingerprint(conn, field, value)?,
            source,
            issued_at,
            issued_precision,
            at,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(generation)
}

/// Records one lifecycle event for an entry (a cutover, say). Public so the application layer can
/// write events that no save diff would detect; the detail must carry no secret.
pub fn record_event(
    conn: &Connection,
    entry_id: &str,
    kind: &str,
    at: &str,
    source: &str,
    detail: Value,
) -> Result<(), String> {
    event(conn, entry_id, kind, at, source, detail)
}

fn event(
    conn: &Connection,
    entry_id: &str,
    kind: &str,
    at: &str,
    source: &str,
    detail: Value,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO lifecycle_events (entry_id, kind, at, at_precision, source, detail)
         VALUES (?1, ?2, ?3, 'second', ?4, ?5)",
        params![entry_id, kind, at, source, detail.to_string()],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn provenance_kind(entry: &Value) -> Option<&str> {
    entry
        .get("provenance")
        .and_then(|v| v.get("source_kind").or_else(|| v.get("kind")))
        .and_then(Value::as_str)
}

fn issued(entry: &Value) -> (Option<&str>, &str) {
    let value = entry
        .get("issued_at")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty());
    let precision = entry
        .get("issued_at_precision")
        .and_then(Value::as_str)
        .filter(|v| matches!(*v, "exact" | "date" | "month" | "year" | "unknown"))
        .unwrap_or("unknown");
    (value, precision)
}

/// Seed generation one for entries that predate R07. It runs after row storage
/// migration because a 0.42.x vault can still have its document in the legacy
/// blob when migration 0003 installs the relational tables.
pub fn seed_existing(conn: &Connection, document: &Value) -> Result<(), String> {
    let work = || -> Result<(), String> {
        for entry in document
            .get("api_keys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = entry_id(entry) else { continue };
            for (field, value) in secret_values(entry) {
                let exists: Option<i64> = conn
                    .query_row(
                        "SELECT generation FROM credential_generations WHERE entry_id = ?1 AND field = ?2 LIMIT 1",
                        params![id, field],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                if exists.is_none() {
                    insert_generation(
                        conn,
                        id,
                        &field,
                        &value,
                        "migration",
                        None,
                        None,
                        "unknown",
                    )?;
                }
            }
        }
        Ok(())
    };
    if conn.is_autocommit() {
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
        match work() {
            Ok(()) => crate::commit(conn),
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    } else {
        work()
    }
}

/// Record lifecycle side effects inside the transaction that writes entry rows.
pub fn record_save(
    conn: &Connection,
    old: &BTreeMap<String, Value>,
    next: &[Value],
    at: &str,
) -> Result<(), String> {
    for entry in next {
        let Some(id) = entry_id(entry) else { continue };
        let before = old.get(id);
        let current = secret_values(entry);
        let (issued_at, issued_precision) = issued(entry);
        match before {
            None => {
                let source = if provenance_kind(entry) == Some("import") {
                    "import"
                } else {
                    "manual"
                };
                event(conn, id, "created", at, source, json!({}))?;
                if source == "import" {
                    event(conn, id, "imported", at, "import", json!({}))?;
                }
                for (field, value) in current {
                    insert_generation(
                        conn,
                        id,
                        &field,
                        &value,
                        source,
                        Some(at),
                        issued_at,
                        issued_precision,
                    )?;
                }
            }
            Some(previous) => {
                let old_values = secret_values(previous);
                let source = if entry.get("last_rotated_at") != previous.get("last_rotated_at") {
                    "manual"
                } else {
                    "save"
                };
                for (field, value) in &current {
                    if old_values.get(field) == Some(value) {
                        continue;
                    }
                    conn.execute(
                        "UPDATE credential_generations SET state = 'superseded' WHERE entry_id = ?1 AND field = ?2 AND state = 'active'",
                        params![id, field],
                    )
                    .map_err(|e| e.to_string())?;
                    let generation = insert_generation(
                        conn,
                        id,
                        field,
                        value,
                        source,
                        Some(at),
                        issued_at,
                        issued_precision,
                    )?;
                    event(
                        conn,
                        id,
                        "rotated",
                        at,
                        source,
                        json!({ "field": field, "generation": generation }),
                    )?;
                }
                let verified = entry.get("last_verified_at").and_then(Value::as_str);
                if verified.is_some()
                    && verified != previous.get("last_verified_at").and_then(Value::as_str)
                {
                    event(
                        conn,
                        id,
                        "verified",
                        verified.unwrap_or(at),
                        "probe",
                        json!({}),
                    )?;
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub kind: String,
    pub at: String,
    pub at_precision: String,
    pub source: String,
    pub detail: Value,
}

/// Public, secret-free timeline records for one stable entry id.
pub fn timeline(conn: &Connection, entry_id: &str) -> Result<Vec<TimelineEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT kind, at, at_precision, source, detail FROM lifecycle_events
             WHERE entry_id = ?1 ORDER BY at DESC, id DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([entry_id], |row| {
            let detail: String = row.get(4)?;
            Ok(TimelineEvent {
                kind: row.get(0)?,
                at: row.get(1)?,
                at_precision: row.get(2)?,
                source: row.get(3)?,
                detail: serde_json::from_str(&detail).unwrap_or_else(|_| json!({})),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// Latest deployment state per target. R12 will write reports through this API.
pub fn deployment_state(conn: &Connection, entry_id: &str) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT field, target_id, generation, applied_sha256, applied_at, checked_at, result, run_id
             FROM deployment_state WHERE entry_id = ?1 ORDER BY target_id, field",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([entry_id], |row| {
        Ok(json!({
            "field": row.get::<_, String>(0)?, "target_id": row.get::<_, String>(1)?,
            "generation": row.get::<_, i64>(2)?, "applied_sha256": row.get::<_, Option<String>>(3)?,
            "applied_at": row.get::<_, Option<String>>(4)?, "checked_at": row.get::<_, Option<String>>(5)?,
            "result": row.get::<_, String>(6)?, "run_id": row.get::<_, Option<String>>(7)?,
        }))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// Deployment rows whose recorded generation is no longer the active one.
/// This is the query R12 uses for "rotated but not redeployed" without ever
/// comparing a secret value or cross-vault fingerprint.
pub fn deployment_generation_mismatches(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT d.entry_id, d.field, d.target_id, d.generation, g.generation
             FROM deployment_state d
             JOIN credential_generations g
               ON g.entry_id = d.entry_id AND g.field = d.field AND g.state = 'active'
             WHERE d.generation != g.generation
             ORDER BY d.entry_id, d.field, d.target_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(json!({
                "entry_id": row.get::<_, String>(0)?, "field": row.get::<_, String>(1)?,
                "target_id": row.get::<_, String>(2)?, "deployed_generation": row.get::<_, i64>(3)?,
                "active_generation": row.get::<_, i64>(4)?,
            }))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE vault_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn the_same_value_has_a_different_vault_scoped_fingerprint() {
        let a = db();
        let b = db();
        assert_ne!(
            fingerprint(&a, "api_key", "fixture-value").unwrap(),
            fingerprint(&b, "api_key", "fixture-value").unwrap()
        );
    }

    #[test]
    fn migration_generation_is_idempotent_and_does_not_invent_dates() {
        let conn = db();
        let document = json!({ "api_keys": [{ "id": "entry-1", "api_key": "fixture-value" }] });
        seed_existing(&conn, &document).unwrap();
        seed_existing(&conn, &document).unwrap();
        let rows: Vec<(i64, String, Option<String>, String)> = conn
            .prepare("SELECT generation, source, issued_at, issued_at_precision FROM credential_generations")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(rows, vec![(1, "migration".into(), None, "unknown".into())]);
    }

    #[test]
    fn a_secret_change_creates_a_generation_and_manual_event() {
        let conn = db();
        let old = json!({ "id": "entry-1", "api_key": "old" });
        seed_existing(&conn, &json!({ "api_keys": [old.clone()] })).unwrap();
        let new = json!({ "id": "entry-1", "api_key": "new", "last_rotated_at": "2026-10-14" });
        let mut previous = BTreeMap::new();
        previous.insert("entry-1".into(), old);
        record_save(&conn, &previous, &[new], "2026-10-14T01:02:03Z").unwrap();
        let generations: Vec<(i64, String)> = conn
            .prepare("SELECT generation, state FROM credential_generations ORDER BY generation")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            generations,
            vec![(1, "superseded".into()), (2, "active".into())]
        );
        assert_eq!(timeline(&conn, "entry-1").unwrap()[0].source, "manual");
        conn.execute(
            "INSERT INTO deployment_state (entry_id, field, target_id, generation, result) VALUES ('entry-1', 'api_key', 'web-01', 1, 'ok')",
            [],
        )
        .unwrap();
        assert_eq!(
            deployment_generation_mismatches(&conn).unwrap()[0]["active_generation"],
            2
        );
    }

    #[test]
    fn imported_generation_keeps_the_issuer_date_precision() {
        let conn = db();
        let entry = json!({
            "id": "imported", "api_key": "fixture-value",
            "provenance": { "source_kind": "import" },
            "issued_at": "2024-03", "issued_at_precision": "month",
        });
        record_save(&conn, &BTreeMap::new(), &[entry], "2026-10-14T01:02:03Z").unwrap();
        let row: (String, Option<String>, String) = conn
            .query_row(
                "SELECT source, issued_at, issued_at_precision FROM credential_generations WHERE entry_id = 'imported'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            ("import".into(), Some("2024-03".into()), "month".into())
        );
    }
}
