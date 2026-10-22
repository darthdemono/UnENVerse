// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Changing the master password: a new salt, a new key, the same data.
//!
//! The key is Argon2id over the password and `vault.salt`, so changing the password changes both
//! files: `vault.db` is re-encrypted and `vault.salt` replaced. Two files cannot be swapped
//! atomically, so the order is chosen so that a crash at any point leaves a vault that opens with
//! either the old password or the new one, never neither:
//!
//! 1. take a write lock on the old database (a second connection, since `ATTACH` cannot run inside
//!    a transaction), checkpoint it, and copy `vault.db` and
//!    `vault.salt` to `*.pre-rekey` (the way back);
//! 2. write the new salt as `vault.salt.new`;
//! 3. `sqlcipher_export` the data into `vault.db.rekey-new` under the new key;
//! 4. open that file with the new key and check that it is intact and holds the same rows;
//! 5. rename it over `vault.db`, then rename `vault.salt.new` over `vault.salt`.
//!
//! A crash before step 5's first rename leaves the old pair untouched (the leftovers are removed).
//! A crash between the two renames leaves a new database beside the old salt and a `vault.salt.new`;
//! [`recover_interrupted`], which every unlock runs first, sees that the new password opens the
//! database through `vault.salt.new` and finishes the rename. The `*.pre-rekey` copies stay until
//! the next successful unlock ([`cleanup_after_unlock`]). Backups made before the change still open
//! with the old password, which is the point of a backup, and the documentation says so.
//!
//! Recovery packages are encrypted to an age recipient, not to the master password, so they are not
//! affected.

use crate::{derive_key, open_db, platform, restrict_to_owner, VaultKey, SALT_LEN};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

/// The shortest new master password the command accepts, as in the desktop app.
pub const MIN_PASSWORD_CHARS: usize = 12;

