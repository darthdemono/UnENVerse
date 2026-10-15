//! Local recovery destinations, package runs and the unlocked scheduler.
//!
//! The database holds durable intent and state in `vault_core::ops`; this
//! module owns the local filesystem protocol and the process lifetime.

use crate::access::{self, Access};
use crate::data;
use crate::destinations::{
    BackupDestination, LocalDestination, SftpAuthMethod, SftpConfig, SftpCredential,
    SftpDestination,
};
use crate::error::{CliError, CliResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::sync::watch;
use vault_core::ops::{self, Run, RunState, Schedule};
use vault_core::recovery::{self, Manifest, PackageInput};
use vault_core::{open_db, VaultKey, Zeroize};

const DESTINATIONS_META_KEY: &str = "recovery_destinations_v1";
const DESTINATION_MARKER: &str = ".unenverse-destination.json";
const RETENTION_META_KEY: &str = "recovery_retention_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Destination {
    pub id: String,
    pub path: PathBuf,
    pub created_at: String,
    #[serde(default)]
    pub transport: DestinationTransport,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DestinationTransport {
    #[default]
    Local,
    Sftp(SftpConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageReceipt {
    pub package: PathBuf,
    pub completion: PathBuf,
    /// The destination that completed and read this package back.  Retention
    /// ignores legacy or foreign manifests without this exact identifier.
    #[serde(default)]
    pub destination_id: String,
    pub sha256: String,
    pub size: u64,
    pub manifest: Manifest,
}

/// The intentionally conservative default: a recent daily set plus enough
/// weekly and monthly points to recover from a late-discovered problem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub daily: u8,
    pub weekly: u8,
    pub monthly: u8,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            daily: 7,
            weekly: 4,
            monthly: 6,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecoveryPaths {
    pub db_path: PathBuf,
    pub salt_path: PathBuf,
    pub data_dir: PathBuf,
    pub materialisation_log: PathBuf,
}

impl RecoveryPaths {
    pub fn local() -> Self {
        let db_path = access::default_db_path();
        let salt_path = access::default_salt_path();
        let data_dir = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Self {
            db_path,
            salt_path,
            data_dir,
            materialisation_log: crate::matlog::path(),
        }
    }

    fn auxiliary(&self) -> Vec<(String, PathBuf)> {
        vec![
            ("nodes.json".to_string(), self.data_dir.join("nodes.json")),
            (
                "lan/server.crt".to_string(),
                self.data_dir.join("lan/server.crt"),
            ),
            (
                "lan/server.key".to_string(),
                self.data_dir.join("lan/server.key"),
            ),
            (
                "materialisations.jsonl".to_string(),
                self.materialisation_log.clone(),
            ),
        ]
    }
}

fn validate_destination_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(
            "Destination id must contain only lowercase letters, digits, '-' or '_'".to_string(),
        );
    }
    Ok(())
}

fn sync_dir(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let tmp = path.with_extension(format!("tmp-{}", vault_core::new_uuid()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    sync_dir(path.parent().unwrap_or_else(|| Path::new(".")))
}

fn destination_marker(dest: &Destination) -> serde_json::Value {
    serde_json::json!({ "format": 1, "destination_id": dest.id, "created_at": dest.created_at })
}

fn require_marker(dest: &Destination) -> Result<(), String> {
    let marker = dest.path.join(DESTINATION_MARKER);
    if !marker.exists() {
        return Err(
            "Destination marker is missing; re-add the destination after checking its path"
                .to_string(),
        );
    }
    let found: serde_json::Value =
        serde_json::from_slice(&fs::read(&marker).map_err(|e| e.to_string())?)
            .map_err(|_| "Destination marker is invalid".to_string())?;
    if found["destination_id"].as_str() == Some(dest.id.as_str()) {
        Ok(())
    } else {
        Err("Destination marker belongs to another destination".to_string())
    }
}

fn create_marker(dest: &Destination) -> Result<(), String> {
    let marker = dest.path.join(DESTINATION_MARKER);
    if marker.exists() {
        return require_marker(dest);
    }
    atomic_json(&marker, &destination_marker(dest))
}

fn load_destinations(conn: &vault_core::SqlConnection) -> Result<Vec<Destination>, String> {
    use rusqlite::OptionalExtension;
    let json: Option<String> = conn
        .query_row(
            "SELECT value FROM vault_meta WHERE key = ?1",
            [DESTINATIONS_META_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    json.map(|v| {
        serde_json::from_str(&v)
            .map_err(|e| format!("Stored recovery destinations are invalid: {e}"))
    })
    .transpose()
    .map(|v| v.unwrap_or_default())
}

fn save_destinations(
    conn: &vault_core::SqlConnection,
    destinations: &[Destination],
) -> Result<(), String> {
    let value = serde_json::to_string(destinations).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?1, ?2)",
        (DESTINATIONS_META_KEY, value),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn add_destination(
    conn: &vault_core::SqlConnection,
    id: &str,
    path: &Path,
) -> Result<Destination, String> {
    validate_destination_id(id)?;
    fs::create_dir_all(path).map_err(|e| format!("Create destination {}: {e}", path.display()))?;
    let mut all = load_destinations(conn)?;
    if all.iter().any(|d| d.id == id) {
        return Err(format!("Destination `{id}` already exists"));
    }
    let dest = Destination {
        id: id.to_string(),
        path: path.canonicalize().map_err(|e| e.to_string())?,
        created_at: vault_core::iso_now(),
        transport: DestinationTransport::Local,
    };
    create_marker(&dest)?;
    all.push(dest.clone());
    save_destinations(conn, &all)?;
    Ok(dest)
}

pub fn list_destinations(conn: &vault_core::SqlConnection) -> Result<Vec<Destination>, String> {
    load_destinations(conn)
}

pub fn remove_destination(conn: &vault_core::SqlConnection, id: &str) -> Result<(), String> {
    let mut all = load_destinations(conn)?;
    let before = all.len();
    all.retain(|d| d.id != id);
    if all.len() == before {
        return Err(format!("No recovery destination `{id}`"));
    }
    save_destinations(conn, &all)
}

fn find_destination(conn: &vault_core::SqlConnection, id: &str) -> Result<Destination, String> {
    load_destinations(conn)?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("No recovery destination `{id}`"))
}

fn credential_for_conn(
    conn: &vault_core::SqlConnection,
    config: &SftpConfig,
) -> Result<SftpCredential, String> {
    let (_, vault) = vault_core::snapshot_vault(conn).map_err(|e| e.to_string())?;
    let vault = vault.ok_or_else(|| {
        "An SFTP destination needs a vault entry containing its authentication secret".to_string()
    })?;
    let index = data::find_entry_index(&vault, &config.credential_entry).map_err(|e| e.message)?;
    let entries = data::entries(&vault);
    let secret = entries[index]
        .get("api_key")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "SFTP credential entry `{}` needs a non-empty api_key field",
                config.credential_entry
            )
        })?;
    Ok(match config.auth {
        SftpAuthMethod::Password => SftpCredential::Password(secret.to_string()),
        SftpAuthMethod::PrivateKey => SftpCredential::PrivateKey(secret.to_string()),
    })
}

fn open_destination(
    conn: &vault_core::SqlConnection,
    destination: &Destination,
) -> Result<Box<dyn BackupDestination>, String> {
    match &destination.transport {
        DestinationTransport::Local => {
            require_marker(destination)?;
            Ok(Box::new(LocalDestination::new(destination.path.clone())))
        }
        DestinationTransport::Sftp(config) => Ok(Box::new(SftpDestination::new(
            config.clone(),
            credential_for_conn(conn, config)?,
        ))),
    }
}

pub fn add_sftp_destination(
    conn: &vault_core::SqlConnection,
    id: &str,
    config: SftpConfig,
) -> Result<Destination, String> {
    validate_destination_id(id)?;
    if config.host.trim().is_empty()
        || config.username.trim().is_empty()
        || config.remote_path.trim().is_empty()
        || config.credential_entry.trim().is_empty()
        || config.host_key_fingerprint.trim().is_empty()
    {
        return Err(
            "SFTP host, user, remote path, credential entry and host-key fingerprint are required"
                .to_string(),
        );
    }
    if load_destinations(conn)?
        .iter()
        .any(|destination| destination.id == id)
    {
        return Err(format!("Destination `{id}` already exists"));
    }
    // Connect before persisting.  This rechecks the user-confirmed pin while
    // authenticating with the vault-resident secret, so a changed host cannot
    // become a destination through a stale confirmation dialog.
    let credential = credential_for_conn(conn, &config)?;
    let mut remote = SftpDestination::new(config.clone(), credential);
    remote.probe()?;
    let destination = Destination {
        id: id.to_string(),
        // Kept for the local-destination JSON shape from R05.  SFTP operations
        // exclusively use the POSIX remote_path inside the transport config.
        path: PathBuf::from(&config.remote_path),
        created_at: vault_core::iso_now(),
        transport: DestinationTransport::Sftp(config),
    };
    let mut all = load_destinations(conn)?;
    all.push(destination.clone());
    save_destinations(conn, &all)?;
    Ok(destination)
}

pub fn add_sftp_destination_access(
    access: &Access,
    id: &str,
    config: SftpConfig,
) -> CliResult<Destination> {
    let Access::Local(_) = access else {
        return Err(CliError::invalid(
            "SFTP recovery destinations require local vault access",
        ));
    };
    add_sftp_destination(&access.conn()?, id, config).map_err(CliError::from)
}

fn cleanup_stale_parts(destination: &mut dyn BackupDestination) -> Result<usize, String> {
    let mut removed = 0;
    for object in destination.list()? {
        // Only clean incomplete packages made by this application.  A shared
        // directory can contain another owner's arbitrary `.part` files.
        if object.name.starts_with("unenverse-") && object.name.ends_with(".part") {
            destination.delete(&object.name)?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn sha256_file(path: &Path) -> Result<(String, u64), String> {
    use sha2::{Digest, Sha256};
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        size += n as u64;
    }
    Ok((hex::encode(hash.finalize()), size))
}

struct InterruptibleWriter<'a> {
    file: &'a File,
    stop: &'a AtomicBool,
}

impl Write for InterruptibleWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.stop.load(Ordering::Acquire) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "operation stopped",
            ));
        }
        let mut file = self.file;
        file.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self.file;
        file.flush()
    }
}

