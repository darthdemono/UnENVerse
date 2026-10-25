// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Durable, encrypted migration manifests and source archives.
//!
//! A migration import changes two things which must agree: the vault document
//! and the source identity used to reconcile the next import.  These tables live
//! in SQLCipher beside the document, so a source path, an unsupported stanza or
//! a template's original bytes never escape into an unencrypted sidecar.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationItem {
    pub source_kind: String,
    pub source_id: String,
    pub source_hash: String,
    pub target_entry_id: Option<String>,
    pub target_rev: Option<String>,
    pub status: String,
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceArchive {
    pub id: String,
    pub source_kind: String,
    pub source_id: Option<String>,
    pub reason: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub id: String,
    pub source_kind: String,
    pub source_label: String,
    pub created_at: String,
    pub status: String,
    pub manifest: Value,
    pub items: Vec<MigrationItem>,
    pub archives: Vec<SourceArchive>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousItem {
    pub source_hash: String,
    pub target_entry_id: Option<String>,
    pub target_rev: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveInfo {
    pub id: String,
    pub migration_id: String,
    pub source_kind: String,
    pub source_id: Option<String>,
    pub reason: String,
    pub media_type: String,
    pub sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationInfo {
    pub id: String,
    pub source_kind: String,
    pub source_label: String,
    pub created_at: String,
    pub status: String,
    pub manifest: Value,
}

/// Creates the R08 additive tables. Their contents are encrypted because the
/// containing SQLCipher database is encrypted; no source content is copied to a
/// filesystem cache.
pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS migrations (
             id           TEXT PRIMARY KEY,
             source_kind  TEXT NOT NULL,
             source_label TEXT NOT NULL,
             created_at   TEXT NOT NULL,
             status       TEXT NOT NULL,
             manifest     TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS migration_items (
             migration_id    TEXT NOT NULL REFERENCES migrations(id) ON DELETE CASCADE,
             source_kind     TEXT NOT NULL,
             source_id       TEXT NOT NULL,
             source_hash     TEXT NOT NULL,
             target_entry_id TEXT,
             target_rev      TEXT,
             status          TEXT NOT NULL,
             conflict        TEXT,
             PRIMARY KEY (migration_id, source_kind, source_id)
         );
         CREATE INDEX IF NOT EXISTS migration_items_identity
             ON migration_items(source_kind, source_id, migration_id);
         CREATE TABLE IF NOT EXISTS vault_blobs (
             id         TEXT PRIMARY KEY,
             sha256     TEXT NOT NULL,
             media_type TEXT NOT NULL,
             bytes      BLOB NOT NULL,
             created_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS source_archives (
             id           TEXT PRIMARY KEY,
             migration_id TEXT NOT NULL REFERENCES migrations(id) ON DELETE CASCADE,
             source_kind  TEXT NOT NULL,
             source_id    TEXT,
             reason       TEXT NOT NULL,
             blob_id      TEXT NOT NULL REFERENCES vault_blobs(id),
             created_at   TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS source_archives_migration
             ON source_archives(migration_id);",
    )
    .map_err(|e| e.to_string())
}

/// Latest persisted source state, independent of which migration first saw it.
pub fn previous_item(
    conn: &Connection,
    source_kind: &str,
    source_id: &str,
) -> Result<Option<PreviousItem>, String> {
    conn.query_row(
        "SELECT source_hash, target_entry_id, target_rev
           FROM migration_items
          WHERE source_kind = ?1 AND source_id = ?2
          ORDER BY rowid DESC LIMIT 1",
        params![source_kind, source_id],
        |row| {
            Ok(PreviousItem {
                source_hash: row.get(0)?,
                target_entry_id: row.get(1)?,
                target_rev: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Writes a migration manifest inside the caller's write transaction.
pub fn record_txn(conn: &Connection, record: &MigrationRecord) -> Result<(), String> {
    let manifest = serde_json::to_string(&record.manifest).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO migrations (id, source_kind, source_label, created_at, status, manifest)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            record.id,
            record.source_kind,
            record.source_label,
            record.created_at,
            record.status,
            manifest,
        ],
    )
    .map_err(|e| e.to_string())?;
    for item in &record.items {
        conn.execute(
            "INSERT INTO migration_items
             (migration_id, source_kind, source_id, source_hash, target_entry_id, target_rev, status, conflict)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id,
                item.source_kind,
                item.source_id,
                item.source_hash,
                item.target_entry_id,
                item.target_rev,
                item.status,
                item.conflict,
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    for archive in &record.archives {
        let hash = sha256_hex(&archive.bytes);
        conn.execute(
            "INSERT INTO vault_blobs (id, sha256, media_type, bytes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                archive.id,
                hash,
                archive.media_type,
                archive.bytes,
                record.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO source_archives
             (id, migration_id, source_kind, source_id, reason, blob_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                archive.id,
                record.id,
                archive.source_kind,
                archive.source_id,
                archive.reason,
                archive.id,
                record.created_at,
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn archives(conn: &Connection) -> Result<Vec<ArchiveInfo>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.migration_id, a.source_kind, a.source_id, a.reason,
                    b.media_type, b.sha256, a.created_at
               FROM source_archives a JOIN vault_blobs b ON b.id = a.blob_id
              ORDER BY a.created_at, a.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ArchiveInfo {
                id: row.get(0)?,
                migration_id: row.get(1)?,
                source_kind: row.get(2)?,
                source_id: row.get(3)?,
                reason: row.get(4)?,
                media_type: row.get(5)?,
                sha256: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn migrations(conn: &Connection) -> Result<Vec<MigrationInfo>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, source_kind, source_label, created_at, status, manifest
               FROM migrations ORDER BY created_at, id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let manifest: String = row.get(5)?;
            Ok(MigrationInfo {
                id: row.get(0)?,
                source_kind: row.get(1)?,
                source_label: row.get(2)?,
                created_at: row.get(3)?,
                status: row.get(4)?,
                manifest: serde_json::from_str(&manifest).unwrap_or(Value::Null),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn migration(conn: &Connection, id: &str) -> Result<Option<MigrationInfo>, String> {
    conn.query_row(
        "SELECT id, source_kind, source_label, created_at, status, manifest FROM migrations WHERE id = ?1",
        [id],
        |row| {
            let manifest: String = row.get(5)?;
            Ok(MigrationInfo {
                id: row.get(0)?,
                source_kind: row.get(1)?,
                source_label: row.get(2)?,
                created_at: row.get(3)?,
                status: row.get(4)?,
                manifest: serde_json::from_str(&manifest).unwrap_or(Value::Null),
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Every source id the latest record of which still describes something that was imported or
/// skipped as unchanged, for one source kind. Archived-only items are not included: they are not
/// entries, so there is nothing in the vault to compare.
pub fn known_source_ids(conn: &Connection, source_kind: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT source_id FROM migration_items m
              WHERE source_kind = ?1 AND target_entry_id IS NOT NULL
                AND rowid = (SELECT MAX(rowid) FROM migration_items
                              WHERE source_kind = m.source_kind AND source_id = m.source_id)
              ORDER BY source_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([source_kind], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn archive_bytes(conn: &Connection, id: &str) -> Result<Option<Vec<u8>>, String> {
    conn.query_row(
        "SELECT b.bytes FROM source_archives a JOIN vault_blobs b ON b.id = a.blob_id WHERE a.id = ?1",
        [id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_identity_and_encrypted_archive_rows_together() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        record_txn(
            &conn,
            &MigrationRecord {
                id: "m1".into(),
                source_kind: "env".into(),
                source_label: "fixture.env".into(),
                created_at: "2026-10-14T00:00:00Z".into(),
                status: "applied".into(),
                manifest: serde_json::json!({"read": 1}),
                items: vec![MigrationItem {
                    source_kind: "env".into(),
                    source_id: "env:fixture:A".into(),
                    source_hash: "abc".into(),
                    target_entry_id: Some("e1".into()),
                    target_rev: Some("r1".into()),
                    status: "imported".into(),
                    conflict: None,
                }],
                archives: vec![SourceArchive {
                    id: "a1".into(),
                    source_kind: "env".into(),
                    source_id: None,
                    reason: "comments preserved".into(),
                    media_type: "text/plain".into(),
                    bytes: b"# comment\nA=value\n".to_vec(),
                }],
            },
        )
        .unwrap();
        conn.execute_batch("COMMIT").unwrap();

        assert_eq!(
            archive_bytes(&conn, "a1").unwrap().unwrap(),
            b"# comment\nA=value\n"
        );
        assert_eq!(archives(&conn).unwrap().len(), 1);
        assert_eq!(
            previous_item(&conn, "env", "env:fixture:A").unwrap(),
            Some(PreviousItem {
                source_hash: "abc".into(),
                target_entry_id: Some("e1".into()),
                target_rev: Some("r1".into()),
            })
        );
    }
}
