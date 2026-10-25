// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Which vault this is, and which copy of it is allowed to act.
//!
//! A vault has one `vault_id` for its whole life. Each copy that was ever opened has an
//! `instance_id`, an `instance_epoch` that only grows, and a role: `active` or `standby`. A restore
//! from a recovery package, or a copy of the files made by a sync folder, would otherwise be a
//! second vault that believes it is the only one, with the same hub identity, pushing to the same
//! nodes. The rules:
//!
//! - A restored vault is `standby` with a new `instance_id` (the `vault.restored` marker beside the
//!   database, written by `unv recover restore`, is read the next time the database is opened). A
//!   standby copy reads, verifies and exports; the hub does not push and the operations executor
//!   does not run.
//! - [`activate`] makes a copy `active` and raises the epoch above anything this copy has seen. The
//!   person doing it is saying the old instance is retired.
//! - The hub tells nodes its `(vault_id, epoch)`; a node remembers the highest epoch and refuses a
//!   lower one, so a forgotten second hub cannot push over the live one.
//! - [`sync_folder`] names a cloud-sync folder a live vault sits in. Two machines opening one
//!   synced file is the way a vault gets corrupted or forked without anybody deciding to.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The file `unv recover restore` leaves beside a restored vault. Its presence means "the next open
/// is a new instance in standby".
pub const RESTORED_MARKER: &str = "vault.restored";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Active,
    Standby,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Standby => "standby",
        }
    }
    fn parse(text: &str) -> Self {
        // Anything unreadable is treated as standby: the safe side of the two.
        if text == "active" {
            Self::Active
        } else {
            Self::Standby
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub vault_id: String,
    pub instance_id: String,
    pub epoch: u64,
    pub role: Role,
}

fn get(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM vault_meta WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .optional()
    .map_err(|e| e.to_string())
}

fn put(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?1, ?2)",
        [key, value],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Reads the identity, creating it the first time: a new `vault_id`, a new `instance_id`, epoch 1,
/// active. An existing vault gets one the first time it is opened by a build that knows about this.
pub fn ensure(conn: &Connection) -> Result<Identity, String> {
    let vault_id = match get(conn, "vault_id")? {
        Some(v) => v,
        None => {
            let v = crate::new_uuid();
            put(conn, "vault_id", &v)?;
            v
        }
    };
    let instance_id = match get(conn, "instance_id")? {
        Some(v) => v,
        None => {
            let v = crate::new_uuid();
            put(conn, "instance_id", &v)?;
            v
        }
    };
    let epoch = match get(conn, "instance_epoch")?.and_then(|v| v.parse::<u64>().ok()) {
        Some(e) => e,
        None => {
            put(conn, "instance_epoch", "1")?;
            1
        }
    };
    let role = match get(conn, "instance_role")? {
        Some(r) => Role::parse(&r),
        None => {
            put(conn, "instance_role", Role::Active.as_str())?;
            Role::Active
        }
    };
    Ok(Identity {
        vault_id,
        instance_id,
        epoch,
        role,
    })
}

/// Makes this copy the active one and raises its epoch by one.
pub fn activate(conn: &Connection) -> Result<Identity, String> {
    let current = ensure(conn)?;
    let epoch = current.epoch + 1;
    put(conn, "instance_epoch", &epoch.to_string())?;
    put(conn, "instance_role", Role::Active.as_str())?;
    Ok(Identity {
        epoch,
        role: Role::Active,
        ..current
    })
}

/// Makes this copy a new instance in standby: what a restore does. The `vault_id` is kept (it is
/// still that vault); the instance and its epoch are new.
pub fn become_standby(conn: &Connection) -> Result<Identity, String> {
    let current = ensure(conn)?;
    let instance_id = crate::new_uuid();
    put(conn, "instance_id", &instance_id)?;
    put(conn, "instance_role", Role::Standby.as_str())?;
    Ok(Identity {
        instance_id,
        role: Role::Standby,
        ..current
    })
}

/// Called when a database is opened: a restore marker beside it turns this copy into a standby
/// instance, once. The marker is removed only after the change is stored.
pub fn on_open(conn: &Connection, db_path: &Path) -> Result<(), String> {
    let marker = db_path.with_file_name(RESTORED_MARKER);
    if marker.exists() {
        become_standby(conn)?;
        std::fs::remove_file(&marker).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Refuses an action that only the active instance may take.
pub fn require_active(conn: &Connection, what: &str) -> Result<(), String> {
    let id = ensure(conn)?;
    match id.role {
        Role::Active => Ok(()),
        Role::Standby => Err(format!(
            "This vault is a standby copy, so it will not {what}. Once the other instance is retired, run: unv vault activate"
        )),
    }
}

/// Cloud-sync clients a vault should not live inside.
const SYNC_NAMES: &[(&str, &str)] = &[
    ("dropbox", "Dropbox"),
    ("onedrive", "OneDrive"),
    ("google drive", "Google Drive"),
    ("googledrive", "Google Drive"),
    ("my drive", "Google Drive"),
    ("icloud drive", "iCloud Drive"),
    ("iclouddrive", "iCloud Drive"),
    ("com~apple~clouddocs", "iCloud Drive"),
    ("nextcloud", "Nextcloud"),
    ("owncloud", "ownCloud"),
    ("syncthing", "Syncthing"),
    ("pcloud", "pCloud"),
    ("megasync", "MEGA"),
    ("box sync", "Box"),
    ("sync.com", "Sync.com"),
];

/// The sync client whose folder `path` is inside, or `None`. `marker_exists` answers whether a
/// directory holds a marker file (`.stfolder` for Syncthing); it is a parameter so the check can be
/// tested without touching the disk.
pub fn sync_folder(
    path: &Path,
    marker_exists: impl Fn(&Path, &str) -> bool,
) -> Option<&'static str> {
    for ancestor in path.ancestors() {
        if let Some(name) = ancestor
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
        {
            for (needle, label) in SYNC_NAMES {
                // `OneDrive - Contoso` and `Dropbox (Personal)` are real folder names.
                if name == *needle || name.starts_with(&format!("{needle} ")) {
                    return Some(label);
                }
            }
        }
        if marker_exists(ancestor, ".stfolder") {
            return Some("Syncthing");
        }
    }
    None
}

/// [`sync_folder`] against the real file system.
pub fn sync_folder_on_disk(path: &Path) -> Option<&'static str> {
    sync_folder(path, |dir, marker| dir.join(marker).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE vault_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        c
    }

    #[test]
    fn a_vault_gets_one_identity_and_keeps_it() {
        let c = conn();
        let a = ensure(&c).unwrap();
        assert_eq!((a.epoch, a.role), (1, Role::Active));
        assert_eq!(ensure(&c).unwrap(), a, "reading never changes it");
        assert!(require_active(&c, "push").is_ok());
    }

    #[test]
    fn a_restore_makes_a_new_standby_instance_of_the_same_vault_and_it_refuses_to_act() {
        let c = conn();
        let before = ensure(&c).unwrap();
        let after = become_standby(&c).unwrap();
        assert_eq!(after.vault_id, before.vault_id);
        assert_ne!(after.instance_id, before.instance_id);
        assert_eq!(after.role, Role::Standby);
        let err = require_active(&c, "push to nodes").unwrap_err();
        assert!(
            err.contains("standby")
                && err.contains("push to nodes")
                && err.contains("unv vault activate"),
            "{err}"
        );
    }

    #[test]
    fn activating_raises_the_epoch_and_makes_the_copy_active_again() {
        let c = conn();
        ensure(&c).unwrap();
        become_standby(&c).unwrap();
        let a = activate(&c).unwrap();
        assert_eq!((a.epoch, a.role), (2, Role::Active));
        assert_eq!(
            activate(&c).unwrap().epoch,
            3,
            "each activation outranks the last"
        );
        assert!(require_active(&c, "push").is_ok());
    }

    #[test]
    fn a_damaged_role_or_epoch_is_read_on_the_safe_side() {
        let c = conn();
        ensure(&c).unwrap();
        put(&c, "instance_role", "garbage").unwrap();
        assert_eq!(ensure(&c).unwrap().role, Role::Standby);
        put(&c, "instance_epoch", "not a number").unwrap();
        assert_eq!(ensure(&c).unwrap().epoch, 1);
    }

    #[test]
    fn the_restore_marker_turns_the_next_open_into_standby_once() {
        let dir = std::env::temp_dir().join(format!("vc-id-{}", crate::new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("vault.db");
        let c = conn();
        ensure(&c).unwrap();
        on_open(&c, &db).unwrap();
        assert_eq!(
            ensure(&c).unwrap().role,
            Role::Active,
            "no marker, no change"
        );
        std::fs::write(dir.join(RESTORED_MARKER), "").unwrap();
        on_open(&c, &db).unwrap();
        assert_eq!(ensure(&c).unwrap().role, Role::Standby);
        assert!(!dir.join(RESTORED_MARKER).exists(), "the marker is spent");
        activate(&c).unwrap();
        on_open(&c, &db).unwrap();
        assert_eq!(
            ensure(&c).unwrap().role,
            Role::Active,
            "a second open does not demote it again"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_clients_are_recognised_by_folder_name_on_every_platform() {
        let none = |_: &Path, _: &str| false;
        let cases = [
            ("/home/me/Dropbox/vault/vault.db", "Dropbox"),
            ("C:\\Users\\me\\OneDrive - Contoso\\vault.db", "OneDrive"),
            (
                "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/v/vault.db",
                "iCloud Drive",
            ),
            ("/Users/me/Google Drive/My Drive/v.db", "Google Drive"),
            ("/home/me/Nextcloud/v.db", "Nextcloud"),
            ("/home/me/ownCloud/v.db", "ownCloud"),
            ("/home/me/pCloudDrive/pcloud/v.db", "pCloud"),
        ];
        for (path, label) in cases {
            // Windows-style paths use backslashes; split them the way the platform would.
            let p: std::path::PathBuf = path.split(['/', '\\']).collect();
            assert_eq!(sync_folder(&p, none), Some(label), "{path}");
        }
        assert_eq!(
            sync_folder(
                Path::new("/home/me/.local/share/io.unenverse/vault.db"),
                none
            ),
            None
        );
        assert_eq!(
            sync_folder(Path::new("/home/me/dropbox-notes-archive-old/v.db"), none),
            None
        );
    }

    #[test]
    fn a_syncthing_marker_in_any_parent_counts() {
        let has = |dir: &Path, m: &str| dir == Path::new("/data/shared") && m == ".stfolder";
        assert_eq!(
            sync_folder(Path::new("/data/shared/deep/vault.db"), has),
            Some("Syncthing")
        );
        assert_eq!(sync_folder(Path::new("/data/private/vault.db"), has), None);
    }

    #[test]
    fn the_disk_check_finds_a_real_marker() {
        let dir = std::env::temp_dir().join(format!("vc-st-{}", crate::new_uuid()));
        std::fs::create_dir_all(dir.join(".stfolder")).unwrap();
        assert_eq!(
            sync_folder_on_disk(&dir.join("vault.db")),
            Some("Syncthing")
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            sync_folder_on_disk(&std::env::temp_dir().join("nothing-here")),
            None
        );
    }
}
