// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Streamed age-encrypted recovery packages (`.unvpkg`, format v1).

use age::secrecy::ExposeSecret;
use age::{Decryptor, Encryptor};
use rusqlite::{backup::Backup, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8] = b"UNVPKG1\n";
const MAX_HEADER_BYTES: usize = 1024 * 1024;
pub const RECIPIENT_META_KEY: &str = "recovery_recipient_v1";
/// The age identity used only by an unlocked local executor to exercise a
/// restore drill.  `vault_meta` is SQLCipher-encrypted and this value is never
/// exposed through an output command; the user-held recovery-key copy remains
/// the recovery path for a new machine.
pub const DRILL_IDENTITY_META_KEY: &str = "recovery_drill_identity_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageFile {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub format_version: u32,
    pub app_version: String,
    pub created_at: String,
    pub vault_version: Option<String>,
    pub entry_count: usize,
    pub files: Vec<PackageFile>,
}

#[derive(Debug, Clone)]
pub struct PackageInput<'a> {
    pub conn: &'a Connection,
    pub key: &'a crate::VaultKey,
    pub db_path: &'a Path,
    pub salt_path: &'a Path,
    /// `(archive name, source path)` pairs. Callers pass only the explicitly
    /// recoverable node/hub/materialisation files; approver keys never enter it.
    pub auxiliary: Vec<(String, PathBuf)>,
    pub app_version: &'a str,
}

fn err<E: std::fmt::Display>(context: &str, error: E) -> String {
    format!("{context}: {error}")
}

fn hash_file(path: &Path) -> Result<(String, u64), String> {
    let mut file = File::open(path).map_err(|e| err("open package input", e))?;
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| err("read package input", e))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        size += n as u64;
    }
    Ok((hex::encode(hash.finalize()), size))
}

fn snapshot_path(db_path: &Path) -> PathBuf {
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(
        ".vault.snapshot-{}-{}.db",
        std::process::id(),
        crate::new_uuid()
    ))
}

fn snapshot(conn: &Connection, key: &crate::VaultKey, db_path: &Path) -> Result<PathBuf, String> {
    let path = snapshot_path(db_path);
    let mut destination = Connection::open(&path).map_err(|e| err("create snapshot", e))?;
    destination
        .execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex::encode(key)))
        .map_err(|e| err("key snapshot", e))?;
    let backup = Backup::new(conn, &mut destination).map_err(|e| err("start snapshot", e))?;
    backup
        .run_to_completion(64, std::time::Duration::from_millis(5), None)
        .map_err(|e| err("snapshot vault", e))?;
    drop(backup);
    drop(destination);
    crate::restrict_to_owner(&path)?;
    Ok(path)
}

fn append_file<W: Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    path: &Path,
) -> Result<(), String> {
    builder
        .append_path_with_name(path, name)
        .map_err(|e| err("add package file", e))
}

fn append_bytes<W: Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    builder
        .append_data(&mut header, name, bytes)
        .map_err(|e| err("add package manifest", e))
}

/// Generates a recovery identity, returning the public recipient and the
/// identity text that must be shown or saved exactly once by the caller.
pub fn generate_identity() -> (String, String) {
    let identity = age::x25519::Identity::generate();
    (
        identity.to_public().to_string(),
        identity.to_string().expose_secret().to_string(),
    )
}

