// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Several writers, one database (roadmap: chaos testing, storage level).
//!
//! Each scenario puts two or more connections to the same SQLCipher file in the
//! situations a real deployment produces (a server thread and a node event, two
//! clients saving, a reader beside a writer) and then checks invariants that must hold
//! whatever the interleaving. Every test names the bug it was written for; the comment
//! says what the code did before the fix. Revert the fix and the test must fail.
//!
//! Invariants checked by `check`: the integrity hash verifies, the audit chain is
//! linear (no two rows claim the same predecessor) and entry ids are unique.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use vault_core::{
    apply_row_patch, begin_row_patch, init_schema, load_audit, load_vault, open_db, record_event,
    save_vault, vault_version, verify_vault_integrity, SaveCtx, CONFLICT_ERR,
};

static SEQ: AtomicU64 = AtomicU64::new(0);
const KEY: [u8; 32] = [9u8; 32];

struct Db {
    dir: PathBuf,
    path: PathBuf,
}

impl Db {
    fn new(tag: &str) -> Db {
        let dir = std::env::temp_dir().join(format!(
            "unv-chaos-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vault.db");
        let db = Db { dir, path };
        let c = db.conn();
        init_schema(&c).unwrap();
        db
    }

    fn conn(&self) -> rusqlite::Connection {
        open_db(&self.path, &KEY).unwrap()
    }

    /// The invariants that must hold once every writer has finished.
    fn check(&self) {
        let c = self.conn();
        assert!(
            verify_vault_integrity(&c).unwrap(),
            "the integrity hash no longer matches the rows"
        );
        let mut rows = load_audit(&c).unwrap();
        rows.sort_by_key(|r| r.id);
        let mut prev: Option<String> = None;
        for r in &rows {
            assert_eq!(
                r.prev_hash, prev,
                "audit row {} ({}) does not follow the row before it: the chain forked",
                r.id, r.action
            );
            prev = r.entry_hash.clone();
        }
        if let Some(doc) = load_vault(&c).unwrap() {
            let mut seen = std::collections::HashSet::new();
            for e in doc["api_keys"].as_array().into_iter().flatten() {
                let id = e["id"].as_str().unwrap_or("");
                assert!(
                    !id.is_empty() && seen.insert(id.to_string()),
                    "bad entry id {id:?}"
                );
            }
        }
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn entry(id: &str, key: &str) -> Value {
    json!({ "id": id, "provider": format!("P-{id}"), "api_key": key })
}

fn doc(entries: Vec<Value>) -> Value {
    json!({ "api_keys": entries, "projects": [], "user_categories": [] })
}

fn ids(c: &rusqlite::Connection) -> Vec<String> {
    load_vault(c).unwrap().map_or_else(Vec::new, |d| {
        d["api_keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap().to_string())
            .collect()
    })
}

/// Save `entry` as a well-behaved client: read the version first, add on top of the
/// current document, retry on a conflict. Returns once the save is acknowledged.
fn add_entry(c: &rusqlite::Connection, e: Value) {
    for _ in 0..200 {
        let (base, d) = begin_row_patch(c, None).unwrap();
        let mut d = d;
        d["api_keys"].as_array_mut().unwrap().push(e.clone());
        match save_vault(
            c,
            d,
            SaveCtx {
                actor: None,
                expect_version: base.as_deref(),
            },
        ) {
            Ok(_) => return,
            Err(m) if m.starts_with(CONFLICT_ERR) => continue,
            Err(m) => panic!("save failed: {m}"),
        }
    }
    panic!("could not save {e} after 200 attempts");
}

// ── The audit chain ──────────────────────────────────────────────────────────

/// BUG: `append_audit` read the previous hash and inserted the next row as two
/// autocommit statements. Two writers (a node event beside a vault save) read the same
/// predecessor and wrote two rows that both claim it; the chain forked and could never
/// verify again.
#[test]
fn concurrent_audit_events_keep_one_linear_chain() {
    let db = Arc::new(Db::new("audit"));
    let start = Arc::new(Barrier::new(8));
    let mut hs = Vec::new();
    for t in 0..6 {
        let (db, start) = (db.clone(), start.clone());
        hs.push(thread::spawn(move || {
            let c = db.conn();
            start.wait();
            for i in 0..40 {
                record_event(&c, "chaos", &format!("t{t}-{i}"), None, None).unwrap();
            }
        }));
    }
    for t in 0..2 {
        let (db, start) = (db.clone(), start.clone());
        hs.push(thread::spawn(move || {
            let c = db.conn();
            start.wait();
            for i in 0..15 {
                add_entry(&c, entry(&format!("w{t}-{i}"), "v"));
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    db.check();
    let rows = load_audit(&db.conn()).unwrap();
    assert!(
        rows.iter().filter(|r| r.action == "chaos").count() == 240,
        "every event must be recorded exactly once"
    );
}

// ── Row patches ──────────────────────────────────────────────────────────────

/// BUG: a row patch with no expected version loaded the document and saved it with
/// no expected version, which `merge` reads as "the writer's document is everything".
/// Entries another writer committed in between were reverted (or deleted if new).
#[test]
fn a_row_patch_without_a_version_never_deletes_what_others_add() {
    let db = Arc::new(Db::new("patch"));
    add_entry(&db.conn(), entry("seed", "v"));
    let start = Arc::new(Barrier::new(2));
    let patcher = {
        let (db, start) = (db.clone(), start.clone());
        thread::spawn(move || {
            let c = db.conn();
            start.wait();
            for i in 0..60 {
                loop {
                    let (base, d) = begin_row_patch(&c, None).unwrap();
                    let patch = json!({ "put": [entry(&format!("p{i}"), "v")] });
                    let d = apply_row_patch(d, &patch).unwrap();
                    match save_vault(
                        &c,
                        d,
                        SaveCtx {
                            actor: None,
                            expect_version: base.as_deref(),
                        },
                    ) {
                        Ok(_) => break,
                        Err(m) if m.starts_with(CONFLICT_ERR) => continue,
                        Err(m) => panic!("{m}"),
                    }
                }
            }
        })
    };
    let adder = {
        let (db, start) = (db.clone(), start.clone());
        thread::spawn(move || {
            let c = db.conn();
            start.wait();
            for i in 0..60 {
                add_entry(&c, entry(&format!("a{i}"), "v"));
            }
        })
    };
    patcher.join().unwrap();
    adder.join().unwrap();
    let have: std::collections::HashSet<_> = ids(&db.conn()).into_iter().collect();
    for i in 0..60 {
        assert!(
            have.contains(&format!("p{i}")),
            "patched entry p{i} is gone"
        );
        assert!(
            have.contains(&format!("a{i}")),
            "added entry a{i} was deleted by a patch"
        );
    }
    db.check();
}

// ── Version history ──────────────────────────────────────────────────────────

fn history_values(c: &rusqlite::Connection, id: &str) -> Vec<String> {
    vault_core::storage::entry_history(c, id)
        .unwrap()
        .map(|h| {
            h.as_array()
                .unwrap()
                .iter()
                .map(|r| r["value"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// BUG: a writer's copy of an entry carries the history it read. Saving it back when the
/// row had not changed (a retried PUT, two clients arriving at the same value) replaced
/// the stored history with that stale copy, or deleted it when the copy had none. The
/// record of the old secret was lost with no conflict.
#[test]
fn a_stale_copy_of_an_unchanged_entry_cannot_erase_its_history() {
    let db = Db::new("history");
    let a = db.conn();
    let b = db.conn();
    save_vault(&a, doc(vec![entry("e1", "K0")]), SaveCtx::default()).unwrap();
    // Both clients read the vault at this version.
    let (base, stale) = begin_row_patch(&b, None).unwrap();
    // A rotates the key: K0 goes into the history.
    save_vault(&a, doc(vec![entry("e1", "K1")]), SaveCtx::default()).unwrap();
    assert_eq!(history_values(&a, "e1"), vec!["K0"]);
    // B arrives at the same value by itself and saves its older copy (no history).
    let mut mine = stale;
    mine["api_keys"][0]["api_key"] = json!("K1");
    let r = save_vault(
        &b,
        mine,
        SaveCtx {
            actor: None,
            expect_version: base.as_deref(),
        },
    );
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(
        history_values(&a, "e1"),
        vec!["K0"],
        "the old secret was erased by a stale copy of the same entry"
    );
    db.check();
}

/// Same bug, the whole-document path: a writer whose copy has an OLDER history for a
/// row that is unchanged must not shrink the stored one.
#[test]
fn a_whole_document_save_never_shortens_history() {
    let db = Db::new("history2");
    let c = db.conn();
    save_vault(&c, doc(vec![entry("e1", "K0")]), SaveCtx::default()).unwrap();
    for k in ["K1", "K2", "K3"] {
        save_vault(&c, doc(vec![entry("e1", k)]), SaveCtx::default()).unwrap();
    }
    assert_eq!(history_values(&c, "e1"), vec!["K2", "K1", "K0"]);
    // A client that holds the entry with one record only saves it back unchanged.
    let mut d = load_vault(&c).unwrap().unwrap();
    let h = d["api_keys"][0]["version_history"]
        .as_array()
        .unwrap()
        .clone();
    d["api_keys"][0]["version_history"] = json!([h[0].clone()]);
    save_vault(&c, d, SaveCtx::default()).unwrap();
    assert_eq!(history_values(&c, "e1"), vec!["K2", "K1", "K0"]);
    db.check();
}

// ── Reads beside writes ──────────────────────────────────────────────────────

/// BUG: a load was several statements (rows, then history) and the integrity check
/// several more, each its own snapshot. A save landing between them gave a document
/// whose history belonged to another version than its rows, and reported tampering on
/// a healthy vault. Both now read one snapshot.
#[test]
fn readers_beside_a_rotating_writer_see_one_consistent_vault() {
    let db = Arc::new(Db::new("snapshot"));
    save_vault(&db.conn(), doc(vec![entry("e1", "k0")]), SaveCtx::default()).unwrap();
    let start = Arc::new(Barrier::new(2));
    let writer = {
        let (db, start) = (db.clone(), start.clone());
        thread::spawn(move || {
            let c = db.conn();
            start.wait();
            for i in 1..=120 {
                save_vault(
                    &c,
                    doc(vec![entry("e1", &format!("k{i}"))]),
                    SaveCtx::default(),
                )
                .unwrap();
            }
        })
    };
    let c = db.conn();
    start.wait();
    for _ in 0..400 {
        assert!(
            verify_vault_integrity(&c).unwrap(),
            "false tampering report"
        );
        let d = load_vault(&c).unwrap().unwrap();
        let e = &d["api_keys"][0];
        let n: usize = e["api_key"].as_str().unwrap()[1..].parse().unwrap();
        let recorded = e["version_history"].as_array().map_or(0, Vec::len);
        assert_eq!(
            recorded,
            n.min(50),
            "rows at k{n} but {recorded} history records: rows and history are from different versions"
        );
    }
    writer.join().unwrap();
    db.check();
}

// ── The unique-ID registry's secrets ─────────────────────────────────────────

/// BUG: the registry key and pepper were read, generated and `INSERT OR REPLACE`d. Two
/// first callers minted two keys; the second replaced the first, so a registry file
/// already created under the first key became unreadable, and a changed pepper made
/// every issued id look unissued.
#[test]
fn concurrent_first_use_of_the_uid_registry_agrees_on_one_key_and_pepper() {
    // Repeated on fresh databases: the window is one statement wide.
    for round in 0..30 {
        let db = Arc::new(Db::new(&format!("uid{round}")));
        let start = Arc::new(Barrier::new(8));
        let hs: Vec<_> = (0..8)
            .map(|_| {
                let (db, start) = (db.clone(), start.clone());
                thread::spawn(move || {
                    let c = db.conn();
                    start.wait();
                    vault_core::uid_registry::ensure_registry_secrets(&c).unwrap()
                })
            })
            .collect();
        let all: Vec<_> = hs.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(
            all.windows(2).all(|w| w[0] == w[1]),
            "callers got different secrets"
        );
        assert_eq!(
            vault_core::uid_registry::ensure_registry_secrets(&db.conn()).unwrap(),
            all[0]
        );
    }
}

// ── The pool state file ──────────────────────────────────────────────────────

/// BUG: pools.json was read, changed and written with no lock and no atomic rename.
/// Concurrent runners lost each other's cursor moves, use counts and cooldowns, and a
/// reader that caught a half-written file parsed it as empty, which the next save wrote
/// out, erasing every cursor on the machine.
#[test]
fn concurrent_pool_runners_lose_no_update() {
    let dir = std::env::temp_dir().join(format!("unv-chaos-pool-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("pools.json");
    // Only this test touches the variable, before any thread starts.
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var("UNV_POOL_FILE", &file);
    }
    let start = Arc::new(Barrier::new(8));
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let start = start.clone();
            thread::spawn(move || {
                start.wait();
                for _ in 0..40 {
                    let (_, saved) = vault_core::pool::update(|st| {
                        let n = st["counter"].as_u64().unwrap_or(0);
                        st["counter"] = json!(n + 1);
                    })
                    .unwrap();
                    assert!(saved.is_none(), "{saved:?}");
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    let st = vault_core::pool::load();
    assert_eq!(st["counter"].as_u64(), Some(320), "updates were lost");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_version_token_survives_unrelated_connections() {
    // Control for the helpers above: a vault written by one connection reads the same
    // version from another.
    let db = Db::new("control");
    save_vault(&db.conn(), doc(vec![entry("e1", "k")]), SaveCtx::default()).unwrap();
    assert_eq!(
        vault_version(&db.conn()).unwrap(),
        vault_version(&db.conn()).unwrap()
    );
    db.check();
}

// ── Second factor ────────────────────────────────────────────────────────────

/// BUG: the replay mark was read, compared and then written unconditionally. Several
/// simultaneous logins with one code each read the old mark, each verified, each wrote it
/// and each was let in; and a slower attempt could move the mark backwards.
#[test]
fn one_totp_code_admits_at_most_one_of_many_simultaneous_logins() {
    use vault_core::users::{
        create_user, init_users_schema, totp_confirm, totp_enroll, verify_user_totp,
    };
    for round in 0..12 {
        let db = Arc::new(Db::new(&format!("totp{round}")));
        let id = {
            let c = db.conn();
            init_users_schema(&c).unwrap();
            let u = create_user(&c, "alice", Some("hunter2hunter2"), false).unwrap();
            let secret = totp_enroll(&c, &u.id, "UnENVerse").unwrap().secret.unwrap();
            let now = vault_core::totp::now_unix();
            let code = vault_core::totp::totp_at(&secret, now).unwrap();
            assert!(totp_confirm(&c, &u.id, &code).unwrap());
            (u.id, secret)
        };
        // A code for the next step: valid now (inside the window) and not yet used.
        let code = vault_core::totp::totp_at(&id.1, vault_core::totp::now_unix() + 30).unwrap();
        let start = Arc::new(Barrier::new(8));
        let hs: Vec<_> = (0..8)
            .map(|_| {
                let (db, start, uid, code) =
                    (db.clone(), start.clone(), id.0.clone(), code.clone());
                thread::spawn(move || {
                    let c = db.conn();
                    start.wait();
                    verify_user_totp(&c, &uid, &code).unwrap_or(false)
                })
            })
            .collect();
        let admitted = hs
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|ok| *ok)
            .count();
        assert!(
            admitted <= 1,
            "round {round}: {admitted} logins were admitted with one code"
        );
    }
}

/// BUG: a client read the document and then its version as two statements. A save landing
/// between them gave a document with a NEWER version; the client's next save was judged
/// against a base that already contained the other writer's rows, which its copy lacked,
/// so they looked like deletions it had made. `snapshot_vault` reads both at once.
#[test]
fn a_document_and_its_version_always_describe_the_same_save() {
    let db = Arc::new(Db::new("pair"));
    save_vault(&db.conn(), doc(vec![entry("e0", "v")]), SaveCtx::default()).unwrap();
    let start = Arc::new(Barrier::new(2));
    let writer = {
        let (db, start) = (db.clone(), start.clone());
        thread::spawn(move || {
            let c = db.conn();
            start.wait();
            for i in 1..=150 {
                let mut d = load_vault(&c).unwrap().unwrap();
                d["api_keys"]
                    .as_array_mut()
                    .unwrap()
                    .push(entry(&format!("e{i}"), "v"));
                save_vault(&c, d, SaveCtx::default()).unwrap();
            }
        })
    };
    let c = db.conn();
    start.wait();
    for _ in 0..400 {
        let (ver, d) = vault_core::snapshot_vault(&c).unwrap();
        let seq: usize = ver.unwrap().split('.').next().unwrap().parse().unwrap();
        // Every save adds exactly one entry, so the save number is the entry count.
        assert_eq!(
            d.unwrap()["api_keys"].as_array().unwrap().len(),
            seq,
            "the document is not the one the version describes"
        );
    }
    writer.join().unwrap();
}

// ── References across writers ────────────────────────────────────────────────

fn doc_with(projects: Vec<Value>, entries: Vec<Value>) -> Value {
    json!({ "api_keys": entries, "projects": projects, "user_categories": [] })
}

fn project(id: &str) -> Value {
    json!({ "id": id, "name": id, "chunks": [] })
}

/// BUG: a project one writer deleted and an entry another writer added to it merged cleanly
/// row by row, leaving the entry pointing at a project that no longer exists. The cascade
/// that remaps `projectIds` only ever sees the writer's own document.
#[test]
fn an_entry_added_to_a_project_another_writer_deleted_does_not_dangle() {
    let db = Db::new("dangling");
    let a = db.conn();
    let b = db.conn();
    let mut e = entry("e1", "k");
    e["projectIds"] = json!(["Universal", "p1"]);
    save_vault(
        &a,
        doc_with(vec![project("Universal"), project("p1")], vec![e]),
        SaveCtx::default(),
    )
    .unwrap();
    let (base, stale) = begin_row_patch(&b, None).unwrap();
    // A deletes the project and moves its entry out (the client-side cascade).
    let mut after = load_vault(&a).unwrap().unwrap();
    after["projects"] = json!([project("Universal")]);
    after["api_keys"][0]["projectIds"] = json!(["Universal"]);
    save_vault(&a, after, SaveCtx::default()).unwrap();
    // B, from its older copy, adds a new entry to the deleted project.
    let mut mine = stale;
    let mut n = entry("n1", "k");
    n["projectIds"] = json!(["Universal", "p1"]);
    mine["api_keys"].as_array_mut().unwrap().push(n);
    save_vault(
        &b,
        mine,
        SaveCtx {
            actor: None,
            expect_version: base.as_deref(),
        },
    )
    .unwrap();
    let d = load_vault(&a).unwrap().unwrap();
    for ent in d["api_keys"].as_array().unwrap() {
        for pid in ent["projectIds"].as_array().unwrap() {
            let pid = pid.as_str().unwrap();
            assert!(
                pid == "Universal"
                    || d["projects"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| p["id"] == pid),
                "entry {} points at the deleted project {pid}",
                ent["id"]
            );
        }
    }
    assert!(
        ids(&a).contains(&"n1".to_string()),
        "the new entry must survive"
    );
    db.check();
}

/// The same for a bundle: a member attached to a bundle another writer dissolved becomes an
/// ordinary entry instead of keeping a `bundle_id` that points at nothing.
#[test]
fn a_member_attached_to_a_dissolved_bundle_becomes_an_ordinary_entry() {
    let db = Db::new("bundle");
    let a = db.conn();
    let b = db.conn();
    let mut bundle = entry("b1", "");
    bundle["secretType"] = json!("bundle");
    let mut m1 = entry("m1", "k");
    m1["bundle_id"] = json!("b1");
    m1["bundle_slot"] = json!("v3");
    save_vault(
        &a,
        doc_with(vec![project("Universal")], vec![bundle, m1]),
        SaveCtx::default(),
    )
    .unwrap();
    let (base, stale) = begin_row_patch(&b, None).unwrap();
    // A dissolves the bundle: the bundle goes, its member is released.
    let mut after = load_vault(&a).unwrap().unwrap();
    after["api_keys"] = json!([{ "id": "m1", "provider": "P-m1", "api_key": "k" }]);
    save_vault(&a, after, SaveCtx::default()).unwrap();
    // B attaches a new member to the bundle it still sees.
    let mut mine = stale;
    let mut m2 = entry("m2", "k2");
    m2["bundle_id"] = json!("b1");
    m2["bundle_slot"] = json!("v4");
    mine["api_keys"].as_array_mut().unwrap().push(m2);
    save_vault(
        &b,
        mine,
        SaveCtx {
            actor: None,
            expect_version: base.as_deref(),
        },
    )
    .unwrap();
    let d = load_vault(&a).unwrap().unwrap();
    let m2 = d["api_keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "m2")
        .expect("m2 must survive");
    assert!(
        m2.get("bundle_id").is_none(),
        "m2 still points at the dissolved bundle: {m2}"
    );
    assert!(m2.get("bundle_slot").is_none());
    db.check();
}
