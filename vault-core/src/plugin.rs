// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Plugin manifests and the registry of installed plugins (protocol `unv-plugin-rpc/1`).
//!
//! A plugin is an external program in `<plugins dir>/<name>/` with a `plugin.json` manifest. It runs
//! with the user's rights, as any program the user starts would; nothing here sandboxes it. What the
//! registry does give is a record of what the user agreed to (the capabilities in the manifest and
//! the SHA-256 of the program at install time) so a replaced binary is noticed before it is called.
//! A plugin is never handed the master key, other entries or signing keys: the host passes it only
//! the parameters of one operation and the credential slots the user bound to it.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const PROTOCOL: &str = "unv-plugin-rpc/1";

/// What a plugin may be asked to do. The user sees this list when enabling it.
pub const CAPABILITIES: &[&str] = &["destination", "notifier", "issuer", "importer"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub protocol: String,
    /// The program, relative to the plugin's directory.
    pub exec: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub capabilities: Vec<String>,
    /// Named credential slots the user binds to vault entries; the plugin never sees an entry it was not given.
    #[serde(default)]
    pub slots: Vec<String>,
    #[serde(default)]
    pub description: String,
}

fn plain(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let m: Manifest = serde_json::from_str(text).map_err(|e| format!("plugin.json: {e}"))?;
        m.validate()?;
        Ok(m)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !plain(&self.name) {
            return Err(format!(
                "plugin name '{}' must be lowercase letters, digits, '-' or '_'",
                self.name
            ));
        }
        if self.protocol != PROTOCOL {
            return Err(format!(
                "plugin speaks '{}', this host speaks '{PROTOCOL}'",
                self.protocol
            ));
        }
        if self.version.trim().is_empty() {
            return Err("plugin version is empty".into());
        }
        // A rooted path ("/bin/sh", "\\x", "C:\\x") or a ".." step leaves the plugin's directory; on
        // Windows "/bin/sh" is not `is_absolute()` (no drive), so look at the components instead.
        let exec = Path::new(&self.exec);
        if self.exec.is_empty()
            || exec.is_absolute()
            || self.exec.starts_with(['/', '\\'])
            || exec.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err("plugin exec must be a path inside the plugin's own directory".into());
        }
        if self.capabilities.is_empty() {
            return Err("a plugin must declare at least one capability".into());
        }
        if let Some(bad) = self
            .capabilities
            .iter()
            .find(|c| !CAPABILITIES.contains(&c.as_str()))
        {
            return Err(format!("unknown capability '{bad}'"));
        }
        if let Some(bad) = self.slots.iter().find(|s| !plain(s)) {
            return Err(format!("slot name '{bad}' must be a plain name"));
        }
        Ok(())
    }
}

/// SHA-256 of the program file, as lowercase hex: the provenance recorded at install and re-checked
/// before each call.
pub fn provenance(program: &Path) -> Result<String, String> {
    let bytes = std::fs::read(program).map_err(|e| format!("{}: {e}", program.display()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// A plugin found on disk and not yet installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovered {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl Discovered {
    pub fn program(&self) -> PathBuf {
        self.dir.join(&self.manifest.exec)
    }
}

/// Reads `<dir>/<name>/plugin.json` for every subdirectory. A broken manifest is reported with
/// its path and does not hide the others.
pub fn discover(dir: &Path) -> (Vec<Discovered>, Vec<String>) {
    let mut found = Vec::new();
    let mut problems = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return (found, problems);
    };
    let mut dirs: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for d in dirs {
        let file = d.join("plugin.json");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        match Manifest::parse(&text) {
            Ok(manifest) => found.push(Discovered { dir: d, manifest }),
            Err(e) => problems.push(format!("{}: {e}", file.display())),
        }
    }
    (found, problems)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub name: String,
    pub version: String,
    pub hash: String,
    pub dir: String,
    pub capabilities: Vec<String>,
    pub slots: Vec<String>,
    pub enabled: bool,
    pub installed_at: String,
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS plugins (
             name TEXT PRIMARY KEY, version TEXT NOT NULL, hash TEXT NOT NULL, dir TEXT NOT NULL,
             capabilities TEXT NOT NULL, slots TEXT NOT NULL, enabled INTEGER NOT NULL,
             installed_at TEXT NOT NULL
         );",
    )
    .map_err(|e| e.to_string())
}

/// Records a plugin as installed and disabled. Installing again replaces the record (a new
/// version or hash) and disables it: the user agrees to the new capabilities by enabling it.
pub fn install(conn: &Connection, found: &Discovered) -> Result<Installed, String> {
    let m = &found.manifest;
    let hash = provenance(&found.program())?;
    let row = Installed {
        name: m.name.clone(),
        version: m.version.clone(),
        hash,
        dir: found.dir.to_string_lossy().into_owned(),
        capabilities: m.capabilities.clone(),
        slots: m.slots.clone(),
        enabled: false,
        installed_at: crate::iso_now(),
    };
    conn.execute(
        "INSERT OR REPLACE INTO plugins (name, version, hash, dir, capabilities, slots, enabled, installed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
        params![
            row.name,
            row.version,
            row.hash,
            row.dir,
            serde_json::to_string(&row.capabilities).map_err(|e| e.to_string())?,
            serde_json::to_string(&row.slots).map_err(|e| e.to_string())?,
            row.installed_at
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(row)
}

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Installed> {
    let list = |i: usize| -> rusqlite::Result<Vec<String>> {
        let text: String = r.get(i)?;
        Ok(serde_json::from_str(&text).unwrap_or_default())
    };
    Ok(Installed {
        name: r.get(0)?,
        version: r.get(1)?,
        hash: r.get(2)?,
        dir: r.get(3)?,
        capabilities: list(4)?,
        slots: list(5)?,
        enabled: r.get::<_, i64>(6)? != 0,
        installed_at: r.get(7)?,
    })
}

const COLUMNS: &str = "name, version, hash, dir, capabilities, slots, enabled, installed_at";

pub fn get(conn: &Connection, name: &str) -> Result<Option<Installed>, String> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM plugins WHERE name = ?1"),
        [name],
        from_row,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn list(conn: &Connection) -> Result<Vec<Installed>, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {COLUMNS} FROM plugins ORDER BY name"))
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], from_row).map_err(|e| e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}