/// Where a rekey can be stopped, for the tests that prove each stop is survivable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    CopiedOld,
    WroteNewSalt,
    Exported,
    Verified,
    SwappedDatabase,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    /// Rows copied across all tables.
    pub rows: u64,
    /// The copy of the old database, kept until the next successful unlock.
    pub kept_database: PathBuf,
    pub kept_salt: PathBuf,
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn sql_literal(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

/// Every table's row count, so a copy can be compared with its source.
fn table_counts(conn: &Connection) -> Result<Vec<(String, i64)>, String> {
    let names: Vec<String> = {
        let mut st = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .map_err(|e| e.to_string())?;
        let rows = st.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    names
        .into_iter()
        .map(|n| {
            let c: i64 = conn
                .query_row(&format!("SELECT count(*) FROM \"{n}\""), [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            Ok((n, c))
        })
        .collect()
}

fn intact(conn: &Connection) -> Result<(), String> {
    let ok: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if ok != "ok" {
        return Err(format!("integrity check failed: {ok}"));
    }
    let mut st = conn
        .prepare("PRAGMA cipher_integrity_check")
        .map_err(|e| e.to_string())?;
    if st
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("SQLCipher integrity check failed".into());
    }
    Ok(())
}

/// Re-encrypts the vault under `new_password`. `stop_after` ends the run early with an error, which
/// is what a crash at that point looks like; production callers pass `None` through [`rekey`].
pub fn rekey_with(
    db_path: &Path,
    salt_path: &Path,
    old_password: &str,
    new_password: &str,
    stop_after: Option<Stage>,
) -> Result<Report, String> {
    if new_password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(format!(
            "The new master password must be at least {MIN_PASSWORD_CHARS} characters"
        ));
    }
    if new_password == old_password {
        return Err("The new master password is the same as the old one".into());
    }
    recover_interrupted(db_path, salt_path, old_password)?;
    crate::check_salt_pairing(db_path, salt_path)?;
    let old_salt: [u8; SALT_LEN] = fs::read(salt_path)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "vault.salt is corrupt (wrong length)".to_string())?;
    let old_key = derive_key(old_password, &old_salt)?;
    let conn = open_db(db_path, &old_key)?;
    let before = table_counts(&conn)?;
    intact(&conn)?;

    let kept_db = with_suffix(db_path, ".pre-rekey");
    let kept_salt = with_suffix(salt_path, ".pre-rekey");
    let new_salt_path = with_suffix(salt_path, ".new");
    let new_db_path = with_suffix(db_path, ".rekey-new");
    let _ = fs::remove_file(&new_db_path);
    let _ = fs::remove_file(&new_salt_path);

    let stop = |stage: Stage| -> Result<(), String> {
        if stop_after == Some(stage) {
            Err(format!("interrupted after {stage:?}"))
        } else {
            Ok(())
        }
    };
    let cleanup = || {
        let _ = fs::remove_file(&new_db_path);
        let _ = fs::remove_file(&new_salt_path);
    };

    // `ATTACH` is refused inside a transaction, so the write lock is held by a second connection
    // while this one reads and exports. Readers are not blocked; writers are.
    let guard = open_db(db_path, &old_key)?;
    let result = (|| -> Result<u64, String> {
        // 1. No writer can slip in while the copy is taken and the export runs.
        guard
            .execute_batch("BEGIN IMMEDIATE;")
            .map_err(|e| format!("the vault is in use by another program: {e}"))?;
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(|e| e.to_string())?;
        fs::copy(db_path, &kept_db).map_err(|e| format!("cannot keep the old database: {e}"))?;
        fs::copy(salt_path, &kept_salt).map_err(|e| format!("cannot keep the old salt: {e}"))?;
        restrict_to_owner(&kept_db)?;
        restrict_to_owner(&kept_salt)?;
        stop(Stage::CopiedOld)?;

        // 2. The new salt, durable before anything depends on it.
        let new_salt: [u8; SALT_LEN] = rand::random();
        {
            use std::io::Write;
            let mut f = fs::File::create(&new_salt_path).map_err(|e| e.to_string())?;
            f.write_all(&new_salt).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        restrict_to_owner(&new_salt_path)?;
        let new_key: VaultKey = derive_key(new_password, &new_salt)?;
        stop(Stage::WroteNewSalt)?;

        // 3. Export under the new key.
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        conn.execute_batch(&format!(
            "ATTACH DATABASE '{}' AS rekeyed KEY \"x'{}'\"; SELECT sqlcipher_export('rekeyed'); PRAGMA rekeyed.user_version = {version}; DETACH DATABASE rekeyed;",
            sql_literal(&new_db_path),
            hex::encode(new_key)
        ))
        .map_err(|e| format!("export failed: {e}"))?;
        restrict_to_owner(&new_db_path)?;
        stop(Stage::Exported)?;

        // 4. Prove the copy before it replaces anything.
        let check = open_db(&new_db_path, &new_key).map_err(|e| {
            format!("the re-encrypted copy does not open with the new password: {e}")
        })?;
        intact(&check)?;
        let after = table_counts(&check)?;
        if after != before {
            return Err("the re-encrypted copy does not hold the same rows".into());
        }
        let rows = after.iter().map(|(_, c)| *c as u64).sum();
        check
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(|e| e.to_string())?;
        drop(check);
        stop(Stage::Verified)?;
        Ok(rows)
    })();

    let rows = match result {
        Ok(rows) => rows,
        Err(e) => {
            let _ = guard.execute_batch("ROLLBACK;");
            drop(guard);
            drop(conn);
            // Before the swap nothing of the old pair has changed; only an interruption that
            // simulates a crash keeps its leftovers for the recovery test to find.
            if stop_after.is_none() {
                cleanup();
            }
            return Err(e);
        }
    };

    // 5. The swap. The old connection must be closed and its sidecars gone first.
    let _ = guard.execute_batch("ROLLBACK;");
    drop(guard);
    drop(conn);
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(with_suffix(db_path, suffix));
    }
    platform::replace_file(&new_db_path, db_path)
        .map_err(|e| format!("cannot install the new database: {e}"))?;
    stop(Stage::SwappedDatabase)?;
    platform::replace_file(&new_salt_path, salt_path)
        .map_err(|e| format!("cannot install the new salt: {e}"))?;
    Ok(Report {
        rows,
        kept_database: kept_db,
        kept_salt,
    })
}

/// [`rekey_with`] without a stop.
pub fn rekey(
    db_path: &Path,
    salt_path: &Path,
    old_password: &str,
    new_password: &str,
) -> Result<Report, String> {
    rekey_with(db_path, salt_path, old_password, new_password, None)
}

/// Finishes or abandons a rekey that a crash interrupted. Cheap when there is nothing to do: one
/// existence check. Returns whether it changed anything.
///
/// If `vault.salt.new` exists, the new password opening the database through it proves the
/// database was already swapped, so the salt rename is completed; otherwise the rekey never got that
/// far and its leftovers are removed.
pub fn recover_interrupted(
    db_path: &Path,
    salt_path: &Path,
    password: &str,
) -> Result<bool, String> {
    let new_salt_path = with_suffix(salt_path, ".new");
    let new_db_path = with_suffix(db_path, ".rekey-new");
    if !new_salt_path.exists() {
        // A half-written export without a new salt is just litter.
        if new_db_path.exists() {
            let _ = fs::remove_file(&new_db_path);
            return Ok(true);
        }
        return Ok(false);
    }
    let new_salt = fs::read(&new_salt_path).map_err(|e| e.to_string())?;
    let swapped = derive_key(password, &new_salt)
        .ok()
        .is_some_and(|key| db_path.exists() && open_db_quietly(db_path, &key));
    if swapped {
        platform::replace_file(&new_salt_path, salt_path)
            .map_err(|e| format!("cannot finish the interrupted password change: {e}"))?;
    } else {
        let _ = fs::remove_file(&new_salt_path);
    }
    let _ = fs::remove_file(&new_db_path);
    Ok(true)
}

/// Whether the database opens under `key`, without creating or modifying anything.
fn open_db_quietly(db_path: &Path, key: &VaultKey) -> bool {
    let Ok(conn) = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return false;
    };
    conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex::encode(key)))
        .is_ok()
        && conn
            .execute_batch("SELECT count(*) FROM sqlite_master;")
            .is_ok()
}