fn write_package(
    destination_id: &str,
    destination: &mut dyn BackupDestination,
    staging_dir: &Path,
    input: PackageInput<'_>,
    stop: Option<&AtomicBool>,
) -> Result<PackageReceipt, String> {
    destination.probe()?;
    cleanup_stale_parts(destination)?;
    let stem = format!(
        "unenverse-{}-{}",
        vault_core::iso_now().replace(':', "-"),
        vault_core::new_uuid()
    );
    fs::create_dir_all(staging_dir).map_err(|e| e.to_string())?;
    let package_name = format!("{stem}.unvpkg");
    let package_part_name = format!("{package_name}.part");
    let completion_name = format!("{stem}.complete.json");
    let completion_part_name = format!("{completion_name}.part");
    let staged_package = staging_dir.join(format!("{stem}.unvpkg.part"));
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staged_package)
        .map_err(|e| e.to_string())?;
    let written = match stop {
        Some(stop) => recovery::write_package(input, InterruptibleWriter { file: &file, stop }),
        None => recovery::write_package(input, &file),
    };
    let manifest = match written {
        Ok(manifest) => manifest,
        Err(error) => {
            drop(file);
            let _ = fs::remove_file(&staged_package);
            return Err(error);
        }
    };
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let (sha256, size) = sha256_file(&staged_package)?;
    if let Err(error) = destination.put(&staged_package, &package_part_name) {
        let _ = fs::remove_file(&staged_package);
        return Err(format!("Upload recovery package: {error}"));
    }
    if let Err(error) = destination.rename_no_replace(&package_part_name, &package_name) {
        let _ = fs::remove_file(&staged_package);
        return Err(format!("Finalize recovery package: {error}"));
    }
    let readback_package = staging_dir.join(format!("{stem}.readback.unvpkg"));
    if let Err(error) = destination.get(&package_name, &readback_package) {
        let _ = fs::remove_file(&staged_package);
        return Err(format!("Read back recovery package: {error}"));
    }
    let (actual, actual_size) = sha256_file(&readback_package)?;
    if sha256 != actual || size != actual_size {
        let _ = fs::remove_file(&staged_package);
        let _ = fs::remove_file(&readback_package);
        return Err("Recovery package did not verify after destination read-back".to_string());
    }
    let receipt = PackageReceipt {
        package: PathBuf::from(&package_name),
        completion: PathBuf::from(&completion_name),
        destination_id: destination_id.to_string(),
        sha256,
        size,
        manifest,
    };
    let staged_receipt = staging_dir.join(format!("{stem}.complete.json"));
    atomic_json(&staged_receipt, &receipt)?;
    if let Err(error) = destination.put(&staged_receipt, &completion_part_name) {
        let _ = fs::remove_file(&staged_package);
        let _ = fs::remove_file(&readback_package);
        let _ = fs::remove_file(&staged_receipt);
        return Err(format!("Upload recovery completion manifest: {error}"));
    }
    if let Err(error) = destination.rename_no_replace(&completion_part_name, &completion_name) {
        let _ = fs::remove_file(&staged_package);
        let _ = fs::remove_file(&readback_package);
        let _ = fs::remove_file(&staged_receipt);
        return Err(format!("Finalize recovery completion manifest: {error}"));
    }
    let readback_receipt = staging_dir.join(format!("{stem}.readback.complete.json"));
    if let Err(error) = destination.get(&completion_name, &readback_receipt) {
        let _ = fs::remove_file(&staged_package);
        let _ = fs::remove_file(&readback_package);
        let _ = fs::remove_file(&staged_receipt);
        return Err(format!("Read back recovery completion manifest: {error}"));
    }
    let read_back: PackageReceipt = serde_json::from_slice(
        &fs::read(&readback_receipt).map_err(|e| format!("Read completion record: {e}"))?,
    )
    .map_err(|e| format!("Read completion record: {e}"))?;
    let _ = fs::remove_file(&staged_package);
    let _ = fs::remove_file(&readback_package);
    let _ = fs::remove_file(&staged_receipt);
    let _ = fs::remove_file(&readback_receipt);
    if read_back.destination_id != destination_id
        || read_back.sha256 != receipt.sha256
        || read_back.size != receipt.size
        || read_back.package != receipt.package
    {
        return Err("Recovery completion manifest did not verify after read-back".to_string());
    }
    Ok(receipt)
}

