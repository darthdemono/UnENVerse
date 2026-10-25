// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! `unv entry forget`: purge the old values of one entry from the vault's own history.
//!
//! What is purged, and what is not:
//!
//! * the entry's `version_history` (its old values), which lives in `vault_history`;
//! * the stored content of every config-history snapshot that recorded the entry as exposed. The
//!   snapshot row stays, with its hash and chain link, marked `forgotten`, so the chain still
//!   verifies and the timeline still shows that a file existed (the masked text is kept);
//! * a pending rotation value for a finished run of the entry.
//!
//! The entry's current value is not touched: forgetting is for values already replaced, and a live
//! credential is rotated or deleted instead. `vault_saves` and `vault_changes` hold hashes only,
//! so there is nothing in them to purge. An audit row `entry.forget` records that it happened and
//! how much was removed, never a value.
//!
//! [`Report::unreachable`] lists what this cannot touch, because it is not in the vault database.

use crate::config_history;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Report {
    /// Old values removed from the entry's version history.
    pub history_versions: usize,
    /// Config-history snapshots whose stored content was purged.
    pub snapshots: usize,
    /// Pending rotation values removed.
    pub pending_values: usize,
    /// Where copies can still be, which no command here can reach.
    pub unreachable: Vec<&'static str>,
}

const UNREACHABLE: [&str; 5] = [
    "backups and recovery packages made before now (they keep what the vault held then)",
    "nodes that received a file containing the value",
    "the source the entry was imported from (Git store, password manager, service)",
    "a `.v1.bak` left by an old schema migration",
    "freed disk space: an SSD or copy-on-write filesystem may keep old blocks",
];

/// Whether a rotation of this entry is still running (its run is not finished).
fn rotation_running(conn: &Connection, entry_id: &str, provider: &str) -> Result<bool, String> {
    crate::rotation::init_schema(conn)?;
    Ok(crate::rotation::list_runs(conn)?
        .iter()
        .any(|r| !r.is_finished() && (r.entry == entry_id || r.entry == provider)))
}