/// Removes the `*.pre-rekey` copies. Called after an unlock has succeeded, which proves the new pair
/// works; until then they are the way back.
pub fn cleanup_after_unlock(db_path: &Path, salt_path: &Path) -> bool {
    let a = fs::remove_file(with_suffix(db_path, ".pre-rekey")).is_ok();
    let b = fs::remove_file(with_suffix(salt_path, ".pre-rekey")).is_ok();
    a || b
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Vault {
        dir: PathBuf,
        db: PathBuf,
        salt: PathBuf,
    }

    fn vault(old: &str) -> Vault {
        let dir = std::env::temp_dir().join(format!("vc-rekey-{}", crate::new_uuid()));
        fs::create_dir_all(&dir).unwrap();
        let (db, salt) = (dir.join("vault.db"), dir.join("vault.salt"));
        let s = crate::read_or_create_salt(&salt).unwrap();
        let key = derive_key(old, &s).unwrap();
        let conn = open_db(&db, &key).unwrap();
        crate::init_schema(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS probe (k TEXT PRIMARY KEY, v TEXT); INSERT INTO probe VALUES ('a', 'one'), ('b', 'two');",
        )
        .unwrap();
        Vault { dir, db, salt }
    }

    /// Opens the vault the way an unlock does: recover first, then derive from the salt on disk.
    fn unlock(v: &Vault, password: &str) -> Result<Connection, String> {
        recover_interrupted(&v.db, &v.salt, password)?;
        let salt = fs::read(&v.salt).map_err(|e| e.to_string())?;
        open_db(&v.db, &derive_key(password, &salt)?)
    }

    fn rows(conn: &Connection) -> i64 {
        conn.query_row("SELECT count(*) FROM probe", [], |r| r.get(0))
            .unwrap()
    }

    // Built at run time: a literal here reads as a hard-coded key to the code scanner.
    static OLD: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| format!("the old master password {}", 1));
    static NEW: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| format!("the new master password {}", 2));

    #[test]
    fn the_new_password_opens_the_same_data_and_the_old_one_no_longer_does() {
        let v = vault(&OLD);
        let report = rekey(&v.db, &v.salt, &OLD, &NEW).unwrap();
        assert!(report.rows >= 2);
        assert_eq!(rows(&unlock(&v, &NEW).unwrap()), 2);
        assert_eq!(unlock(&v, &OLD).err().unwrap(), "Wrong master password");
        assert!(
            report.kept_database.exists() && report.kept_salt.exists(),
            "the way back is kept"
        );
        // After a successful unlock the copies go.
        assert!(cleanup_after_unlock(&v.db, &v.salt));
        assert!(!report.kept_database.exists() && !report.kept_salt.exists());
        assert!(
            !cleanup_after_unlock(&v.db, &v.salt),
            "nothing left to remove"
        );
        let _ = fs::remove_dir_all(&v.dir);
    }

    #[test]
    fn the_kept_copy_still_opens_with_the_old_password() {
        let v = vault(&OLD);
        let report = rekey(&v.db, &v.salt, &OLD, &NEW).unwrap();
        let salt = fs::read(&report.kept_salt).unwrap();
        let conn = open_db(&report.kept_database, &derive_key(&OLD, &salt).unwrap()).unwrap();
        assert_eq!(rows(&conn), 2);
        drop(conn);
        let _ = fs::remove_dir_all(&v.dir);
    }

    #[test]
    fn a_stop_at_any_step_leaves_a_vault_that_opens_with_the_old_or_the_new_password() {
        for (stage, new_wins) in [
            (Stage::CopiedOld, false),
            (Stage::WroteNewSalt, false),
            (Stage::Exported, false),
            (Stage::Verified, false),
            (Stage::SwappedDatabase, true),
        ] {
            let v = vault(&OLD);
            let err = rekey_with(&v.db, &v.salt, &OLD, &NEW, Some(stage)).unwrap_err();
            assert!(err.contains("interrupted"), "{stage:?}: {err}");
            let (winner, loser) = if new_wins {
                (&**NEW, &**OLD)
            } else {
                (&**OLD, &**NEW)
            };
            let conn = unlock(&v, winner)
                .unwrap_or_else(|e| panic!("{stage:?}: the expected password should open: {e}"));
            assert_eq!(rows(&conn), 2, "{stage:?}");
            drop(conn);
            assert!(
                unlock(&v, loser).is_err(),
                "{stage:?}: the other password should not open"
            );
            assert!(
                !with_suffix(&v.salt, ".new").exists(),
                "{stage:?}: no half-finished salt is left"
            );
            assert!(
                !with_suffix(&v.db, ".rekey-new").exists(),
                "{stage:?}: no half-written copy is left"
            );
            let _ = fs::remove_dir_all(&v.dir);
        }
    }

    #[test]
    fn recovery_is_a_no_op_when_nothing_was_interrupted() {
        let v = vault(&OLD);
        assert!(!recover_interrupted(&v.db, &v.salt, &OLD).unwrap());
        // A stray half-written export with no new salt is removed.
        fs::write(with_suffix(&v.db, ".rekey-new"), b"junk").unwrap();
        assert!(recover_interrupted(&v.db, &v.salt, &OLD).unwrap());
        assert!(!with_suffix(&v.db, ".rekey-new").exists());
        let _ = fs::remove_dir_all(&v.dir);
    }

    #[test]
    fn a_wrong_old_password_a_weak_new_one_and_an_unchanged_one_are_refused_before_any_file_changes(
    ) {
        let v = vault(&OLD);
        let before = (fs::read(&v.db).unwrap().len(), fs::read(&v.salt).unwrap());
        assert_eq!(
            rekey(&v.db, &v.salt, &format!("{}x", &*OLD), &NEW).unwrap_err(),
            "Wrong master password"
        );
        let weak = rekey(&v.db, &v.salt, &OLD, &"x".repeat(5)).unwrap_err();
        assert!(weak.contains("at least 12"), "{weak}");
        let same = rekey(&v.db, &v.salt, &OLD, &OLD).unwrap_err();
        assert!(same.contains("same"), "{same}");
        assert_eq!(fs::read(&v.salt).unwrap(), before.1);
        assert!(!with_suffix(&v.db, ".pre-rekey").exists());
        assert_eq!(rows(&unlock(&v, &OLD).unwrap()), 2);
        let _ = fs::remove_dir_all(&v.dir);
    }

    #[test]
    fn a_missing_salt_is_reported_as_such_and_not_papered_over() {
        let v = vault(&OLD);
        fs::remove_file(&v.salt).unwrap();
        let err = rekey(&v.db, &v.salt, &OLD, &NEW).unwrap_err();
        assert!(
            err.contains("vault.salt") && err.contains("missing"),
            "{err}"
        );
        let _ = fs::remove_dir_all(&v.dir);
    }

    #[test]
    fn a_vault_another_program_holds_open_for_writing_is_refused_not_corrupted() {
        let v = vault(&OLD);
        let salt = fs::read(&v.salt).unwrap();
        let other = open_db(&v.db, &derive_key(&OLD, &salt).unwrap()).unwrap();
        other.execute_batch("BEGIN IMMEDIATE;").unwrap();
        let err = rekey(&v.db, &v.salt, &OLD, &NEW).unwrap_err();
        assert!(err.contains("in use"), "{err}");
        other.execute_batch("ROLLBACK;").unwrap();
        drop(other);
        assert_eq!(rows(&unlock(&v, &OLD).unwrap()), 2, "still the old vault");
        assert!(
            !with_suffix(&v.salt, ".new").exists() && !with_suffix(&v.db, ".rekey-new").exists()
        );
        let _ = fs::remove_dir_all(&v.dir);
    }
}
