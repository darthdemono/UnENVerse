// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
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
}