pub fn set_recipient(conn: &Connection, recipient: &str) -> Result<(), String> {
    recipient
        .parse::<age::x25519::Recipient>()
        .map_err(|_| "Recovery recipient is not a valid age X25519 recipient".to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?1, ?2)",
        (RECIPIENT_META_KEY, recipient),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Stores the recovery identity inside the encrypted vault for unattended
/// restore drills.  Callers must never print this value after creation.
pub fn set_drill_identity(conn: &Connection, identity: &str) -> Result<(), String> {
    identity
        .trim()
        .parse::<age::x25519::Identity>()
        .map_err(|_| "Recovery identity is not a valid age X25519 identity".to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?1, ?2)",
        (DRILL_IDENTITY_META_KEY, identity),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn recipient(conn: &Connection) -> Result<Option<String>, String> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT value FROM vault_meta WHERE key = ?1",
        [RECIPIENT_META_KEY],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn drill_identity(conn: &Connection) -> Result<Option<String>, String> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT value FROM vault_meta WHERE key = ?1",
        [DRILL_IDENTITY_META_KEY],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Writes one package to `output` without buffering the vault or package.
pub fn write_package<W: Write>(input: PackageInput<'_>, mut output: W) -> Result<Manifest, String> {
    let recipient = recipient(input.conn)?.ok_or_else(|| {
        "No recovery recipient is configured. Create a recovery key before packaging.".to_string()
    })?;
    let recipient = recipient
        .parse::<age::x25519::Recipient>()
        .map_err(|_| "Stored recovery recipient is invalid".to_string())?;
    let snapshot = snapshot(input.conn, input.key, input.db_path)?;
    let result = (|| {
        let mut sources = vec![
            ("vault.db".to_string(), snapshot.clone()),
            ("vault.salt".to_string(), input.salt_path.to_path_buf()),
        ];
        for (name, path) in input.auxiliary {
            if name == "approver.key" || name.starts_with("approver.key/") {
                return Err(
                    "approver.key is deliberately excluded from recovery packages".to_string(),
                );
            }
            if path.exists() {
                sources.push((name, path));
            }
        }
        let mut seen = BTreeSet::new();
        let mut files = Vec::new();
        for (name, path) in &sources {
            if !seen.insert(name.clone()) || name.starts_with('/') || name.contains("..") {
                return Err(format!("Invalid recovery package file name `{name}`"));
            }
            let (sha256, size) = hash_file(path)?;
            files.push(PackageFile {
                name: name.clone(),
                sha256,
                size,
            });
        }
        let document = crate::load_vault(input.conn)?.unwrap_or_else(|| serde_json::json!({}));
        let entry_count = document["api_keys"].as_array().map_or(0, Vec::len);
        let manifest = Manifest {
            format_version: 1,
            app_version: input.app_version.to_string(),
            created_at: crate::iso_now(),
            vault_version: crate::vault_version(input.conn)?,
            entry_count,
            files,
        };
        let manifest_json = serde_json::to_vec(&manifest).map_err(|e| err("encode manifest", e))?;
        if manifest_json.len() > MAX_HEADER_BYTES {
            return Err("Recovery manifest is too large".to_string());
        }
        output
            .write_all(MAGIC)
            .map_err(|e| err("write package header", e))?;
        output
            .write_all(&(manifest_json.len() as u32).to_le_bytes())
            .map_err(|e| err("write package header", e))?;
        output
            .write_all(&manifest_json)
            .map_err(|e| err("write package header", e))?;
        let encryptor =
            Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .map_err(|e| err("create age encryptor", e))?;
        let encrypted = encryptor
            .wrap_output(output)
            .map_err(|e| err("start package encryption", e))?;
        let mut tar = tar::Builder::new(encrypted);
        append_bytes(&mut tar, "manifest.json", &manifest_json)?;
        for (name, path) in &sources {
            append_file(&mut tar, name, path)?;
        }
        let encrypted = tar.into_inner().map_err(|e| err("finish package tar", e))?;
        let _ = encrypted
            .finish()
            .map_err(|e| err("finish package encryption", e))?;
        Ok(manifest)
    })();
    let _ = fs::remove_file(&snapshot);
    result
}

/// Reads the cleartext envelope header only; the vault payload stays encrypted.
pub fn inspect_package(path: &Path) -> Result<Manifest, String> {
    let file = File::open(path).map_err(|e| err("open package", e))?;
    read_header(BufReader::new(file)).map(|(manifest, _)| manifest)
}

fn read_header<R: Read>(mut reader: R) -> Result<(Manifest, R), String> {
    let mut magic = [0_u8; MAGIC.len()];
    reader
        .read_exact(&mut magic)
        .map_err(|e| err("read package header", e))?;
    if magic != MAGIC {
        return Err("Not an UnENVerse recovery package".to_string());
    }
    let mut len = [0_u8; 4];
    reader
        .read_exact(&mut len)
        .map_err(|e| err("read package header", e))?;
    let len = u32::from_le_bytes(len) as usize;
    if len == 0 || len > MAX_HEADER_BYTES {
        return Err("Recovery package header is invalid".to_string());
    }
    let mut json = vec![0_u8; len];
    reader
        .read_exact(&mut json)
        .map_err(|e| err("read package header", e))?;
    let manifest = serde_json::from_slice(&json).map_err(|e| err("parse package manifest", e))?;
    Ok((manifest, reader))
}

fn stage_path(target: &Path) -> Result<PathBuf, String> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Restore target has no directory name")?;
    Ok(parent.join(format!(".{name}.restore-{}", crate::new_uuid())))
}

fn verify_member_name(name: &str, expected: &BTreeMap<String, PackageFile>) -> Result<(), String> {
    if name == "manifest.json" || expected.contains_key(name) {
        Ok(())
    } else {
        Err(format!("Unexpected recovery package member `{name}`"))
    }
}

fn copy_member<R: Read>(reader: &mut R, path: &Path) -> Result<(String, u64), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| err("create restore directory", e))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| err("write restored file", e))?;
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| err("read package member", e))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| err("write restored file", e))?;
        hash.update(&buf[..n]);
        size += n as u64;
    }
    file.sync_all().map_err(|e| err("sync restored file", e))?;
    crate::restrict_to_owner(path)?;
    Ok((hex::encode(hash.finalize()), size))
}

