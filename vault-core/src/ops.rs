// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Durable operations records and their state-machine guard.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Queued,
    WaitingForUnlock,
    Planning,
    AwaitingApproval,
    Running,
    StepDispatching,
    StepDone,
    RetryWait,
    Paused,
    Uncertain,
    NeedsAttention,
    Succeeded,
    Failed,
    SkippedStale,
    Cancelled,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::WaitingForUnlock => "waiting_for_unlock",
            Self::Planning => "planning",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Running => "running",
            Self::StepDispatching => "step_dispatching",
            Self::StepDone => "step_done",
            Self::RetryWait => "retry_wait",
            Self::Paused => "paused",
            Self::Uncertain => "uncertain",
            Self::NeedsAttention => "needs_attention",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::SkippedStale => "skipped_stale",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "queued" => Ok(Self::Queued),
            "waiting_for_unlock" => Ok(Self::WaitingForUnlock),
            "planning" => Ok(Self::Planning),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "running" => Ok(Self::Running),
            "step_dispatching" => Ok(Self::StepDispatching),
            "step_done" => Ok(Self::StepDone),
            "retry_wait" => Ok(Self::RetryWait),
            "paused" => Ok(Self::Paused),
            "uncertain" => Ok(Self::Uncertain),
            "needs_attention" => Ok(Self::NeedsAttention),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "skipped_stale" => Ok(Self::SkippedStale),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Unknown operation state `{value}`")),
        }
    }

    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::SkippedStale | Self::Cancelled
        )
    }
}

const EDGES: &[(RunState, RunState)] = &[
    (RunState::Queued, RunState::WaitingForUnlock),
    (RunState::Queued, RunState::Planning),
    (RunState::Queued, RunState::Cancelled),
    (RunState::WaitingForUnlock, RunState::Planning),
    (RunState::WaitingForUnlock, RunState::Cancelled),
    (RunState::Planning, RunState::Running),
    (RunState::Planning, RunState::AwaitingApproval),
    (RunState::Planning, RunState::Paused),
    (RunState::Planning, RunState::SkippedStale),
    (RunState::Planning, RunState::Cancelled),
    (RunState::AwaitingApproval, RunState::Planning),
    (RunState::AwaitingApproval, RunState::Cancelled),
    (RunState::Running, RunState::StepDispatching),
    (RunState::Running, RunState::RetryWait),
    (RunState::Running, RunState::Paused),
    (RunState::Running, RunState::Uncertain),
    (RunState::Running, RunState::Succeeded),
    (RunState::Running, RunState::Failed),
    (RunState::Running, RunState::Cancelled),
    (RunState::StepDispatching, RunState::StepDone),
    (RunState::StepDispatching, RunState::Uncertain),
    (RunState::StepDispatching, RunState::Paused),
    (RunState::StepDone, RunState::Running),
    (RunState::Uncertain, RunState::StepDone),
    (RunState::Uncertain, RunState::Running),
    (RunState::Uncertain, RunState::NeedsAttention),
    (RunState::RetryWait, RunState::Planning),
    (RunState::Paused, RunState::Planning),
    (RunState::NeedsAttention, RunState::Planning),
];