pub fn forget_entry(
    conn: &Connection,
    entry_id: &str,
    provider: &str,
    actor: Option<&str>,
) -> Result<Report, String> {
    config_history::init_schema(conn)?;
    if rotation_running(conn, entry_id, provider)? {
        return Err(format!(
            "A rotation of {provider} is still running; finish or abandon it before forgetting its history"
        ));
    }
    let key = format!("id\u{1}{entry_id}");
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM vault_rows WHERE kind = 'entry' AND key = ?1",
            params![key],
            |_| Ok(true),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or(false);
    if !exists {
        return Err(format!("No entry with id {entry_id}"));
    }
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let done = (|| -> Result<Report, String> {
        let history_versions = match conn
            .query_row(
                "SELECT data FROM vault_history WHERE key = ?1",
                params![key],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        {
            Some(text) => {
                let n = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|v| v.as_array().map(|a| a.len()))
                    .unwrap_or(1);
                conn.execute("DELETE FROM vault_history WHERE key = ?1", params![key])
                    .map_err(|e| e.to_string())?;
                n
            }
            None => 0,
        };

        // The id is a UUID, so a text match narrows the rows; the parsed list decides.
        let like = format!("%{}%", entry_id.replace(['%', '_'], ""));
        let candidates: Vec<(i64, String)> = {
            let mut st = conn
                .prepare(
                    "SELECT seq, exposed FROM config_snapshots WHERE forgotten = 0 AND exposed LIKE ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = st
                .query_map(params![like], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
        };
        let mut snapshots = 0;
        for (seq, exposed) in candidates {
            let list: Vec<crate::blast::Exposed> =
                serde_json::from_str(&exposed).map_err(|e| e.to_string())?;
            if list.iter().any(|x| x.entry_id == entry_id) {
                conn.execute(
                    "UPDATE config_snapshots SET content = '', forgotten = 1 WHERE seq = ?1",
                    params![seq],
                )
                .map_err(|e| e.to_string())?;
                snapshots += 1;
            }
        }

        let pending_values = conn
            .execute(
                "DELETE FROM rotation_pending WHERE run_id IN \
                 (SELECT id FROM rotation_runs WHERE entry = ?1 OR entry = ?2)",
                params![entry_id, provider],
            )
            .map_err(|e| e.to_string())?;

        crate::record_event(
            conn,
            "entry.forget",
            provider,
            Some(
                &serde_json::json!({
                    "entry_id": entry_id, "history_versions": history_versions,
                    "snapshots": snapshots, "pending_values": pending_values
                })
                .to_string(),
            ),
            actor,
        )?;
        Ok(Report {
            history_versions,
            snapshots,
            pending_values,
            unreachable: UNREACHABLE.to_vec(),
        })
    })();
    match done {
        Ok(r) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(r)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_history::{self, Stream};
    use serde_json::json;

    const OLD: &str = "OLD-SECRET-VALUE-aaaa";

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::init_schema(&c).unwrap();
        crate::save_vault(
            &c,
            json!({ "api_keys": [
                { "id": "a", "provider": "A", "api_key": "live-a",
                  "version_history": [{ "value": OLD, "saved_at": "2026-01-01T00:00:00Z" }] },
                { "id": "b", "provider": "B", "api_key": "live-b",
                  "version_history": [{ "value": "b-old", "saved_at": "2026-01-01T00:00:00Z" }] }
            ] }),
            crate::SaveCtx {
                actor: None,
                expect_version: None,
            },
        )
        .unwrap();
        c
    }

    fn snap(c: &Connection, exporter: &str, body: &str, entry_id: &str) -> i64 {
        let exposed = json!([{ "entry_id": entry_id, "provider": "X", "key_id": "",
            "field": "version_history", "fp": "f", "short": false }])
        .to_string();
        let s = Stream {
            project: "p1",
            project_name: "edge",
            exporter,
        };
        config_history::record(c, &s, body, "masked", &exposed, "save", None)
            .unwrap()
            .unwrap()
    }

    /// Decrypt-and-grep stand-in: every text column of every table, searched for the value.
    fn mentions(c: &Connection, needle: &str) -> bool {
        let tables: Vec<String> = c
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        for t in tables {
            let cols: Vec<String> = c
                .prepare(&format!("PRAGMA table_info(\"{t}\")"))
                .unwrap()
                .query_map([], |r| r.get(1))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            for col in cols {
                let n: i64 = c
                    .query_row(
                        &format!(
                            "SELECT count(*) FROM \"{t}\" WHERE CAST(\"{col}\" AS TEXT) LIKE ?1"
                        ),
                        [format!("%{needle}%")],
                        |r| r.get(0),
                    )
                    .unwrap();
                if n > 0 {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn the_old_value_is_found_before_and_not_after_and_everything_else_survives() {
        let c = db();
        let mine = snap(&c, "nginx", &format!("token={OLD}\n"), "a");
        let other = snap(&c, "compose", "token=b-old\n", "b");
        assert!(mentions(&c, OLD));

        let r = forget_entry(&c, "a", "A", Some("owner")).unwrap();
        assert_eq!((r.history_versions, r.snapshots), (1, 1));
        assert!(!r.unreachable.is_empty());
        assert!(!mentions(&c, OLD), "the value is gone from every table");

        // The other entry is untouched, and the current value of this one stays.
        assert!(config_history::get(&c, other)
            .unwrap()
            .unwrap()
            .content
            .contains("b-old"));
        assert_eq!(
            crate::storage::entry_history(&c, "b").unwrap().unwrap()[0]["value"],
            "b-old"
        );
        assert_eq!(
            crate::storage::entry_history(&c, "a").unwrap().unwrap(),
            json!([])
        );
        assert!(mentions(&c, "live-a"));

        // The snapshot is inert but still on the timeline; its content cannot be read back.
        assert!(config_history::get(&c, mine)
            .err()
            .unwrap()
            .contains("forgotten"));
        assert_eq!(
            config_history::list(&c, Some("p1"), None, None, 10)
                .unwrap()
                .len(),
            2
        );

        // Tombstone present, no value in it, both chains verify.
        let audit = crate::load_audit(&c).unwrap();
        let row = audit.iter().find(|a| a.action == "entry.forget").unwrap();
        assert!(!row.details.clone().unwrap().contains(OLD));
        assert!(crate::verify_audit(&c).unwrap());
        assert!(config_history::verify(&c).unwrap().problems.is_empty());
        assert!(crate::storage::verify(&c).unwrap());
    }

    #[test]
    fn a_snapshot_that_is_forgotten_but_edited_back_in_is_reported() {
        let c = db();
        let s = snap(&c, "nginx", &format!("token={OLD}\n"), "a");
        forget_entry(&c, "a", "A", None).unwrap();
        c.execute(
            "UPDATE config_snapshots SET content = 'x' WHERE seq = ?1",
            [s],
        )
        .unwrap();
        assert!(!config_history::verify(&c).unwrap().problems.is_empty());
        // And editing the kept hash breaks the chain.
        c.execute(
            "UPDATE config_snapshots SET content = '', sha256 = 'beef' WHERE seq = ?1",
            [s],
        )
        .unwrap();
        assert!(!config_history::verify(&c).unwrap().problems.is_empty());
    }

    #[test]
    fn an_unknown_entry_and_a_running_rotation_are_refused_and_change_nothing() {
        let c = db();
        snap(&c, "nginx", &format!("token={OLD}\n"), "a");
        assert!(forget_entry(&c, "nope", "N", None)
            .unwrap_err()
            .contains("No entry"));

        crate::rotation::init_schema(&c).unwrap();
        let run = crate::rotation::Run::plan(
            "r1",
            "A",
            2,
            crate::rotation::Class::CreateOverlapping,
            crate::rotation::Policy::default(),
            "old-id",
        );
        let mut store = crate::rotation::SqlStore { conn: &c };
        crate::rotation::RunStore::save(&mut store, &run).unwrap();
        let e = forget_entry(&c, "a", "A", None).unwrap_err();
        assert!(e.contains("rotation"), "{e}");
        assert!(mentions(&c, OLD), "nothing was purged");
    }
}