/// Decrypts and validates into a staging directory before any target file is written.
pub fn restore_package(
    package: &Path,
    target: &Path,
    identity_file: &Path,
) -> Result<Manifest, String> {
    let identity_text =
        fs::read_to_string(identity_file).map_err(|e| err("read recovery identity", e))?;
    restore_package_with_identity(package, target, &identity_text)
}

/// Decrypts a package with in-memory identity text.  This is used by the
/// unlocked monthly drill so it never materializes the identity in a temporary
/// file.  Human recovery still uses [`restore_package`] and a user-held key.
pub fn restore_package_with_identity(
    package: &Path,
    target: &Path,
    identity_text: &str,
) -> Result<Manifest, String> {
    if target.exists()
        && fs::read_dir(target)
            .map_err(|e| err("read restore target", e))?
            .next()
            .is_some()
    {
        return Err("Restore target must be an empty directory".to_string());
    }
    let identity = identity_text
        .trim()
        .parse::<age::x25519::Identity>()
        .map_err(|_| "Recovery identity is not a valid age X25519 identity".to_string())?;
    let file = File::open(package).map_err(|e| err("open package", e))?;
    let (manifest, reader) = read_header(BufReader::new(file))?;
    if manifest.format_version != 1 {
        return Err(format!(
            "Unsupported recovery package version {}",
            manifest.format_version
        ));
    }
    let decryptor = Decryptor::new_buffered(reader)
        .map_err(|_| "Recovery package encryption header is invalid".to_string())?;
    let plaintext = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|_| "Recovery package decryption failed".to_string())?;
    let stage = stage_path(target)?;
    fs::create_dir(&stage).map_err(|e| err("create restore staging directory", e))?;
    let result = (|| {
        let expected = manifest
            .files
            .iter()
            .cloned()
            .map(|f| (f.name.clone(), f))
            .collect::<BTreeMap<_, _>>();
        let mut seen = BTreeSet::new();
        let mut tar = tar::Archive::new(plaintext);
        for entry in tar.entries().map_err(|e| err("read package tar", e))? {
            let mut entry = entry.map_err(|e| err("read package tar", e))?;
            if !entry.header().entry_type().is_file() {
                return Err("Recovery package contains a non-file member".to_string());
            }
            let name = entry
                .path()
                .map_err(|e| err("read package member name", e))?
                .to_string_lossy()
                .to_string();
            verify_member_name(&name, &expected)?;
            if !seen.insert(name.clone()) {
                return Err(format!("Recovery package repeats `{name}`"));
            }
            if name == "manifest.json" {
                let mut bytes = Vec::new();
                entry
                    .by_ref()
                    .take(MAX_HEADER_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| err("read package manifest", e))?;
                if bytes.len() > MAX_HEADER_BYTES
                    || serde_json::from_slice::<Manifest>(&bytes).ok().as_ref() != Some(&manifest)
                {
                    return Err("Encrypted recovery manifest does not match its header".to_string());
                }
                fs::write(stage.join("manifest.json"), bytes)
                    .map_err(|e| err("write restored manifest", e))?;
                continue;
            }
            let spec = expected
                .get(&name)
                .ok_or_else(|| "Package member was not declared".to_string())?;
            let (sha256, size) = copy_member(&mut entry, &stage.join(&name))?;
            if sha256 != spec.sha256 || size != spec.size {
                return Err(format!(
                    "Recovery package member `{name}` failed verification"
                ));
            }
        }
        if !seen.contains("manifest.json") || expected.keys().any(|name| !seen.contains(name)) {
            return Err("Recovery package is incomplete".to_string());
        }
        // Drain the age reader so its authenticated final chunk is checked,
        // including bytes after the tar end markers.
        let mut plaintext = tar.into_inner();
        std::io::copy(&mut plaintext, &mut std::io::sink())
            .map_err(|e| err("verify package encryption", e))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
        return result.map(|_| manifest);
    }
    if target.exists() {
        for entry in fs::read_dir(&stage).map_err(|e| err("read restore staging directory", e))? {
            let entry = entry.map_err(|e| err("read restore staging directory", e))?;
            fs::rename(entry.path(), target.join(entry.file_name()))
                .map_err(|e| err("install restored file", e))?;
        }
        fs::remove_dir(&stage).map_err(|e| err("remove restore staging directory", e))?;
    } else {
        fs::rename(&stage, target).map_err(|e| err("install restored vault", e))?;
    }
    // The restored vault must not believe it is the one that was backed up: its first open makes
    // it a new, standby instance (see `identity`).
    fs::write(target.join(crate::identity::RESTORED_MARKER), b"")
        .map_err(|e| err("mark the restored vault as a new instance", e))?;
    Ok(manifest)
}