pub fn set_enabled(conn: &Connection, name: &str, enabled: bool) -> Result<(), String> {
    let n = conn
        .execute(
            "UPDATE plugins SET enabled = ?2 WHERE name = ?1",
            params![name, enabled as i64],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("No plugin named {name}"));
    }
    Ok(())
}

pub fn remove(conn: &Connection, name: &str) -> Result<bool, String> {
    conn.execute("DELETE FROM plugins WHERE name = ?1", [name])
        .map(|n| n > 0)
        .map_err(|e| e.to_string())
}

/// The installed program still matches what the user installed.
pub fn check_provenance(installed: &Installed, manifest: &Manifest) -> Result<(), String> {
    let program = Path::new(&installed.dir).join(&manifest.exec);
    let now = provenance(&program)?;
    if now == installed.hash {
        Ok(())
    } else {
        Err(format!(
            "plugin '{}' changed since it was installed (was {}, is {}); install it again to accept the new program",
            installed.name,
            &installed.hash[..12.min(installed.hash.len())],
            &now[..12.min(now.len())]
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(extra: &str) -> String {
        format!(
            r#"{{"name":"ntfy","version":"1.0.0","protocol":"unv-plugin-rpc/1","exec":"run.sh","capabilities":["notifier"]{extra}}}"#
        )
    }

    #[test]
    fn a_valid_manifest_parses_and_defaults_are_empty() {
        let m = Manifest::parse(&manifest("")).unwrap();
        assert_eq!(m.name, "ntfy");
        assert!(m.slots.is_empty() && m.args.is_empty());
    }

    #[test]
    fn a_manifest_cannot_name_a_program_outside_its_directory_or_an_unknown_capability() {
        assert!(Manifest::parse(&manifest(r#","slots":["Bad Name"]"#))
            .unwrap_err()
            .contains("slot name"));
        let bad =
            |from: &str, to: &str| Manifest::parse(&manifest("").replace(from, to)).unwrap_err();
        assert!(bad("run.sh", "../run.sh").contains("inside"));
        assert!(bad("run.sh", "/bin/sh").contains("inside"));
        assert!(bad("\"ntfy\"", "\"Not Plain\"").contains("lowercase"));
        assert!(bad("rpc/1", "rpc/2").contains("speaks"));
        assert!(bad("notifier", "root").contains("unknown capability"));
        assert!(bad("[\"notifier\"]", "[]").contains("at least one"));
        assert!(bad("\"1.0.0\"", "\"  \"").contains("version is empty"));
        assert!(Manifest::parse("{").unwrap_err().contains("plugin.json"));
    }

    fn plugin_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("vc-plug-{}", crate::new_uuid()));
        std::fs::create_dir_all(d.join("ntfy")).unwrap();
        std::fs::write(d.join("ntfy/plugin.json"), manifest("")).unwrap();
        std::fs::write(d.join("ntfy/run.sh"), "#!/bin/sh\n").unwrap();
        d
    }

    #[test]
    fn discovery_reports_a_broken_manifest_without_hiding_the_others() {
        let d = plugin_dir();
        std::fs::create_dir_all(d.join("broken")).unwrap();
        std::fs::write(d.join("broken/plugin.json"), "{}").unwrap();
        std::fs::create_dir_all(d.join("no-manifest")).unwrap();
        let (found, problems) = discover(&d);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].manifest.name, "ntfy");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("broken"));
        assert_eq!(discover(&d.join("absent")), (vec![], vec![]));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn install_enable_and_a_replaced_program_is_noticed() {
        let d = plugin_dir();
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let (found, _) = discover(&d);
        let row = install(&conn, &found[0]).unwrap();
        assert!(!row.enabled, "installing never enables");
        assert_eq!(get(&conn, "ntfy").unwrap().unwrap(), row);
        set_enabled(&conn, "ntfy", true).unwrap();
        assert!(list(&conn).unwrap()[0].enabled);
        let installed = get(&conn, "ntfy").unwrap().unwrap();
        check_provenance(&installed, &found[0].manifest).unwrap();
        std::fs::write(d.join("ntfy/run.sh"), "#!/bin/sh\necho changed\n").unwrap();
        let err = check_provenance(&installed, &found[0].manifest).unwrap_err();
        assert!(err.contains("changed since it was installed"), "{err}");
        // Installing again accepts the new program and disables the plugin until it is enabled.
        let again = install(&conn, &found[0]).unwrap();
        assert_ne!(again.hash, installed.hash);
        assert!(!get(&conn, "ntfy").unwrap().unwrap().enabled);
        assert!(set_enabled(&conn, "absent", true).is_err());
        assert!(remove(&conn, "ntfy").unwrap());
        assert!(!remove(&conn, "ntfy").unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }
}
