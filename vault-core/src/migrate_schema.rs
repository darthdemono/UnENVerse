// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Ordered, recorded SQLCipher schema migrations.
//!
//! Migration 0001 captures the complete pre-R05 schema, including the explicit
//! column additions that older builds used to attempt on every open. Later
//! migrations can therefore be additive without re-running ad hoc DDL.

use crate::{config_history, ics_feeds, lifecycle, migration, ops, storage, users};
use rusqlite::{Connection, OptionalExtension};

/// Add one known column only when this known table does not already have it.
///
/// The table and column names are compile-time migration constants. This is not
/// a general SQL builder and must never receive user input.
pub(crate) fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| e.to_string())?;
    let found = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|name| name == column);
    if !found {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
        ))
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn migration_0001(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS vault (
             id   INTEGER PRIMARY KEY CHECK (id = 1),
             data TEXT    NOT NULL
         );
         CREATE TABLE IF NOT EXISTS vault_audit (
             id             INTEGER PRIMARY KEY AUTOINCREMENT,
             action         TEXT    NOT NULL,
             entry_provider TEXT,
             timestamp      TEXT    NOT NULL,
             details        TEXT
         );
         CREATE TABLE IF NOT EXISTS vault_meta (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )
    .map_err(|e| e.to_string())?;
    add_column_if_missing(conn, "vault_audit", "entry_hash", "TEXT")?;
    add_column_if_missing(conn, "vault_audit", "prev_hash", "TEXT")?;
    add_column_if_missing(conn, "vault_audit", "actor", "TEXT")?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS vault_audit_provider ON vault_audit (entry_provider);
         CREATE INDEX IF NOT EXISTS vault_audit_time ON vault_audit (timestamp);",
    )
    .map_err(|e| e.to_string())?;
    storage::init_schema(conn)?;
    users::init_users_schema(conn)?;
    ics_feeds::init_schema(conn)?;
    config_history::init_schema(conn)?;
    Ok(())
}

fn migration_0002(conn: &Connection) -> Result<(), String> {
    ops::init_schema(conn)
}

fn migration_0003(conn: &Connection) -> Result<(), String> {
    lifecycle::init_schema(conn)
}

fn migration_0004(conn: &Connection) -> Result<(), String> {
    migration::init_schema(conn)
}

/// A vault gets its `vault_id`, an `instance_id`, an epoch and a role (see `identity`).
fn migration_0005(conn: &Connection) -> Result<(), String> {
    crate::identity::ensure(conn).map(|_| ())
}

/// Applies every pending migration in one immediate transaction.
pub fn migrate_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                 id          INTEGER PRIMARY KEY,
                 name        TEXT NOT NULL,
                 applied_at  TEXT NOT NULL,
                 app_version TEXT NOT NULL
             );",
        )
        .map_err(|e| e.to_string())?;
        for (id, name, apply) in [
            (
                1_i64,
                "baseline",
                migration_0001 as fn(&Connection) -> Result<(), String>,
            ),
            (
                2_i64,
                "ops_records",
                migration_0002 as fn(&Connection) -> Result<(), String>,
            ),
            (
                3_i64,
                "metadata_lifecycle",
                migration_0003 as fn(&Connection) -> Result<(), String>,
            ),
            (
                4_i64,
                "migration_core",
                migration_0004 as fn(&Connection) -> Result<(), String>,
            ),
            (
                5_i64,
                "vault_identity",
                migration_0005 as fn(&Connection) -> Result<(), String>,
            ),
        ] {
            let done: Option<i64> = conn
                .query_row(
                    "SELECT id FROM schema_migrations WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if done.is_none() {
                apply(conn)?;
                conn.execute(
                    "INSERT INTO schema_migrations (id, name, applied_at, app_version) \
                     VALUES (?1, ?2, ?3, ?4)",
                    (id, name, crate::iso_now(), env!("CARGO_PKG_VERSION")),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT;").map_err(|e| e.to_string()),
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(err)
        }
    }
}