/// Validates every local invariant of a restored package directory.
pub fn verify_restored(target: &Path, password: &str) -> Result<Manifest, String> {
    let salt = fs::read(target.join("vault.salt")).map_err(|e| err("read restored salt", e))?;
    let key = crate::derive_key(password, &salt)?;
    verify_restored_with_key(target, &key)
}

/// Equivalent to [`verify_restored`], but reuses an already-derived key held by
/// the unlocked executor.  Restore drills therefore do not prompt for, retain
/// or re-derive the vault password.
pub fn verify_restored_with_key(target: &Path, key: &crate::VaultKey) -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(target.join("manifest.json")).map_err(|e| err("read restored manifest", e))?,
    )
    .map_err(|e| err("parse restored manifest", e))?;
    for spec in &manifest.files {
        let (sha256, size) = hash_file(&target.join(&spec.name))?;
        if sha256 != spec.sha256 || size != spec.size {
            return Err(format!("Restored file `{}` failed verification", spec.name));
        }
    }
    let conn = crate::open_db(&target.join("vault.db"), key)?;
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if integrity != "ok" {
        return Err(format!("SQLite integrity_check failed: {integrity}"));
    }
    let mut stmt = conn
        .prepare("PRAGMA cipher_integrity_check")
        .map_err(|e| e.to_string())?;
    if stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("SQLCipher integrity check failed".to_string());
    }
    if !crate::storage::verify(&conn)? || !crate::verify_audit(&conn)? {
        return Err("Vault storage or audit verification failed".to_string());
    }
    if !crate::config_history::verify(&conn)?.problems.is_empty() {
        return Err("Config history verification failed".to_string());
    }
    let entries = crate::load_vault(&conn)?
        .and_then(|v| v["api_keys"].as_array().map(Vec::len))
        .unwrap_or(0);
    if entries != manifest.entry_count || crate::vault_version(&conn)? != manifest.vault_version {
        return Err("Restored vault count or version does not match the manifest".to_string());
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("unenverse-recovery-{name}-{}", crate::new_uuid()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn package_fixture(dir: &Path) -> (PathBuf, String, PathBuf) {
        let db = dir.join("vault.db");
        let salt = dir.join("vault.salt");
        fs::write(&salt, b"0123456789abcdef").unwrap();
        let password = "recovery-test-password".to_string();
        let key = crate::derive_key(&password, &fs::read(&salt).unwrap()).unwrap();
        let conn = crate::open_db(&db, &key).unwrap();
        crate::init_schema(&conn).unwrap();
        crate::save_vault(&conn, serde_json::json!({ "api_keys": [{ "id": "entry-1", "provider": "fixture", "api_key": "not-a-real-secret" }] }), crate::SaveCtx::default()).unwrap();
        let (recipient, identity) = generate_identity();
        set_recipient(&conn, &recipient).unwrap();
        let package = dir.join("fixture.unvpkg");
        let output = File::create(&package).unwrap();
        write_package(
            PackageInput {
                conn: &conn,
                key: &key,
                db_path: &db,
                salt_path: &salt,
                auxiliary: Vec::new(),
                app_version: "test",
            },
            output,
        )
        .unwrap();
        let identity_file = dir.join("identity.txt");
        fs::write(&identity_file, identity).unwrap();
        (package, password, identity_file)
    }

    #[test]
    fn an_identity_round_trips_to_its_recipient() {
        let (recipient, identity) = generate_identity();
        let identity = identity.parse::<age::x25519::Identity>().unwrap();
        assert_eq!(identity.to_public().to_string(), recipient);
    }

    #[test]
    fn package_restores_only_after_full_validation() {
        let dir = scratch("roundtrip");
        let (package, password, identity) = package_fixture(&dir);
        assert_eq!(inspect_package(&package).unwrap().entry_count, 1);
        let target = dir.join("restored");
        restore_package(&package, &target, &identity).unwrap();
        assert_eq!(verify_restored(&target, &password).unwrap().entry_count, 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_restored_vault_opens_as_a_new_standby_instance_of_the_same_vault() {
        let dir = scratch("standby");
        let (package, password, identity) = package_fixture(&dir);
        let target = dir.join("restored");
        restore_package(&package, &target, &identity).unwrap();
        assert!(target.join(crate::identity::RESTORED_MARKER).exists());
        // The marker is not part of the manifest, so verification is unaffected by it.
        verify_restored(&target, &password).unwrap();
        let salt = fs::read(target.join("vault.salt")).unwrap();
        let key = crate::derive_key(&password, &salt).unwrap();
        let before = {
            let c = crate::open_db(&target.join("vault.db"), &key).unwrap();
            crate::identity::ensure(&c).unwrap()
        };
        // The first full open (schema init) consumes the marker.
        let c = crate::open_db(&target.join("vault.db"), &key).unwrap();
        crate::init_schema(&c).unwrap();
        let after = crate::identity::ensure(&c).unwrap();
        assert_eq!(after.vault_id, before.vault_id);
        assert_ne!(after.instance_id, before.instance_id);
        assert_eq!(after.role, crate::identity::Role::Standby);
        assert!(!target.join(crate::identity::RESTORED_MARKER).exists());
        // Opening again does not change it.
        crate::init_schema(&c).unwrap();
        assert_eq!(crate::identity::ensure(&c).unwrap(), after);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn wrong_identity_and_tamper_never_populate_the_target() {
        let dir = scratch("reject");
        let (package, _password, identity) = package_fixture(&dir);
        let (_, wrong) = generate_identity();
        let wrong_file = dir.join("wrong-identity.txt");
        fs::write(&wrong_file, wrong).unwrap();
        let wrong_target = dir.join("wrong-target");
        // Regression: a failed decryption used to create target files before
        // authentication had completed, leaving an ambiguous partial restore.
        assert!(restore_package(&package, &wrong_target, &wrong_file).is_err());
        assert!(!wrong_target.exists());
        let mut bytes = fs::read(&package).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        let tampered = dir.join("tampered.unvpkg");
        fs::write(&tampered, bytes).unwrap();
        let target = dir.join("tampered-target");
        assert!(restore_package(&tampered, &target, &identity).is_err());
        assert!(!target.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