pub fn retention_policy(conn: &vault_core::SqlConnection) -> Result<RetentionPolicy, String> {
    use rusqlite::OptionalExtension;
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM vault_meta WHERE key = ?1",
            [RETENTION_META_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    value
        .map(|json| {
            serde_json::from_str(&json)
                .map_err(|e| format!("Stored recovery retention policy is invalid: {e}"))
        })
        .transpose()
        .map(|policy| policy.unwrap_or_default())
}

pub fn set_retention_policy(
    conn: &vault_core::SqlConnection,
    policy: &RetentionPolicy,
) -> Result<(), String> {
    if policy.daily == 0 || policy.weekly == 0 || policy.monthly == 0 {
        return Err("Retention windows must each retain at least one package".to_string());
    }
    let value = serde_json::to_string(policy).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO vault_meta (key, value) VALUES (?1, ?2)",
        (RETENTION_META_KEY, value),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn receipt_name(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

fn completed_receipts(
    destination: &mut dyn BackupDestination,
    destination_id: &str,
    staging_dir: &Path,
) -> Result<Vec<PackageReceipt>, String> {
    fs::create_dir_all(staging_dir).map_err(|e| e.to_string())?;
    let objects = destination.list()?;
    let names = objects
        .iter()
        .map(|object| object.name.clone())
        .collect::<HashSet<_>>();
    let mut receipts = Vec::new();
    for object in objects {
        if !object.name.starts_with("unenverse-") || !object.name.ends_with(".complete.json") {
            continue;
        }
        let local = staging_dir.join(format!("receipt-{}", vault_core::new_uuid()));
        let read = destination
            .get(&object.name, &local)
            .and_then(|_| fs::read(&local).map_err(|e| e.to_string()));
        let _ = fs::remove_file(&local);
        let Ok(bytes) = read else {
            // A missing or malformed completion object is not a completed
            // backup, and therefore must never be selected or deleted here.
            continue;
        };
        let Ok(receipt) = serde_json::from_slice::<PackageReceipt>(&bytes) else {
            continue;
        };
        let package_name = receipt_name(&receipt.package);
        if receipt.destination_id == destination_id
            && receipt_name(&receipt.completion) == Some(object.name.as_str())
            && package_name.is_some_and(|name| names.contains(name))
        {
            receipts.push(receipt);
        }
    }
    Ok(receipts)
}

fn receipt_periods(receipt: &PackageReceipt) -> Result<(String, String, String), String> {
    let timestamp = timestamp(&receipt.manifest.created_at)?;
    let date = timestamp.in_tz("UTC").map_err(|e| e.to_string())?.date();
    let week = date.iso_week_date();
    Ok((
        format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day()),
        format!("{:04}-W{:02}", week.year(), week.week()),
        format!("{:04}-{:02}", date.year(), date.month()),
    ))
}

/// Enforces the per-destination retention policy.  It only considers a pair
/// whose read-back receipt names this exact destination ID, and it always keeps
/// the newest verified receipt even if a custom policy is later made stricter.
pub fn enforce_retention_for_conn(
    conn: &vault_core::SqlConnection,
    destination_id: &str,
    staging_dir: &Path,
) -> Result<usize, String> {
    let configured = find_destination(conn, destination_id)?;
    let mut destination = open_destination(conn, &configured)?;
    destination.probe()?;
    let policy = retention_policy(conn)?;
    let mut receipts = completed_receipts(destination.as_mut(), destination_id, staging_dir)?;
    receipts.sort_by(|left, right| right.manifest.created_at.cmp(&left.manifest.created_at));
    let Some(newest) = receipts.first() else {
        return Ok(0);
    };
    let mut keep = HashSet::from([newest.completion.clone()]);
    let mut days = HashSet::new();
    let mut weeks = HashSet::new();
    let mut months = HashSet::new();
    for receipt in &receipts {
        let (day, week, month) = receipt_periods(receipt)?;
        let mut retained = receipt.completion == newest.completion;
        if days.len() < usize::from(policy.daily) && days.insert(day) {
            retained = true;
        }
        if weeks.len() < usize::from(policy.weekly) && weeks.insert(week) {
            retained = true;
        }
        if months.len() < usize::from(policy.monthly) && months.insert(month) {
            retained = true;
        }
        if retained {
            keep.insert(receipt.completion.clone());
        }
    }
    let mut removed = 0;
    for receipt in receipts {
        if keep.contains(&receipt.completion) {
            continue;
        }
        let Some(completion) = receipt_name(&receipt.completion) else {
            continue;
        };
        let Some(package) = receipt_name(&receipt.package) else {
            continue;
        };
        // Delete the manifest first.  A process interruption can leave an
        // orphaned encrypted package, but can never leave it looking complete.
        destination.delete(completion)?;
        destination.delete(package)?;
        removed += 1;
    }
    Ok(removed)
}

pub fn set_retention_policy_access(access: &Access, policy: &RetentionPolicy) -> CliResult<()> {
    set_retention_policy(&access.conn()?, policy).map_err(CliError::from)
}

pub fn retention_policy_access(access: &Access) -> CliResult<RetentionPolicy> {
    retention_policy(&access.conn()?).map_err(CliError::from)
}

fn latest_completed_receipt(
    conn: &vault_core::SqlConnection,
    destination_id: &str,
    staging_dir: &Path,
) -> Result<(Box<dyn BackupDestination>, PackageReceipt), String> {
    let configured = find_destination(conn, destination_id)?;
    let mut destination = open_destination(conn, &configured)?;
    destination.probe()?;
    let mut receipts = completed_receipts(destination.as_mut(), destination_id, staging_dir)?;
    receipts.sort_by(|left, right| right.manifest.created_at.cmp(&left.manifest.created_at));
    let receipt = receipts
        .into_iter()
        .next()
        .ok_or_else(|| format!("No verified recovery package exists for `{destination_id}`"))?;
    Ok((destination, receipt))
}

/// Downloads, restores and verifies the latest completed package to a temporary
/// directory.  It never activates the restored vault and removes both package
/// and restore directory whether verification succeeds or fails.
pub fn restore_drill_for_paths(
    conn: &vault_core::SqlConnection,
    key: &VaultKey,
    paths: &RecoveryPaths,
    destination_id: &str,
) -> Result<Manifest, String> {
    let staging = paths.data_dir.join("recovery-staging");
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let (mut destination, receipt) = latest_completed_receipt(conn, destination_id, &staging)?;
    let package_name = receipt_name(&receipt.package)
        .ok_or_else(|| "Recovery receipt has an invalid package filename".to_string())?;
    let package = staging.join(format!("drill-{}.unvpkg", vault_core::new_uuid()));
    destination.get(package_name, &package)?;
    let (hash, size) = sha256_file(&package)?;
    if hash != receipt.sha256 || size != receipt.size {
        let _ = fs::remove_file(&package);
        return Err("Restore drill download does not match its verified receipt".to_string());
    }
    let mut identity = recovery::drill_identity(conn)?.ok_or_else(|| {
        "No vault-encrypted recovery identity is available for an unattended restore drill"
            .to_string()
    })?;
    let target = staging.join(format!("drill-restore-{}", vault_core::new_uuid()));
    let result = recovery::restore_package_with_identity(&package, &target, &identity)
        .and_then(|_| recovery::verify_restored_with_key(&target, key));
    identity.zeroize();
    let _ = fs::remove_file(&package);
    let _ = fs::remove_dir_all(&target);
    result
}

pub fn package_for_paths(
    conn: &vault_core::SqlConnection,
    key: &VaultKey,
    paths: &RecoveryPaths,
    destination_id: &str,
) -> Result<PackageReceipt, String> {
    package_for_paths_stoppable(conn, key, paths, destination_id, None)
}

fn package_for_paths_stoppable(
    conn: &vault_core::SqlConnection,
    key: &VaultKey,
    paths: &RecoveryPaths,
    destination_id: &str,
    stop: Option<&AtomicBool>,
) -> Result<PackageReceipt, String> {
    let destination = find_destination(conn, destination_id)?;
    let mut destination_transport = open_destination(conn, &destination)?;
    let receipt = write_package(
        destination_id,
        destination_transport.as_mut(),
        &paths.data_dir.join("recovery-staging"),
        PackageInput {
            conn,
            key,
            db_path: &paths.db_path,
            salt_path: &paths.salt_path,
            auxiliary: paths.auxiliary(),
            app_version: env!("CARGO_PKG_VERSION"),
        },
        stop,
    )?;
    drop(destination_transport);
    enforce_retention_for_conn(
        conn,
        destination_id,
        &paths.data_dir.join("recovery-staging"),
    )?;
    Ok(receipt)
}

pub fn create_recovery_identity(access: &Access) -> CliResult<(String, String)> {
    let conn = access.conn()?;
    let (recipient, identity) = recovery::generate_identity();
    recovery::set_recipient(&conn, &recipient).map_err(CliError::from)?;
    recovery::set_drill_identity(&conn, &identity).map_err(CliError::from)?;
    Ok((recipient, identity))
}

pub fn package(access: &Access, destination_id: &str) -> CliResult<PackageReceipt> {
    let Access::Local(key) = access else {
        return Err(CliError::invalid(
            "Recovery packages require local vault access",
        ));
    };
    let conn = access.conn()?;
    package_for_paths(&conn, key, &RecoveryPaths::local(), destination_id).map_err(CliError::from)
}

pub fn add_destination_access(access: &Access, id: &str, path: &Path) -> CliResult<Destination> {
    add_destination(&access.conn()?, id, path).map_err(CliError::from)
}
pub fn list_destinations_access(access: &Access) -> CliResult<Vec<Destination>> {
    list_destinations(&access.conn()?).map_err(CliError::from)
}
pub fn remove_destination_access(access: &Access, id: &str) -> CliResult<()> {
    remove_destination(&access.conn()?, id).map_err(CliError::from)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScheduleRule {
    Interval {
        seconds: u64,
    },
    Daily {
        at: String,
        time_zone: String,
    },
    Monthly {
        day: u8,
        at: String,
        time_zone: String,
    },
}

fn timestamp(text: &str) -> Result<jiff::Timestamp, String> {
    text.parse()
        .map_err(|e| format!("Invalid schedule timestamp `{text}`: {e}"))
}

fn scheduled_time(at: &str) -> Result<(i8, i8), String> {
    let (hour, minute) = at.split_once(':').ok_or("Scheduled time must be HH:MM")?;
    let hour = hour
        .parse::<i8>()
        .map_err(|_| "Scheduled time must be HH:MM")?;
    let minute = minute
        .parse::<i8>()
        .map_err(|_| "Scheduled time must be HH:MM")?;
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) {
        return Err("Scheduled time must be HH:MM".to_string());
    }
    Ok((hour, minute))
}

fn monthly_date(
    date: jiff::civil::Date,
    day: u8,
    next_month: bool,
) -> Result<jiff::civil::Date, String> {
    if day == 0 || day > 31 {
        return Err("Monthly day must be between 1 and 31".to_string());
    }
    let mut year = date.year();
    let mut month = date.month();
    if next_month {
        if month == 12 {
            year += 1;
            month = 1;
        } else {
            month += 1;
        }
    }
    let first = date
        .with()
        .year(year)
        .month(month)
        .day(1)
        .build()
        .map_err(|e| e.to_string())?;
    first
        .with()
        .day((day as i8).min(first.days_in_month()))
        .build()
        .map_err(|e| e.to_string())
}

fn next_due(rule: &ScheduleRule, after: jiff::Timestamp) -> Result<jiff::Timestamp, String> {
    match rule {
        ScheduleRule::Interval { seconds } if *seconds > 0 => after
            .checked_add(Duration::from_secs(*seconds))
            .map_err(|e| e.to_string()),
        ScheduleRule::Interval { .. } => Err("Schedule interval must be positive".to_string()),
        ScheduleRule::Daily { at, time_zone } => {
            let (hour, minute) = scheduled_time(at)?;
            let local = after
                .in_tz(time_zone)
                .map_err(|_| format!("Unknown IANA time zone `{time_zone}`"))?;
            let today = local
                .date()
                .at(hour, minute, 0, 0)
                .in_tz(time_zone)
                .map_err(|e| e.to_string())?
                .timestamp();
            if today > after {
                Ok(today)
            } else {
                local
                    .date()
                    .tomorrow()
                    .map_err(|e| e.to_string())?
                    .at(hour, minute, 0, 0)
                    .in_tz(time_zone)
                    .map_err(|e| e.to_string())
                    .map(|z| z.timestamp())
            }
        }
        ScheduleRule::Monthly { day, at, time_zone } => {
            let (hour, minute) = scheduled_time(at)?;
            let local = after
                .in_tz(time_zone)
                .map_err(|_| format!("Unknown IANA time zone `{time_zone}`"))?;
            let this_month = monthly_date(local.date(), *day, false)?;
            let candidate = this_month
                .at(hour, minute, 0, 0)
                .in_tz(time_zone)
                .map_err(|e| e.to_string())?
                .timestamp();
            if candidate > after {
                Ok(candidate)
            } else {
                monthly_date(local.date(), *day, true)?
                    .at(hour, minute, 0, 0)
                    .in_tz(time_zone)
                    .map_err(|e| e.to_string())
                    .map(|z| z.timestamp())
            }
        }
    }
}

pub fn add_schedule_for_conn(
    conn: &vault_core::SqlConnection,
    destination_id: &str,
    rule: ScheduleRule,
    max_lateness_secs: Option<i64>,
) -> Result<Schedule, String> {
    add_schedule_kind_for_conn(
        conn,
        "backup.package",
        destination_id,
        rule,
        max_lateness_secs,
    )
}

fn add_schedule_kind_for_conn(
    conn: &vault_core::SqlConnection,
    kind: &str,
    destination_id: &str,
    rule: ScheduleRule,
    max_lateness_secs: Option<i64>,
) -> Result<Schedule, String> {
    let _ = find_destination(conn, destination_id)?;
    let encoded = serde_json::to_string(&rule).map_err(|e| e.to_string())?;
    let due = next_due(&rule, jiff::Timestamp::now())?;
    ops::add_schedule(
        conn,
        &vault_core::new_uuid(),
        kind,
        destination_id,
        &encoded,
        Some(&due.to_string()),
        max_lateness_secs,
    )
}

pub fn add_schedule(
    access: &Access,
    destination_id: &str,
    rule: ScheduleRule,
    max_lateness_secs: Option<i64>,
) -> CliResult<Schedule> {
    add_schedule_for_conn(&access.conn()?, destination_id, rule, max_lateness_secs)
        .map_err(CliError::from)
}

/// Schedules a monthly restore drill.  The first day at 03:00 UTC is the
/// safe default: it is a real calendar month cadence and does not infer the
/// machine's local time zone.
pub fn add_monthly_drill_schedule(
    access: &Access,
    destination_id: &str,
    day: u8,
    at: &str,
    time_zone: &str,
    max_lateness_secs: Option<i64>,
) -> CliResult<Schedule> {
    add_monthly_drill_schedule_for_conn(
        &access.conn()?,
        destination_id,
        day,
        at,
        time_zone,
        max_lateness_secs,
    )
    .map_err(CliError::from)
}

pub fn add_monthly_drill_schedule_for_conn(
    conn: &vault_core::SqlConnection,
    destination_id: &str,
    day: u8,
    at: &str,
    time_zone: &str,
    max_lateness_secs: Option<i64>,
) -> Result<Schedule, String> {
    add_schedule_kind_for_conn(
        conn,
        "backup.drill",
        destination_id,
        ScheduleRule::Monthly {
            day,
            at: at.to_string(),
            time_zone: time_zone.to_string(),
        },
        max_lateness_secs,
    )
}

fn execute_run(
    conn: &vault_core::SqlConnection,
    key: &VaultKey,
    paths: &RecoveryPaths,
    schedule: Option<&Schedule>,
    kind: &str,
    destination: &str,
    missed: i64,
    epoch: Option<i64>,
    stop: Option<&AtomicBool>,
) -> Result<Run, String> {
    let id = vault_core::new_uuid();
    match epoch {
        Some(epoch) => ops::create_run_fenced(conn, &id, schedule, missed, epoch)?,
        None => ops::create_run_with_kind(conn, &id, schedule, kind, missed, None)?,
    };
    let transition = |to, outcome, error| match epoch {
        Some(e) => ops::transition_run_fenced(conn, &id, e, to, outcome, error),
        None => ops::transition_run(conn, &id, to, outcome, error),
    };
    transition(RunState::Planning, None, None)?;
    transition(RunState::Running, None, None)?;
    let result = match kind {
        "backup.package" => {
            package_for_paths_stoppable(conn, key, paths, destination, stop).map(|receipt| {
                format!(
                    "{} ({} bytes, read-back verified)",
                    receipt.package.display(),
                    receipt.size
                )
            })
        }
        "backup.drill" => restore_drill_for_paths(conn, key, paths, destination)
            .map(|manifest| format!("restore drill verified {} entries", manifest.entry_count)),
        other => Err(format!("Unsupported recovery operation kind `{other}`")),
    };
    match result {
        Ok(outcome) => transition(RunState::Succeeded, Some(&outcome), None),
        Err(error) if stop.is_some_and(|flag| flag.load(Ordering::Acquire)) => {
            transition(RunState::Paused, Some("stopped"), Some(("stopped", &error)))
        }
        Err(error) => transition(
            RunState::Failed,
            None,
            Some((
                if kind == "backup.drill" {
                    "drill_failed"
                } else {
                    "backup_failed"
                },
                &error,
            )),
        ),
    }
}

pub fn run_now(access: &Access, destination_id: &str) -> CliResult<Run> {
    let Access::Local(key) = access else {
        return Err(CliError::invalid("Operations require local vault access"));
    };
    let conn = access.conn()?;
    execute_run(
        &conn,
        key,
        &RecoveryPaths::local(),
        None,
        "backup.package",
        destination_id,
        0,
        None,
        None,
    )
    .map_err(CliError::from)
}

/// Runs one restore drill now, recording the download, verification outcome and
/// failure reason in the durable operations list.
pub fn run_drill_now(access: &Access, destination_id: &str) -> CliResult<Run> {
    let Access::Local(key) = access else {
        return Err(CliError::invalid("Operations require local vault access"));
    };
    let conn = access.conn()?;
    run_drill_for_paths(&conn, key, &RecoveryPaths::local(), destination_id).map_err(CliError::from)
}

pub fn run_drill_for_paths(
    conn: &vault_core::SqlConnection,
    key: &VaultKey,
    paths: &RecoveryPaths,
    destination_id: &str,
) -> Result<Run, String> {
    execute_run(
        conn,
        key,
        paths,
        None,
        "backup.drill",
        destination_id,
        0,
        None,
        None,
    )
}

pub fn list_runs(access: &Access) -> CliResult<Vec<Run>> {
    ops::list_runs(&access.conn()?).map_err(CliError::from)
}
pub fn show_run(access: &Access, id: &str) -> CliResult<Run> {
    ops::get_run(&access.conn()?, id)
        .map_err(CliError::from)?
        .ok_or_else(|| CliError::not_found(format!("No operation `{id}`")))
}
pub fn cancel_run(access: &Access, id: &str) -> CliResult<Run> {
    ops::transition_run(
        &access.conn()?,
        id,
        RunState::Cancelled,
        Some("cancelled"),
        None,
    )
    .map_err(CliError::from)
}

fn run_due(
    conn: &vault_core::SqlConnection,
    key: &VaultKey,
    paths: &RecoveryPaths,
    epoch: i64,
    now: jiff::Timestamp,
    stop: &AtomicBool,
) -> Result<(), String> {
    for schedule in ops::list_schedules(conn)? {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        if !schedule.enabled {
            continue;
        }
        let Some(due_text) = &schedule.next_due_at else {
            continue;
        };
        let due = timestamp(due_text)?;
        if due > now {
            continue;
        }
        let rule: ScheduleRule = serde_json::from_str(&schedule.rule)
            .map_err(|e| format!("Invalid schedule rule: {e}"))?;
        let late = now.since(due).map_err(|e| e.to_string())?.get_seconds();
        let next = next_due(&rule, now)?.to_string();
        if schedule.max_lateness_secs.is_some_and(|max| late > max) {
            let id = vault_core::new_uuid();
            ops::create_run_fenced(conn, &id, Some(&schedule), 0, epoch)?;
            ops::transition_run_fenced(conn, &id, epoch, RunState::Planning, None, None)?;
            ops::transition_run_fenced(
                conn,
                &id,
                epoch,
                RunState::SkippedStale,
                Some("stale"),
                Some(("stale", "Schedule exceeded its maximum lateness")),
            )?;
            ops::complete_schedule_occurrence_fenced(conn, &schedule.id, &id, Some(&next), epoch)?;
            continue;
        }
        let missed = match &rule {
            ScheduleRule::Interval { seconds } => ((late.max(0) as u64 / seconds) + 1) as i64,
            ScheduleRule::Daily { .. } => 1,
            ScheduleRule::Monthly { .. } => 1,
        };
        let run = execute_run(
            conn,
            key,
            paths,
            Some(&schedule),
            &schedule.kind,
            &schedule.target,
            missed,
            Some(epoch),
            Some(stop),
        )?;
        ops::complete_schedule_occurrence_fenced(conn, &schedule.id, &run.id, Some(&next), epoch)?;
    }
    Ok(())
}

/// Holds the advisory lock and a cancellable worker. `stop` marks active runs
/// paused before the caller drops or zeroizes its own vault key.
pub struct Executor {
    stop: watch::Sender<bool>,
    stop_flag: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
    _lock: File,
}

impl Executor {
    pub fn start(mut key: VaultKey, paths: RecoveryPaths) -> Result<Self, String> {
        let lock_path = paths.data_dir.join(".unenverse-ops.lock");
        fs::create_dir_all(&paths.data_dir).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        if let Err(error) = lock.try_lock() {
            let holder = open_db(&paths.db_path, &key)
                .ok()
                .and_then(|conn| ops::holder(&conn).ok().flatten())
                .map(|h| h.holder)
                .unwrap_or_else(|| "another executor".to_string());
            key.zeroize();
            return Err(format!(
                "Operations executor is already held by {holder}: {error}"
            ));
        }
        let (stop, receive) = watch::channel(false);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop_flag);
        let paths_for_thread = paths.clone();
        let join = thread::spawn(move || {
            let conn = match open_db(&paths_for_thread.db_path, &key) {
                Ok(conn) => conn,
                Err(_) => {
                    key.zeroize();
                    return;
                }
            };
            if vault_core::init_schema(&conn).is_err() {
                key.zeroize();
                return;
            }
            let holder = format!(
                "{}:{}",
                std::env::var("HOSTNAME").unwrap_or_else(|_| "desktop".to_string()),
                std::process::id()
            );
            let lease = match ops::acquire_lease(&conn, &holder, std::process::id() as i64, &holder)
            {
                Ok(lease) => lease,
                Err(_) => {
                    key.zeroize();
                    return;
                }
            };
            let _ = ops::mark_interrupted(&conn);
            loop {
                if *receive.borrow() {
                    break;
                }
                let _ = run_due(
                    &conn,
                    &key,
                    &paths_for_thread,
                    lease.epoch,
                    jiff::Timestamp::now(),
                    &worker_stop,
                );
                for _ in 0..30 {
                    if *receive.borrow() {
                        break;
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
            let _ = ops::mark_interrupted(&conn);
            key.zeroize();
        });
        Ok(Self {
            stop,
            stop_flag,
            join: Some(join),
            _lock: lock,
        })
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        let _ = self.stop.send(true);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DropAfterPart {
        inner: LocalDestination,
    }

    impl BackupDestination for DropAfterPart {
        fn probe(&mut self) -> Result<(), String> {
            self.inner.probe()
        }

        fn put(&mut self, source: &Path, name: &str) -> Result<(), String> {
            self.inner.put(source, name)?;
            Err("simulated connection drop after partial upload".to_string())
        }

        fn list(&mut self) -> Result<Vec<crate::destinations::RemoteObject>, String> {
            self.inner.list()
        }

        fn get(&mut self, name: &str, output: &Path) -> Result<(), String> {
            self.inner.get(name, output)
        }

        fn delete(&mut self, name: &str) -> Result<(), String> {
            self.inner.delete(name)
        }

        fn rename_no_replace(&mut self, from: &str, to: &str) -> Result<(), String> {
            self.inner.rename_no_replace(from, to)
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("unenverse-ops-{name}-{}", vault_core::new_uuid()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn cleanup(conn: rusqlite::Connection, dir: PathBuf) {
        // Regression: Unix unlinks an open SQLite vault, but Windows keeps it locked.
        drop(conn);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_fake_three_day_clock_coalesces_missed_intervals_into_one_run() {
        let dir = scratch("catchup");
        let db = dir.join("vault.db");
        let salt = dir.join("vault.salt");
        fs::write(&salt, b"0123456789abcdef").unwrap();
        let key = vault_core::derive_key("ops-test-password", &fs::read(&salt).unwrap()).unwrap();
        let conn = open_db(&db, &key).unwrap();
        vault_core::init_schema(&conn).unwrap();
        let (recipient, _) = recovery::generate_identity();
        recovery::set_recipient(&conn, &recipient).unwrap();
        let destination = add_destination(&conn, "test", &dir.join("destination")).unwrap();
        let now = jiff::Timestamp::now();
        // The fake instant is one second short of three whole intervals late:
        // exactly one catch-up run must record the three missed occurrences.
        let due = now
            .checked_sub(Duration::from_secs(3 * 86_400 - 1))
            .unwrap();
        let schedule = ops::add_schedule(
            &conn,
            "schedule",
            "backup.package",
            &destination.id,
            &serde_json::to_string(&ScheduleRule::Interval { seconds: 86_400 }).unwrap(),
            Some(&due.to_string()),
            None,
        )
        .unwrap();
        let paths = RecoveryPaths {
            db_path: db,
            salt_path: salt,
            data_dir: dir.clone(),
            materialisation_log: dir.join("materialisations.jsonl"),
        };
        let lease = ops::acquire_lease(&conn, "test", 1, "test").unwrap();
        let stop = AtomicBool::new(false);
        run_due(&conn, &key, &paths, lease.epoch, now, &stop).unwrap();
        let runs = ops::list_runs(&conn).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].missed_occurrences, 3);
        assert_eq!(runs[0].state, RunState::Succeeded);
        assert_ne!(
            ops::get_schedule(&conn, &schedule.id)
                .unwrap()
                .unwrap()
                .next_due_at,
            schedule.next_due_at
        );
        cleanup(conn, dir);
    }

    #[test]
    fn stale_schedule_creates_a_skipped_record_without_writing_a_package() {
        let dir = scratch("stale");
        let db = dir.join("vault.db");
        let salt = dir.join("vault.salt");
        fs::write(&salt, b"0123456789abcdef").unwrap();
        let key = vault_core::derive_key("ops-test-password", &fs::read(&salt).unwrap()).unwrap();
        let conn = open_db(&db, &key).unwrap();
        vault_core::init_schema(&conn).unwrap();
        let (recipient, _) = recovery::generate_identity();
        recovery::set_recipient(&conn, &recipient).unwrap();
        let destination = add_destination(&conn, "test", &dir.join("destination")).unwrap();
        let now = jiff::Timestamp::now();
        let due = now.checked_sub(Duration::from_secs(10)).unwrap();
        ops::add_schedule(
            &conn,
            "schedule",
            "backup.package",
            &destination.id,
            &serde_json::to_string(&ScheduleRule::Interval { seconds: 60 }).unwrap(),
            Some(&due.to_string()),
            Some(1),
        )
        .unwrap();
        let paths = RecoveryPaths {
            db_path: db,
            salt_path: salt,
            data_dir: dir.clone(),
            materialisation_log: dir.join("materialisations.jsonl"),
        };
        let lease = ops::acquire_lease(&conn, "test", 1, "test").unwrap();
        let stop = AtomicBool::new(false);
        run_due(&conn, &key, &paths, lease.epoch, now, &stop).unwrap();
        let runs = ops::list_runs(&conn).unwrap();
        assert_eq!(runs[0].state, RunState::SkippedStale);
        assert_eq!(
            fs::read_dir(destination.path).unwrap().count(),
            1,
            "only marker remains"
        );
        cleanup(conn, dir);
    }

    #[test]
    fn a_removed_destination_marker_refuses_opening_the_local_transport() {
        let dir = scratch("marker");
        let key = vault_core::derive_key("ops-test-password", b"0123456789abcdef").unwrap();
        let conn = open_db(&dir.join("vault.db"), &key).unwrap();
        vault_core::init_schema(&conn).unwrap();
        let destination = add_destination(&conn, "test", &dir.join("destination")).unwrap();
        fs::remove_file(destination.path.join(DESTINATION_MARKER)).unwrap();
        assert!(open_destination(&conn, &destination).is_err());
        cleanup(conn, dir);
    }

    #[test]
    fn a_second_executor_names_the_existing_holder_instead_of_running_twice() {
        let dir = scratch("lease");
        let db = dir.join("vault.db");
        let salt = dir.join("vault.salt");
        fs::write(&salt, b"0123456789abcdef").unwrap();
        let key = vault_core::derive_key("ops-test-password", &fs::read(&salt).unwrap()).unwrap();
        let conn = open_db(&db, &key).unwrap();
        vault_core::init_schema(&conn).unwrap();
        drop(conn);
        let paths = RecoveryPaths {
            db_path: db,
            salt_path: salt,
            data_dir: dir.clone(),
            materialisation_log: dir.join("materialisations.jsonl"),
        };
        let mut first = Executor::start(key, paths.clone()).unwrap();
        let second = match Executor::start(key, paths) {
            Ok(mut executor) => {
                executor.stop();
                panic!("second executor unexpectedly started");
            }
            Err(error) => error,
        };
        assert!(second.contains("already held"));
        first.stop();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn daily_rule_rejects_unknown_zone_and_bad_time() {
        assert!(next_due(
            &ScheduleRule::Daily {
                at: "30:00".into(),
                time_zone: "UTC".into()
            },
            jiff::Timestamp::now()
        )
        .is_err());
        assert!(next_due(
            &ScheduleRule::Daily {
                at: "10:00".into(),
                time_zone: "Not/AZone".into()
            },
            jiff::Timestamp::now()
        )
        .is_err());
    }

    fn receipt(destination: &Destination, number: u8, created_at: &str) -> PackageReceipt {
        let stem = format!("unenverse-retention-{number:02}");
        PackageReceipt {
            package: PathBuf::from(format!("{stem}.unvpkg")),
            completion: PathBuf::from(format!("{stem}.complete.json")),
            destination_id: destination.id.clone(),
            sha256: "verified-by-write-test".to_string(),
            size: 7,
            manifest: Manifest {
                format_version: 1,
                app_version: "test".to_string(),
                created_at: created_at.to_string(),
                vault_version: None,
                entry_count: 0,
                files: Vec::new(),
            },
        }
    }

    #[test]
    fn retention_never_deletes_the_newest_or_a_foreign_manifest() {
        let dir = scratch("retention");
        let key = vault_core::derive_key("ops-test-password", b"0123456789abcdef").unwrap();
        let conn = open_db(&dir.join("vault.db"), &key).unwrap();
        vault_core::init_schema(&conn).unwrap();
        let destination = add_destination(&conn, "test", &dir.join("destination")).unwrap();
        set_retention_policy(
            &conn,
            &RetentionPolicy {
                daily: 1,
                weekly: 1,
                monthly: 1,
            },
        )
        .unwrap();
        for day in 1..=10 {
            let receipt = receipt(&destination, day, &format!("2026-02-{day:02}T03:00:00Z"));
            fs::write(destination.path.join(&receipt.package), b"package").unwrap();
            atomic_json(&destination.path.join(&receipt.completion), &receipt).unwrap();
        }
        let mut foreign = receipt(&destination, 99, "2026-01-01T03:00:00Z");
        foreign.destination_id = "another-vault-destination".to_string();
        fs::write(destination.path.join(&foreign.package), b"package").unwrap();
        atomic_json(&destination.path.join(&foreign.completion), &foreign).unwrap();

        let removed = enforce_retention_for_conn(&conn, "test", &dir.join("staging")).unwrap();
        assert!(removed > 0);
        // Regression: retention previously selected by filename alone, which
        // could remove another vault's backup or its newest verified package.
        assert!(destination
            .path
            .join("unenverse-retention-10.unvpkg")
            .exists());
        assert!(destination.path.join(&foreign.package).exists());
        assert!(destination.path.join(&foreign.completion).exists());
        cleanup(conn, dir);
    }

    #[test]
    fn a_tampered_restore_drill_fails_in_the_durable_ops_record() {
        let dir = scratch("drill");
        let db = dir.join("vault.db");
        let salt = dir.join("vault.salt");
        fs::write(&salt, b"0123456789abcdef").unwrap();
        let key = vault_core::derive_key("ops-test-password", &fs::read(&salt).unwrap()).unwrap();
        let conn = open_db(&db, &key).unwrap();
        vault_core::init_schema(&conn).unwrap();
        let (recipient, identity) = recovery::generate_identity();
        recovery::set_recipient(&conn, &recipient).unwrap();
        recovery::set_drill_identity(&conn, &identity).unwrap();
        let destination = add_destination(&conn, "test", &dir.join("destination")).unwrap();
        let paths = RecoveryPaths {
            db_path: db,
            salt_path: salt,
            data_dir: dir.clone(),
            materialisation_log: dir.join("materialisations.jsonl"),
        };
        let receipt = package_for_paths(&conn, &key, &paths, &destination.id).unwrap();
        let good = execute_run(
            &conn,
            &key,
            &paths,
            None,
            "backup.drill",
            &destination.id,
            0,
            None,
            None,
        )
        .unwrap();
        assert_eq!(good.state, RunState::Succeeded);
        let package = destination.path.join(&receipt.package);
        let mut bytes = fs::read(&package).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        fs::write(&package, bytes).unwrap();
        let failed = execute_run(
            &conn,
            &key,
            &paths,
            None,
            "backup.drill",
            &destination.id,
            0,
            None,
            None,
        )
        .unwrap();
        assert_eq!(failed.state, RunState::Failed);
        assert_eq!(failed.error_code.as_deref(), Some("drill_failed"));
        cleanup(conn, dir);
    }

    #[test]
    fn a_dropped_upload_leaves_only_a_part_and_the_next_run_cleans_it() {
        let dir = scratch("dropped-upload");
        let db = dir.join("vault.db");
        let salt = dir.join("vault.salt");
        fs::write(&salt, b"0123456789abcdef").unwrap();
        let key = vault_core::derive_key("ops-test-password", &fs::read(&salt).unwrap()).unwrap();
        let conn = open_db(&db, &key).unwrap();
        vault_core::init_schema(&conn).unwrap();
        let (recipient, _) = recovery::generate_identity();
        recovery::set_recipient(&conn, &recipient).unwrap();
        let remote = dir.join("remote");
        fs::create_dir_all(&remote).unwrap();
        let mut drop_after_part = DropAfterPart {
            inner: LocalDestination::new(&remote),
        };
        let result = write_package(
            "test",
            &mut drop_after_part,
            &dir.join("staging"),
            PackageInput {
                conn: &conn,
                key: &key,
                db_path: &db,
                salt_path: &salt,
                auxiliary: Vec::new(),
                app_version: "test",
            },
            None,
        );
        assert!(result.is_err());
        let names = fs::read_dir(&remote)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        // Regression: a connection loss once wrote a completion record after a
        // partial upload, letting retention and restore treat it as complete.
        assert!(names.iter().any(|name| name.ends_with(".unvpkg.part")));
        assert!(!names.iter().any(|name| name.ends_with(".complete.json")));
        let mut retry = LocalDestination::new(&remote);
        assert_eq!(cleanup_stale_parts(&mut retry).unwrap(), 1);
        cleanup(conn, dir);
    }
}
