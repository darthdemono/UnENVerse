//! Local recovery destinations, package runs and the unlocked scheduler.
//!
//! The database holds durable intent and state in `vault_core::ops`; this
//! module owns the local filesystem protocol and the process lifetime.

use crate::access::{self, Access};
use crate::error::{CliError, CliResult};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Destination {
    pub id: String,
    pub path: PathBuf,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageReceipt {
    pub package: PathBuf,
    pub completion: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub manifest: Manifest,
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

pub fn cleanup_stale_parts(destination: &Destination) -> Result<usize, String> {
    require_marker(destination)?;
    let mut removed = 0;
    for entry in fs::read_dir(&destination.path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .ends_with(".unvpkg.part")
        {
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
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
    destination: &Destination,
    input: PackageInput<'_>,
    stop: Option<&AtomicBool>,
) -> Result<PackageReceipt, String> {
    require_marker(destination)?;
    cleanup_stale_parts(destination)?;
    let stem = format!(
        "unenverse-{}-{}",
        vault_core::iso_now().replace(':', "-"),
        vault_core::new_uuid()
    );
    let package = destination.path.join(format!("{stem}.unvpkg"));
    let part = destination.path.join(format!("{stem}.unvpkg.part"));
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&part)
        .map_err(|e| e.to_string())?;
    let written = match stop {
        Some(stop) => recovery::write_package(input, InterruptibleWriter { file: &file, stop }),
        None => recovery::write_package(input, &file),
    };
    let manifest = match written {
        Ok(manifest) => manifest,
        Err(error) => {
            drop(file);
            let _ = fs::remove_file(&part);
            return Err(error);
        }
    };
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(&part, &package).map_err(|e| e.to_string())?;
    sync_dir(&destination.path)?;
    let (sha256, size) = sha256_file(&package)?;
    let completion = destination.path.join(format!("{stem}.complete.json"));
    let receipt = PackageReceipt {
        package: package.clone(),
        completion: completion.clone(),
        sha256,
        size,
        manifest,
    };
    atomic_json(&completion, &receipt)?;
    let read_back: PackageReceipt =
        serde_json::from_slice(&fs::read(&completion).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Read completion record: {e}"))?;
    let (actual, actual_size) = sha256_file(&package)?;
    if read_back.sha256 != actual || read_back.size != actual_size {
        return Err("Recovery package receipt did not verify after write".to_string());
    }
    Ok(receipt)
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
    write_package(
        &destination,
        PackageInput {
            conn,
            key,
            db_path: &paths.db_path,
            salt_path: &paths.salt_path,
            auxiliary: paths.auxiliary(),
            app_version: env!("CARGO_PKG_VERSION"),
        },
        stop,
    )
}

pub fn create_recovery_identity(access: &Access) -> CliResult<(String, String)> {
    let conn = access.conn()?;
    let (recipient, identity) = recovery::generate_identity();
    recovery::set_recipient(&conn, &recipient).map_err(CliError::from)?;
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
    Interval { seconds: u64 },
    Daily { at: String, time_zone: String },
}

fn timestamp(text: &str) -> Result<jiff::Timestamp, String> {
    text.parse()
        .map_err(|e| format!("Invalid schedule timestamp `{text}`: {e}"))
}

fn next_due(rule: &ScheduleRule, after: jiff::Timestamp) -> Result<jiff::Timestamp, String> {
    match rule {
        ScheduleRule::Interval { seconds } if *seconds > 0 => after
            .checked_add(Duration::from_secs(*seconds))
            .map_err(|e| e.to_string()),
        ScheduleRule::Interval { .. } => Err("Schedule interval must be positive".to_string()),
        ScheduleRule::Daily { at, time_zone } => {
            let (hour, minute) = at.split_once(':').ok_or("Daily time must be HH:MM")?;
            let hour = hour.parse::<i8>().map_err(|_| "Daily time must be HH:MM")?;
            let minute = minute
                .parse::<i8>()
                .map_err(|_| "Daily time must be HH:MM")?;
            if !(0..24).contains(&hour) || !(0..60).contains(&minute) {
                return Err("Daily time must be HH:MM".to_string());
            }
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
    }
}

pub fn add_schedule_for_conn(
    conn: &vault_core::SqlConnection,
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
        "backup.package",
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

fn execute_run(
    conn: &vault_core::SqlConnection,
    key: &VaultKey,
    paths: &RecoveryPaths,
    schedule: Option<&Schedule>,
    destination: &str,
    missed: i64,
    epoch: Option<i64>,
    stop: Option<&AtomicBool>,
) -> Result<Run, String> {
    let id = vault_core::new_uuid();
    match epoch {
        Some(epoch) => ops::create_run_fenced(conn, &id, schedule, missed, epoch)?,
        None => ops::create_run(conn, &id, schedule, missed, None)?,
    };
    let transition = |to, outcome, error| match epoch {
        Some(e) => ops::transition_run_fenced(conn, &id, e, to, outcome, error),
        None => ops::transition_run(conn, &id, to, outcome, error),
    };
    transition(RunState::Planning, None, None)?;
    transition(RunState::Running, None, None)?;
    match package_for_paths_stoppable(conn, key, paths, destination, stop) {
        Ok(receipt) => transition(
            RunState::Succeeded,
            Some(&receipt.package.display().to_string()),
            None,
        ),
        Err(error) if stop.is_some_and(|flag| flag.load(Ordering::Acquire)) => {
            transition(RunState::Paused, Some("stopped"), Some(("stopped", &error)))
        }
        Err(error) => transition(RunState::Failed, None, Some(("backup_failed", &error))),
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
        destination_id,
        0,
        None,
        None,
    )
    .map_err(CliError::from)
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
        };
        let run = execute_run(
            conn,
            key,
            paths,
            Some(&schedule),
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

    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("unenverse-ops-{name}-{}", vault_core::new_uuid()));
        fs::create_dir_all(&path).unwrap();
        path
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
        fs::remove_dir_all(dir).unwrap();
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
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_removed_destination_marker_refuses_cleanup_or_write() {
        let dir = scratch("marker");
        let key = vault_core::derive_key("ops-test-password", b"0123456789abcdef").unwrap();
        let conn = open_db(&dir.join("vault.db"), &key).unwrap();
        vault_core::init_schema(&conn).unwrap();
        let destination = add_destination(&conn, "test", &dir.join("destination")).unwrap();
        fs::remove_file(destination.path.join(DESTINATION_MARKER)).unwrap();
        assert!(cleanup_stale_parts(&destination).is_err());
        fs::remove_dir_all(dir).unwrap();
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
}