pub fn can_transition(from: RunState, to: RunState) -> bool {
    EDGES.contains(&(from, to))
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ops_schedules (
             id TEXT PRIMARY KEY, kind TEXT NOT NULL, target TEXT NOT NULL, rule TEXT NOT NULL,
             enabled INTEGER NOT NULL, policy_rev INTEGER NOT NULL, next_due_at TEXT,
             last_run_id TEXT, max_lateness_secs INTEGER, created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS ops_runs (
             id TEXT PRIMARY KEY, schedule_id TEXT, kind TEXT NOT NULL, state TEXT NOT NULL,
             plan TEXT NOT NULL, plan_hash TEXT NOT NULL, input_revs TEXT NOT NULL,
             policy_rev INTEGER NOT NULL, plugin_provenance TEXT,
             missed_occurrences INTEGER NOT NULL DEFAULT 0, attempt INTEGER NOT NULL DEFAULT 0,
             next_attempt_at TEXT, lease_epoch INTEGER, created_at TEXT NOT NULL,
             started_at TEXT, finished_at TEXT, outcome TEXT, error_code TEXT, error_msg TEXT
         );
         CREATE INDEX IF NOT EXISTS ops_runs_schedule ON ops_runs(schedule_id, created_at DESC);
         CREATE TABLE IF NOT EXISTS ops_steps (
             run_id TEXT NOT NULL, seq INTEGER NOT NULL, kind TEXT NOT NULL, target TEXT NOT NULL,
             state TEXT NOT NULL, idempotency_key TEXT NOT NULL, issuer_ref TEXT, output TEXT,
             attempt INTEGER NOT NULL, started_at TEXT, finished_at TEXT,
             PRIMARY KEY (run_id, seq)
         );
         CREATE TABLE IF NOT EXISTS ops_lease (
             id INTEGER PRIMARY KEY CHECK (id = 1), holder TEXT NOT NULL, pid INTEGER NOT NULL,
             host TEXT NOT NULL, epoch INTEGER NOT NULL, acquired_at TEXT NOT NULL,
             heartbeat_at TEXT NOT NULL
         );",
    )
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct Schedule {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub rule: String,
    pub enabled: bool,
    pub policy_rev: i64,
    pub next_due_at: Option<String>,
    pub last_run_id: Option<String>,
    pub max_lateness_secs: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

fn schedule_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Schedule> {
    Ok(Schedule {
        id: row.get(0)?,
        kind: row.get(1)?,
        target: row.get(2)?,
        rule: row.get(3)?,
        enabled: row.get::<_, i64>(4)? != 0,
        policy_rev: row.get(5)?,
        next_due_at: row.get(6)?,
        last_run_id: row.get(7)?,
        max_lateness_secs: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub fn add_schedule(
    conn: &Connection,
    id: &str,
    kind: &str,
    target: &str,
    rule: &str,
    next_due_at: Option<&str>,
    max_lateness_secs: Option<i64>,
) -> Result<Schedule, String> {
    let now = crate::iso_now();
    conn.execute(
        "INSERT INTO ops_schedules (id, kind, target, rule, enabled, policy_rev, next_due_at, max_lateness_secs, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, 1, ?5, ?6, ?7, ?7)",
        params![id, kind, target, rule, next_due_at, max_lateness_secs, now],
    ).map_err(|e| e.to_string())?;
    get_schedule(conn, id)?.ok_or_else(|| "Schedule was not stored".to_string())
}

pub fn get_schedule(conn: &Connection, id: &str) -> Result<Option<Schedule>, String> {
    conn.query_row(
        "SELECT id, kind, target, rule, enabled, policy_rev, next_due_at, last_run_id, max_lateness_secs, created_at, updated_at FROM ops_schedules WHERE id = ?1",
        [id], schedule_row,
    ).optional().map_err(|e| e.to_string())
}

pub fn list_schedules(conn: &Connection) -> Result<Vec<Schedule>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, target, rule, enabled, policy_rev, next_due_at, last_run_id, max_lateness_secs, created_at, updated_at FROM ops_schedules ORDER BY created_at DESC",
    ).map_err(|e| e.to_string())?;
    let schedules = stmt
        .query_map([], schedule_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(schedules)
}

pub fn update_schedule_due(
    conn: &Connection,
    id: &str,
    next_due_at: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE ops_schedules SET next_due_at = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, next_due_at, crate::iso_now()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Records the run and advances the due time atomically, avoiding replay after
/// a process stops between those otherwise separate writes.
pub fn complete_schedule_occurrence(
    conn: &Connection,
    id: &str,
    run_id: &str,
    next_due_at: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE ops_schedules SET last_run_id = ?2, next_due_at = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, run_id, next_due_at, crate::iso_now()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn assert_epoch(conn: &Connection, epoch: i64) -> Result<(), String> {
    let held = holder(conn)?
        .ok_or_else(|| "No operations lease is held".to_string())?
        .epoch;
    if held != epoch {
        return Err("Operations lease changed; refusing stale worker write".to_string());
    }
    Ok(())
}

pub fn complete_schedule_occurrence_fenced(
    conn: &Connection,
    id: &str,
    run_id: &str,
    next_due_at: Option<&str>,
    epoch: i64,
) -> Result<(), String> {
    assert_epoch(conn, epoch)?;
    complete_schedule_occurrence(conn, id, run_id, next_due_at)
}

#[derive(Debug, Clone, Serialize)]
pub struct Run {
    pub id: String,
    pub schedule_id: Option<String>,
    pub kind: String,
    pub state: RunState,
    pub missed_occurrences: i64,
    pub lease_epoch: Option<i64>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub outcome: Option<String>,
    pub error_code: Option<String>,
    pub error_msg: Option<String>,
}

fn run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    let state: String = row.get(3)?;
    Ok(Run {
        id: row.get(0)?,
        schedule_id: row.get(1)?,
        kind: row.get(2)?,
        state: RunState::parse(&state).map_err(|_| rusqlite::Error::InvalidQuery)?,
        missed_occurrences: row.get(4)?,
        lease_epoch: row.get(5)?,
        created_at: row.get(6)?,
        started_at: row.get(7)?,
        finished_at: row.get(8)?,
        outcome: row.get(9)?,
        error_code: row.get(10)?,
        error_msg: row.get(11)?,
    })
}

const RUN_COLUMNS: &str = "id, schedule_id, kind, state, missed_occurrences, lease_epoch, created_at, started_at, finished_at, outcome, error_code, error_msg";

pub fn create_run(
    conn: &Connection,
    id: &str,
    schedule: Option<&Schedule>,
    missed_occurrences: i64,
    lease_epoch: Option<i64>,
) -> Result<Run, String> {
    create_run_with_kind(
        conn,
        id,
        schedule,
        "backup.package",
        missed_occurrences,
        lease_epoch,
    )
}

/// Creates a manual or scheduled run with an explicit operation kind.  A
/// schedule remains authoritative when present; the explicit kind is used only
/// for manually-triggered operations such as a restore drill.
pub fn create_run_with_kind(
    conn: &Connection,
    id: &str,
    schedule: Option<&Schedule>,
    manual_kind: &str,
    missed_occurrences: i64,
    lease_epoch: Option<i64>,
) -> Result<Run, String> {
    let (schedule_id, kind, policy_rev) = match schedule {
        Some(s) => (Some(s.id.as_str()), s.kind.as_str(), s.policy_rev),
        None => (None, manual_kind, 1),
    };
    let now = crate::iso_now();
    conn.execute(
        "INSERT INTO ops_runs (id, schedule_id, kind, state, plan, plan_hash, input_revs, policy_rev, missed_occurrences, lease_epoch, created_at)
         VALUES (?1, ?2, ?3, 'queued', '{}', '', '{}', ?4, ?5, ?6, ?7)",
        params![id, schedule_id, kind, policy_rev, missed_occurrences, lease_epoch, now],
    ).map_err(|e| e.to_string())?;
    get_run(conn, id)?.ok_or_else(|| "Run was not stored".to_string())
}

pub fn create_run_fenced(
    conn: &Connection,
    id: &str,
    schedule: Option<&Schedule>,
    missed_occurrences: i64,
    epoch: i64,
) -> Result<Run, String> {
    assert_epoch(conn, epoch)?;
    create_run(conn, id, schedule, missed_occurrences, Some(epoch))
}

/// Records which plugin programs a run used, as `{ "<plugin>": "<sha256 of the program>" }`
/// (R14). The hash is the install-time provenance re-checked at start, so a run row says
/// which build of a plugin did the work even after the plugin is upgraded or removed.
/// A second call for the same plugin replaces its hash; other plugins are kept.
pub fn record_plugin_provenance(
    conn: &Connection,
    run_id: &str,
    plugin: &str,
    hash: &str,
) -> Result<(), String> {
    let current: Option<String> = conn
        .query_row(
            "SELECT plugin_provenance FROM ops_runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No run {run_id}"))?;
    let mut map: serde_json::Map<String, serde_json::Value> = match current {
        Some(t) if !t.is_empty() => serde_json::from_str(&t).map_err(|e| e.to_string())?,
        _ => Default::default(),
    };
    map.insert(plugin.into(), hash.into());
    conn.execute(
        "UPDATE ops_runs SET plugin_provenance = ?2 WHERE id = ?1",
        params![run_id, serde_json::Value::Object(map).to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// The plugin hashes recorded for a run, empty when it used no plugin.
pub fn plugin_provenance(
    conn: &Connection,
    run_id: &str,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let text: Option<String> = conn
        .query_row(
            "SELECT plugin_provenance FROM ops_runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No run {run_id}"))?;
    match text {
        Some(t) if !t.is_empty() => serde_json::from_str(&t).map_err(|e| e.to_string()),
        _ => Ok(Default::default()),
    }
}

pub fn get_run(conn: &Connection, id: &str) -> Result<Option<Run>, String> {
    conn.query_row(
        &format!("SELECT {RUN_COLUMNS} FROM ops_runs WHERE id = ?1"),
        [id],
        run_row,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn list_runs(conn: &Connection) -> Result<Vec<Run>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM ops_runs ORDER BY created_at DESC"
        ))
        .map_err(|e| e.to_string())?;
    let runs = stmt
        .query_map([], run_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(runs)
}

pub fn transition_run(
    conn: &Connection,
    id: &str,
    to: RunState,
    outcome: Option<&str>,
    error: Option<(&str, &str)>,
) -> Result<Run, String> {
    let current = get_run(conn, id)?.ok_or_else(|| format!("No such operation `{id}`"))?;
    if !can_transition(current.state, to) {
        return Err(format!(
            "Illegal operation transition {} -> {}",
            current.state.as_str(),
            to.as_str()
        ));
    }
    let now = crate::iso_now();
    let started = if to == RunState::Running && current.started_at.is_none() {
        Some(now.as_str())
    } else {
        None
    };
    let finished = if to.terminal() {
        Some(now.as_str())
    } else {
        None
    };
    let (error_code, error_msg) = error.map_or((None, None), |(code, msg)| (Some(code), Some(msg)));
    conn.execute(
        "UPDATE ops_runs SET state = ?2, started_at = COALESCE(?3, started_at), finished_at = COALESCE(?4, finished_at), outcome = COALESCE(?5, outcome), error_code = ?6, error_msg = ?7 WHERE id = ?1",
        params![id, to.as_str(), started, finished, outcome, error_code, error_msg],
    ).map_err(|e| e.to_string())?;
    get_run(conn, id)?.ok_or_else(|| "Run disappeared".to_string())
}

/// Refuses a mutation from an executor whose lease has been superseded.
pub fn transition_run_fenced(
    conn: &Connection,
    id: &str,
    epoch: i64,
    to: RunState,
    outcome: Option<&str>,
    error: Option<(&str, &str)>,
) -> Result<Run, String> {
    assert_epoch(conn, epoch)?;
    transition_run(conn, id, to, outcome, error)
}

#[derive(Debug, Clone, Serialize)]
pub struct Lease {
    pub holder: String,
    pub epoch: i64,
}

pub fn acquire_lease(
    conn: &Connection,
    holder: &str,
    pid: i64,
    host: &str,
) -> Result<Lease, String> {
    let now = crate::iso_now();
    conn.execute(
        "INSERT OR IGNORE INTO ops_lease (id, holder, pid, host, epoch, acquired_at, heartbeat_at) VALUES (1, ?1, ?2, ?3, 0, ?4, ?4)",
        params![holder, pid, host, now],
    ).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE ops_lease SET holder = ?1, pid = ?2, host = ?3, epoch = epoch + 1, acquired_at = ?4, heartbeat_at = ?4 WHERE id = 1",
        params![holder, pid, host, now],
    ).map_err(|e| e.to_string())?;
    let epoch = conn
        .query_row("SELECT epoch FROM ops_lease WHERE id = 1", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    Ok(Lease {
        holder: holder.to_string(),
        epoch,
    })
}

pub fn holder(conn: &Connection) -> Result<Option<Lease>, String> {
    conn.query_row(
        "SELECT holder, epoch FROM ops_lease WHERE id = 1",
        [],
        |row| {
            Ok(Lease {
                holder: row.get(0)?,
                epoch: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn mark_interrupted(conn: &Connection) -> Result<(), String> {
    conn.execute("UPDATE ops_runs SET state = 'paused' WHERE state IN ('queued', 'planning', 'running', 'retry_wait')", [])
        .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE ops_steps SET state = 'uncertain' WHERE state = 'dispatching'",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Deployments (R12) ───────────────────────────────────────────────────────────────────────────
//
// One run per (node, target, bytes): created when the hub holds a push for approval or sends it,
// settled when the node reports the result or when a later beat shows the bytes already on disk.
// A lock or a crash leaves the run paused and its step uncertain; the next beat decides it, so a
// push whose result was lost is never recorded twice and never forgotten.

/// The kind of a run that follows one file being sent to a node.
pub const DEPLOY_KIND: &str = "deploy.push";

/// Failed applies of the same bytes before the run stops retrying and fails.
pub const DEPLOY_MAX_ATTEMPTS: i64 = 5;

/// Open deployment runs older than this many days are cancelled, so a push nobody approved does not
/// stay open forever.
pub const DEPLOY_EXPIRY_DAYS: i64 = 7;

fn deploy_key(node_id: &str, target: &str, sha256: &str) -> String {
    format!("{node_id}|{target}|{sha256}")
}

fn open_deploys(
    conn: &Connection,
    node_id: &str,
    target: &str,
) -> Result<Vec<(Run, String)>, String> {
    let prefix = format!("{node_id}|{target}|");
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {RUN_COLUMNS}, plan_hash FROM ops_runs WHERE kind = ?1 \
             AND substr(plan_hash, 1, length(?2)) = ?2 \
             AND state NOT IN ('succeeded', 'failed', 'skipped_stale', 'cancelled') \
             ORDER BY created_at"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![DEPLOY_KIND, prefix], |row| {
            Ok((run_row(row)?, row.get::<_, String>(12)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Moves a run to `Running` along legal edges, whatever it was waiting in.
fn walk_to_running(conn: &Connection, run: &Run) -> Result<(), String> {
    use RunState::*;
    let path: &[RunState] = match run.state {
        Running => &[],
        Queued => &[Planning, Running],
        Planning => &[Running],
        AwaitingApproval | Paused | RetryWait | NeedsAttention => &[Planning, Running],
        Uncertain | StepDone => &[Running],
        StepDispatching => &[StepDone, Running],
        other => return Err(format!("operation is already {}", other.as_str())),
    };
    for to in path {
        transition_run(conn, &run.id, *to, None, None)?;
    }
    Ok(())
}

fn set_step(conn: &Connection, run_id: &str, state: &str, output: Option<&str>) {
    let _ = conn.execute(
        "UPDATE ops_steps SET state = ?2, output = COALESCE(?3, output), finished_at = ?4 \
         WHERE run_id = ?1 AND seq = 0",
        params![run_id, state, output, crate::iso_now()],
    );
}

fn close_deploy(
    conn: &Connection,
    run: &Run,
    to: RunState,
    outcome: &str,
    error: Option<(&str, &str)>,
) -> Result<(), String> {
    if run.state != RunState::AwaitingApproval {
        walk_to_running(conn, run)?;
    }
    transition_run(conn, &run.id, to, Some(outcome), error)?;
    Ok(())
}

/// Closes open deployment runs that have waited longer than [`DEPLOY_EXPIRY_DAYS`].
fn expire_deploys(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM ops_runs WHERE kind = ?1 \
             AND state NOT IN ('succeeded', 'failed', 'skipped_stale', 'cancelled') \
             AND julianday('now') - julianday(created_at) > ?2"
        ))
        .map_err(|e| e.to_string())?;
    let old = stmt
        .query_map(params![DEPLOY_KIND, DEPLOY_EXPIRY_DAYS], run_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for run in old {
        close_deploy(
            conn,
            &run,
            RunState::Cancelled,
            "expired without a result",
            None,
        )?;
    }
    Ok(())
}

/// A push of `sha256` to `target` on a node is held for a person (`held`) or has just been sent.
/// Returns the run's id. The same bytes reuse the open run; other bytes for the same target
/// supersede it. Sending moves the run to `Running` and its one step to `dispatching`.
pub fn deploy_dispatch(
    conn: &Connection,
    node_id: &str,
    node_name: &str,
    target: &str,
    sha256: &str,
    held: bool,
) -> Result<String, String> {
    init_schema(conn)?;
    expire_deploys(conn)?;
    let key = deploy_key(node_id, target, sha256);
    let mut current: Option<Run> = None;
    for (run, k) in open_deploys(conn, node_id, target)? {
        if k == key {
            current = Some(run);
        } else {
            close_deploy(
                conn,
                &run,
                RunState::Cancelled,
                "superseded by newer bytes",
                None,
            )?;
        }
    }
    let run = match current {
        Some(r) => r,
        None => {
            let id = crate::new_uuid();
            create_run_with_kind(conn, &id, None, DEPLOY_KIND, 0, None)?;
            let plan = serde_json::json!({
                "node": node_id, "node_name": node_name, "target": target, "sha256": sha256,
            })
            .to_string();
            conn.execute(
                "UPDATE ops_runs SET plan = ?2, plan_hash = ?3 WHERE id = ?1",
                params![id, plan, key],
            )
            .map_err(|e| e.to_string())?;
            transition_run(conn, &id, RunState::Planning, None, None)?;
            if held {
                transition_run(conn, &id, RunState::AwaitingApproval, None, None)?;
            }
            get_run(conn, &id)?.ok_or_else(|| "Run was not stored".to_string())?
        }
    };
    if held {
        return Ok(run.id);
    }
    walk_to_running(conn, &run)?;
    conn.execute(
        "INSERT INTO ops_steps (run_id, seq, kind, target, state, idempotency_key, attempt, started_at) \
         VALUES (?1, 0, 'node.apply', ?2, 'dispatching', ?3, 1, ?4) \
         ON CONFLICT(run_id, seq) DO UPDATE SET state = 'dispatching', attempt = attempt + 1, \
         finished_at = NULL",
        params![run.id, format!("{node_name}/{target}"), sha256, crate::iso_now()],
    )
    .map_err(|e| e.to_string())?;
    Ok(run.id)
}

/// The node reported how applying `sha256` went. A success ends the run; a failure leaves it in
/// `RetryWait` until [`DEPLOY_MAX_ATTEMPTS`] sends have failed, then fails it. A result for bytes no
/// run is following (a push from before runs existed) is ignored.
pub fn deploy_result(
    conn: &Connection,
    node_id: &str,
    target: &str,
    sha256: &str,
    ok: bool,
    error: Option<&str>,
) -> Result<(), String> {
    init_schema(conn)?;
    let key = deploy_key(node_id, target, sha256);
    for (run, k) in open_deploys(conn, node_id, target)? {
        if k != key {
            continue;
        }
        if ok {
            set_step(conn, &run.id, "done", Some("applied"));
            close_deploy(conn, &run, RunState::Succeeded, "applied", None)?;
            continue;
        }
        let msg = error.unwrap_or("not applied");
        let attempts: i64 = conn
            .query_row(
                "SELECT attempt FROM ops_steps WHERE run_id = ?1 AND seq = 0",
                [&run.id],
                |r| r.get(0),
            )
            .unwrap_or(1);
        set_step(conn, &run.id, "failed", Some(msg));
        if attempts >= DEPLOY_MAX_ATTEMPTS {
            close_deploy(
                conn,
                &run,
                RunState::Failed,
                "failed",
                Some(("apply_failed", msg)),
            )?;
        } else {
            walk_to_running(conn, &run)?;
            transition_run(
                conn,
                &run.id,
                RunState::RetryWait,
                None,
                Some(("apply_failed", msg)),
            )?;
        }
    }
    Ok(())
}

/// The node says which bytes `target` holds now. A run for exactly those bytes succeeded even if its
/// result never arrived (a lock, a crash, a lost reply); the interrupted step is settled here.
pub fn deploy_observed(
    conn: &Connection,
    node_id: &str,
    target: &str,
    current_sha256: Option<&str>,
) -> Result<(), String> {
    let Some(current) = current_sha256 else {
        return Ok(());
    };
    init_schema(conn)?;
    let key = deploy_key(node_id, target, current);
    for (run, k) in open_deploys(conn, node_id, target)? {
        if k == key {
            set_step(conn, &run.id, "done", Some("found on the node"));
            close_deploy(conn, &run, RunState::Succeeded, "found on the node", None)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_drawn_edge_is_allowed_and_every_other_edge_is_refused() {
        let states = [
            RunState::Queued,
            RunState::WaitingForUnlock,
            RunState::Planning,
            RunState::AwaitingApproval,
            RunState::Running,
            RunState::StepDispatching,
            RunState::StepDone,
            RunState::RetryWait,
            RunState::Paused,
            RunState::Uncertain,
            RunState::NeedsAttention,
            RunState::Succeeded,
            RunState::Failed,
            RunState::SkippedStale,
            RunState::Cancelled,
        ];
        for from in states {
            for to in states {
                assert_eq!(
                    can_transition(from, to),
                    EDGES.contains(&(from, to)),
                    "{} -> {}",
                    from.as_str(),
                    to.as_str()
                );
            }
        }
    }

    #[test]
    fn a_run_remembers_which_plugin_build_it_used() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        create_run_with_kind(&conn, "r1", None, "backup.package", 0, None).unwrap();
        assert!(plugin_provenance(&conn, "r1").unwrap().is_empty());
        record_plugin_provenance(&conn, "r1", "ntfy", "aa11").unwrap();
        record_plugin_provenance(&conn, "r1", "other", "bb22").unwrap();
        // The plugin is upgraded mid-history: the later hash replaces its own, not the other's.
        record_plugin_provenance(&conn, "r1", "ntfy", "cc33").unwrap();
        let got = plugin_provenance(&conn, "r1").unwrap();
        assert_eq!(got["ntfy"], "cc33");
        assert_eq!(got["other"], "bb22");
        assert!(record_plugin_provenance(&conn, "missing", "ntfy", "x").is_err());
    }

    #[test]
    fn a_run_with_an_empty_or_unreadable_plugin_record_is_handled() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        create_run_with_kind(&conn, "r1", None, "backup.package", 0, None).unwrap();
        // An empty string reads as "no plugin" and is replaced by the first record.
        conn.execute(
            "UPDATE ops_runs SET plugin_provenance = '' WHERE id = 'r1'",
            [],
        )
        .unwrap();
        assert!(plugin_provenance(&conn, "r1").unwrap().is_empty());
        record_plugin_provenance(&conn, "r1", "ntfy", "aa11").unwrap();
        assert_eq!(plugin_provenance(&conn, "r1").unwrap()["ntfy"], "aa11");
        // A damaged record is an error, never silently overwritten.
        conn.execute(
            "UPDATE ops_runs SET plugin_provenance = 'not json' WHERE id = 'r1'",
            [],
        )
        .unwrap();
        assert!(plugin_provenance(&conn, "r1").is_err());
        assert!(record_plugin_provenance(&conn, "r1", "ntfy", "bb22").is_err());
        assert!(plugin_provenance(&conn, "missing").is_err());
    }

    fn deploy_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    fn deploy_state(conn: &Connection, id: &str) -> RunState {
        get_run(conn, id).unwrap().unwrap().state
    }

    #[test]
    fn a_deployment_run_follows_one_push_to_its_result() {
        let c = deploy_conn();
        let id = deploy_dispatch(&c, "n1", "edge", "env", "aa", false).unwrap();
        assert_eq!(deploy_state(&c, &id), RunState::Running);
        // The node asks again before it has answered: the same run, one more send.
        assert_eq!(
            deploy_dispatch(&c, "n1", "edge", "env", "aa", false).unwrap(),
            id
        );
        let attempt: i64 = c
            .query_row(
                "SELECT attempt FROM ops_steps WHERE run_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(attempt, 2);
        deploy_result(&c, "n1", "env", "aa", true, None).unwrap();
        assert_eq!(deploy_state(&c, &id), RunState::Succeeded);
        let step: String = c
            .query_row(
                "SELECT state FROM ops_steps WHERE run_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(step, "done");
        // A result nobody follows is ignored, not an error.
        deploy_result(&c, "n1", "env", "zz", true, None).unwrap();
    }

    #[test]
    fn a_held_push_waits_then_is_the_same_run_when_it_is_sent() {
        let c = deploy_conn();
        let id = deploy_dispatch(&c, "n1", "edge", "env", "aa", true).unwrap();
        assert_eq!(deploy_state(&c, &id), RunState::AwaitingApproval);
        assert_eq!(
            deploy_dispatch(&c, "n1", "edge", "env", "aa", true).unwrap(),
            id
        );
        assert_eq!(
            deploy_dispatch(&c, "n1", "edge", "env", "aa", false).unwrap(),
            id
        );
        assert_eq!(deploy_state(&c, &id), RunState::Running);
    }

    #[test]
    fn new_bytes_for_a_target_cancel_the_older_run() {
        let c = deploy_conn();
        let old = deploy_dispatch(&c, "n1", "edge", "env", "aa", true).unwrap();
        let sent = deploy_dispatch(&c, "n1", "edge", "env", "bb", false).unwrap();
        assert_ne!(old, sent);
        assert_eq!(deploy_state(&c, &old), RunState::Cancelled);
        assert_eq!(deploy_state(&c, &sent), RunState::Running);
        // Another target on the same node is untouched.
        let other = deploy_dispatch(&c, "n1", "edge", "web", "aa", false).unwrap();
        assert_eq!(deploy_state(&c, &sent), RunState::Running);
        assert_eq!(deploy_state(&c, &other), RunState::Running);
    }

    #[test]
    fn a_lock_mid_push_is_settled_by_what_the_node_holds_next() {
        // The bug this pins: a push interrupted by a lock was left paused with an uncertain step for
        // ever, and the result that arrived after unlock found nothing to settle.
        let c = deploy_conn();
        let id = deploy_dispatch(&c, "n1", "edge", "env", "aa", false).unwrap();
        mark_interrupted(&c).unwrap();
        assert_eq!(deploy_state(&c, &id), RunState::Paused);
        let step: String = c
            .query_row(
                "SELECT state FROM ops_steps WHERE run_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(step, "uncertain");
        // The node holds other bytes: nothing is decided, the push is sent again as the same run.
        deploy_observed(&c, "n1", "env", Some("old")).unwrap();
        assert_eq!(deploy_state(&c, &id), RunState::Paused);
        assert_eq!(
            deploy_dispatch(&c, "n1", "edge", "env", "aa", false).unwrap(),
            id
        );
        assert_eq!(deploy_state(&c, &id), RunState::Running);
        // Locked again, and this time the bytes are already on the node.
        mark_interrupted(&c).unwrap();
        deploy_observed(&c, "n1", "env", Some("aa")).unwrap();
        assert_eq!(deploy_state(&c, &id), RunState::Succeeded);
        deploy_observed(&c, "n1", "env", None).unwrap();
    }

    #[test]
    fn a_push_that_keeps_failing_retries_and_then_fails() {
        let c = deploy_conn();
        let mut id = String::new();
        for n in 1..=DEPLOY_MAX_ATTEMPTS {
            id = deploy_dispatch(&c, "n1", "edge", "env", "aa", false).unwrap();
            deploy_result(&c, "n1", "env", "aa", false, Some("disk full")).unwrap();
            let want = if n < DEPLOY_MAX_ATTEMPTS {
                RunState::RetryWait
            } else {
                RunState::Failed
            };
            assert_eq!(deploy_state(&c, &id), want, "after send {n}");
        }
        let run = get_run(&c, &id).unwrap().unwrap();
        assert_eq!(run.error_code.as_deref(), Some("apply_failed"));
        assert_eq!(run.error_msg.as_deref(), Some("disk full"));
    }

    #[test]
    fn an_old_open_deployment_is_cancelled_the_next_time_one_is_sent() {
        let c = deploy_conn();
        let old = deploy_dispatch(&c, "n1", "edge", "env", "aa", true).unwrap();
        c.execute(
            "UPDATE ops_runs SET created_at = '2020-01-01T00:00:00.000Z' WHERE id = ?1",
            [&old],
        )
        .unwrap();
        deploy_dispatch(&c, "n2", "other", "env", "bb", false).unwrap();
        assert_eq!(deploy_state(&c, &old), RunState::Cancelled);
    }
}
