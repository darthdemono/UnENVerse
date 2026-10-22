// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The rotation state machine (R13): rotate a credential at its issuer, deploy it to the things that
//! use it, check them, and revoke the old credential only when that is safe.
//!
//! The engine owns the order of steps and the safety rules; it knows nothing about any one issuer or
//! deployment. Those arrive as traits ([`IssuerConnector`], [`Consumers`], [`EntryStore`]), so the
//! same state machine runs against PostgreSQL, Cloudflare or a fake that injects faults, and the
//! properties below are tested against the fake.
//!
//! The rules, in the order they matter:
//!
//! 1. **The old credential stays valid until the end.** It is revoked last, and only when the revoke
//!    rule is met: by default every required consumer has been deployed to and has passed its check
//!    in this run ([`RevokeRule::AfterAllChecked`]).
//! 2. **A value the client chose is stored before the issuer hears of it** (`rotation_pending`),
//!    so a lost response never leaves a credential at the issuer that nobody has.
//! 3. **No blind retry of anything destructive.** A response lost after a destructive call is
//!    `uncertain`; the connector is asked what happened ([`IssuerConnector::reconcile`]) and the
//!    step is repeated only when the answer is "not done". An in-place roll that is lost cannot be
//!    reconciled (the old value is gone and the new one never arrived) and stops as `needs_attention`.
//! 4. **Store first.** The new value is written to the entry (a new generation, the old value in its
//!    history) before it is deployed, so a crash after the issuer changed cannot lose it.
//! 5. **Every transition is persisted** ([`RunStore`]), so a crash, a lock or a restart resumes at
//!    the step that was in flight and never starts over.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a connector can do. The class decides the steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// The issuer can hold the old and a new credential at once (a second API token, a second role).
    CreateOverlapping,
    /// UnENVerse generates the value and tells the issuer to use it (`ALTER ROLE ... PASSWORD`).
    SetValueClientGenerated,
    /// The issuer generates a new value in place and the old one stops working at once.
    RollProviderGenerated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub id: String,
    pub value: String,
}

/// Why an issuer call did not succeed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssuerError {
    /// The issuer said no, definitely: nothing changed.
    Rejected(String),
    /// The call may or may not have taken effect (a timeout, a dropped connection).
    Lost(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reconcile {
    /// The step took effect.
    Done,
    /// It did not; repeating it is safe.
    NotDone,
    /// The connector cannot tell. The engine stops and asks for a person.
    Unknown,
}

/// The step a reconcile is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Create,
    SetValue,
    Roll,
    Revoke,
}

pub trait IssuerConnector {
    fn capabilities(&self) -> Vec<Class>;
    /// Does this credential work at the issuer right now?
    fn verify(&mut self, credential: &Credential) -> Result<(), IssuerError>;
    /// Overlapping: make a new credential called `name` (the name carries the entry, generation and
    /// run id, so it can be found again).
    fn create(&mut self, name: &str) -> Result<Credential, IssuerError>;
    /// Client-generated: what the new credential is called at the issuer. The default is the run's
    /// name; a connector with fixed identities (two database roles taking turns) returns the one
    /// that is not `current_id`.
    fn set_target(&self, _current_id: &str, run_name: &str) -> String {
        run_name.to_string()
    }
    /// Client-generated: tell the issuer to use `value` for `name`. Idempotent.
    fn set_value(&mut self, name: &str, value: &str) -> Result<(), IssuerError>;
    /// Provider-generated: roll `current` in place. The old value stops working.
    fn roll(&mut self, current: &Credential) -> Result<Credential, IssuerError>;
    /// Remove a credential. Called last, and only when the revoke rule is met.
    fn revoke(&mut self, credential: &Credential) -> Result<(), IssuerError>;
    /// Did `step` take effect? Asked only after a lost response.
    fn reconcile(&mut self, step: &StepKind, name: &str) -> Result<Reconcile, IssuerError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Failed(String),
    /// Not reachable now; try again later. Not a failure of the rotation.
    Offline,
}

/// What uses the credential, usually node targets. Each consumer is identified by a stable id.
pub trait Consumers {
    fn required(&self) -> Vec<String>;
    fn deploy(&mut self, id: &str, credential: &Credential, generation: u64) -> Outcome;
    fn check(&mut self, id: &str) -> Outcome;
    /// Put the previous credential back (the rollback rule).
    fn redeploy_previous(&mut self, id: &str) -> Outcome;
}

/// The vault entry being rotated.
pub trait EntryStore {
    fn current(&self) -> Credential;
    /// Writes the new value as the next generation, with the old one kept in history.
    fn commit(&mut self, new: &Credential, generation: u64) -> Result<(), String>;
    /// Puts the old value back as the current one (a rollback).
    fn restore(&mut self, old: &Credential) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevokeRule {
    /// Revoke when every required consumer was deployed to and checked in this run.
    AfterAllChecked,
    /// Never revoke automatically; leave it for a person.
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackRule {
    /// A consumer that fails to deploy or check puts the old credential back everywhere.
    RestoreOnFailure,
    /// Leave things as they are and stop for a person.
    Stop,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub revoke: RevokeRule,
    pub rollback: RollbackRule,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            revoke: RevokeRule::AfterAllChecked,
            rollback: RollbackRule::RestoreOnFailure,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Planned,
    /// The issuer is being asked for a new credential.
    Issuing,
    Verifying,
    /// The new value is written to the entry (store first).
    Committing,
    Deploying,
    Checking,
    Revoking,
    Finished,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum Status {
    Running,
    Succeeded,
    /// Everything worked except revocation: a required consumer was offline or the policy says never.
    /// The old credential is still valid.
    RevocationPending(String),
    /// A person has to look: something is uncertain or a destructive call was lost.
    NeedsAttention(String),
    /// Stopped before any issuer change that matters; nothing to undo.
    Failed(String),
    /// A consumer failed and the old credential was put back everywhere.
    RolledBack(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub entry: String,
    pub generation: u64,
    pub class: Class,
    pub policy: Policy,
    pub step: Step,
    pub status: Status,
    /// The name every credential created for this run carries, so it can be found again.
    pub name: String,
    pub old_id: String,
    pub new_id: Option<String>,
    /// Per consumer: `deployed`, `checked`, `offline`, `failed:<reason>`.
    pub consumers: BTreeMap<String, String>,
    /// Whether the revoke call was made. Checked by the safety property, and by a resumed run so it
    /// never revokes twice.
    pub revoked: bool,
}

/// Where runs and pending values are kept between steps.
pub trait RunStore {
    fn save(&mut self, run: &Run) -> Result<(), String>;
    /// A value chosen by the client, stored before the issuer is told (rule 2).
    fn put_pending(&mut self, run_id: &str, value: &str) -> Result<(), String>;
    fn pending(&self, run_id: &str) -> Option<String>;
    fn clear_pending(&mut self, run_id: &str) -> Result<(), String>;
}

pub fn run_name(entry: &str, generation: u64, run_id: &str) -> String {
    format!("unv:{entry}:g{generation}:{run_id}")
}

impl Run {
    pub fn plan(
        id: &str,
        entry: &str,
        generation: u64,
        class: Class,
        policy: Policy,
        old_id: &str,
    ) -> Self {
        Self {
            id: id.into(),
            entry: entry.into(),
            generation,
            class,
            policy,
            step: Step::Planned,
            status: Status::Running,
            name: run_name(entry, generation, id),
            old_id: old_id.into(),
            new_id: None,
            consumers: BTreeMap::new(),
            revoked: false,
        }
    }

    pub fn is_finished(&self) -> bool {
        !matches!(self.status, Status::Running)
    }
}

pub struct Engine<'a> {
    pub issuer: &'a mut dyn IssuerConnector,
    pub consumers: &'a mut dyn Consumers,
    pub entry: &'a mut dyn EntryStore,
    pub store: &'a mut dyn RunStore,
    /// Generates the value for [`Class::SetValueClientGenerated`].
    pub generate: &'a dyn Fn() -> String,
}

fn stop(run: &mut Run, status: Status) {
    run.status = status;
}

impl Engine<'_> {
    fn persist(&mut self, run: &Run) -> Result<(), String> {
        self.store.save(run)
    }

    /// Runs the machine until it finishes or needs a person, saving after every transition.
    pub fn run(&mut self, run: &mut Run) -> Result<(), String> {
        let mut guard = 0;
        while !run.is_finished() {
            self.step(run)?;
            self.persist(run)?;
            guard += 1;
            if guard > 64 {
                stop(run, Status::NeedsAttention("the run did not finish".into()));
                self.persist(run)?;
            }
        }
        Ok(())
    }

    /// One transition. The run is the whole state: calling this on a run loaded from the store
    /// resumes exactly where the previous process stopped.
    pub fn step(&mut self, run: &mut Run) -> Result<(), String> {
        match run.step {
            Step::Planned => {
                if !self.issuer.capabilities().contains(&run.class) {
                    stop(
                        run,
                        Status::Failed("the issuer cannot rotate this way".into()),
                    );
                } else {
                    for id in self.consumers.required() {
                        run.consumers.entry(id).or_insert_with(|| "pending".into());
                    }
                    run.step = Step::Issuing;
                }
            }
            Step::Issuing => self.issue(run)?,
            Step::Verifying => self.verify(run)?,
            Step::Committing => {
                let Some(new) = self.new_credential(run) else {
                    stop(
                        run,
                        Status::NeedsAttention("the new credential is missing".into()),
                    );
                    return Ok(());
                };
                match self.entry.commit(&new, run.generation) {
                    Ok(()) => {
                        let _ = self.store.clear_pending(&run.id);
                        run.step = Step::Deploying;
                    }
                    Err(e) => stop(
                        run,
                        Status::NeedsAttention(format!(
                            "could not write the new value to the entry: {e}"
                        )),
                    ),
                }
            }
            Step::Deploying => self.deploy(run),
            Step::Checking => self.check(run),
            Step::Revoking => self.revoke(run),
            Step::Finished => {
                if run.status == Status::Running {
                    run.status = Status::Succeeded;
                }
            }
        }
        Ok(())
    }

    /// The new credential, from wherever this class keeps it.
    fn new_credential(&self, run: &Run) -> Option<Credential> {
        match run.class {
            Class::SetValueClientGenerated => self
                .store
                .pending(&run.id)
                .map(|value| Credential {
                    id: run.new_id.clone().unwrap_or_else(|| run.name.clone()),
                    value,
                })
                .or_else(|| {
                    // Already committed: the entry holds it.
                    let current = self.entry.current();
                    (run.step > Step::Committing).then_some(current)
                }),
            _ => {
                // The pending value wins while it exists: a rolled credential keeps its id, so the
                // entry's current credential has the same id as the new one until it is committed.
                if let Some((value, id)) = self.store.pending(&run.id).zip(run.new_id.clone()) {
                    return Some(Credential { id, value });
                }
                let current = self.entry.current();
                (run.new_id.as_deref() == Some(current.id.as_str())).then_some(current)
            }
        }
    }

    fn issue(&mut self, run: &mut Run) -> Result<(), String> {
        match run.class {
            Class::CreateOverlapping => match self.issuer.create(&run.name) {
                Ok(c) => {
                    // Provider-generated: the value only exists now. Keep it safe before anything else.
                    self.store.put_pending(&run.id, &c.value)?;
                    run.new_id = Some(c.id);
                    run.step = Step::Verifying;
                }
                Err(IssuerError::Rejected(e)) => stop(run, Status::Failed(e)),
                Err(IssuerError::Lost(e)) => {
                    match self.issuer.reconcile(&StepKind::Create, &run.name) {
                        // The credential exists at the issuer but its value never reached us. It is a
                        // new, unused credential, so removing it and asking again is safe.
                        Ok(Reconcile::Done) => {
                            stop(
                            run,
                            Status::NeedsAttention(format!(
                                "the issuer created a credential named {} but the answer was lost ({e}); delete it there and run again",
                                run.name
                            )),
                        );
                        }
                        Ok(Reconcile::NotDone) => {} // stay on this step: safe to repeat
                        _ => stop(
                            run,
                            Status::NeedsAttention(format!("uncertain create: {e}")),
                        ),
                    }
                }
            },
            Class::SetValueClientGenerated => {
                // Choose the value and keep it before the issuer is told.
                let value = match self.store.pending(&run.id) {
                    Some(v) => v,
                    None => {
                        let v = (self.generate)();
                        self.store.put_pending(&run.id, &v)?;
                        v
                    }
                };
                let target = self.issuer.set_target(&run.old_id, &run.name);
                match self.issuer.set_value(&target, &value) {
                    Ok(()) => {
                        run.new_id = Some(target.clone());
                        run.step = Step::Verifying;
                    }
                    Err(IssuerError::Rejected(e)) => stop(run, Status::Failed(e)),
                    Err(IssuerError::Lost(e)) => {
                        match self.issuer.reconcile(&StepKind::SetValue, &target) {
                            Ok(Reconcile::Done) => {
                                run.new_id = Some(target.clone());
                                run.step = Step::Verifying;
                            }
                            Ok(Reconcile::NotDone) => {} // repeat: set_value is idempotent
                            _ => stop(
                                run,
                                Status::NeedsAttention(format!("uncertain set_value: {e}")),
                            ),
                        }
                    }
                }
            }
            Class::RollProviderGenerated => {
                let current = self.entry.current();
                match self.issuer.roll(&current) {
                    Ok(c) => {
                        self.store.put_pending(&run.id, &c.value)?;
                        run.new_id = Some(c.id);
                        run.step = Step::Verifying;
                    }
                    Err(IssuerError::Rejected(e)) => stop(run, Status::Failed(e)),
                    // The old value stopped working and the new one never arrived. Nothing to
                    // reconcile and nothing safe to retry.
                    Err(IssuerError::Lost(e)) => stop(
                        run,
                        Status::NeedsAttention(format!(
                            "the roll's answer was lost ({e}); the old value may no longer work and the new one was not received. Issue a new credential at the provider"
                        )),
                    ),
                }
            }
        }
        Ok(())
    }

    fn verify(&mut self, run: &mut Run) -> Result<(), String> {
        let Some(new) = self.new_credential(run) else {
            stop(
                run,
                Status::NeedsAttention("the new credential is missing".into()),
            );
            return Ok(());
        };
        match self.issuer.verify(&new) {
            Ok(()) => run.step = Step::Committing,
            Err(IssuerError::Rejected(e)) | Err(IssuerError::Lost(e)) => {
                // The new credential does not work. The old one is untouched (overlapping and
                // client-generated keep both); a rolled credential is already the only one.
                let status = if run.class == Class::RollProviderGenerated {
                    Status::NeedsAttention(format!("the rolled credential did not verify: {e}"))
                } else {
                    Status::Failed(format!("the new credential did not verify: {e}"))
                };
                stop(run, status);
            }
        }
        Ok(())
    }

    fn deploy(&mut self, run: &mut Run) {
        let Some(new) = self.new_credential(run) else {
            stop(
                run,
                Status::NeedsAttention("the new credential is missing".into()),
            );
            return;
        };
        for id in self.consumers.required() {
            if matches!(
                run.consumers.get(&id).map(String::as_str),
                Some("deployed" | "checked")
            ) {
                continue;
            }
            match self.consumers.deploy(&id, &new, run.generation) {
                Outcome::Ok => {
                    run.consumers.insert(id, "deployed".into());
                }
                Outcome::Offline => {
                    run.consumers.insert(id, "offline".into());
                }
                Outcome::Failed(e) => {
                    run.consumers.insert(id.clone(), format!("failed:{e}"));
                    self.fail_consumer(run, &id, &e);
                    return;
                }
            }
        }
        run.step = Step::Checking;
    }

    fn check(&mut self, run: &mut Run) {
        for id in self.consumers.required() {
            match run.consumers.get(&id).map(String::as_str) {
                Some("checked") => continue,
                Some("deployed") => {}
                // Never deployed (offline): it cannot have passed a check.
                _ => continue,
            }
            match self.consumers.check(&id) {
                Outcome::Ok => {
                    run.consumers.insert(id, "checked".into());
                }
                Outcome::Offline => {
                    run.consumers.insert(id, "offline".into());
                }
                Outcome::Failed(e) => {
                    run.consumers.insert(id.clone(), format!("failed:{e}"));
                    self.fail_consumer(run, &id, &e);
                    return;
                }
            }
        }
        run.step = Step::Revoking;
    }

    /// A consumer failed after the new value was committed. The old credential is still valid, so
    /// the rollback rule can put it back everywhere.
    fn fail_consumer(&mut self, run: &mut Run, id: &str, reason: &str) {
        if run.policy.rollback == RollbackRule::Stop {
            stop(
                run,
                Status::NeedsAttention(format!("{id} failed ({reason}); left as it is")),
            );
            return;
        }
        let mut ok = true;
        for c in self.consumers.required() {
            if matches!(
                run.consumers.get(&c).map(String::as_str),
                Some("deployed" | "checked")
            ) && !matches!(self.consumers.redeploy_previous(&c), Outcome::Ok)
            {
                ok = false;
            }
        }
        let restored = self
            .entry
            .restore(&Credential {
                id: run.old_id.clone(),
                value: String::new(),
            })
            .is_ok();
        if ok && restored {
            stop(run, Status::RolledBack(format!("{id} failed: {reason}")));
        } else {
            stop(
                run,
                Status::NeedsAttention(format!(
                    "{id} failed ({reason}) and putting the old credential back did not fully succeed"
                )),
            );
        }
    }

    fn revoke(&mut self, run: &mut Run) {
        // Rule 1, checked here and nowhere else: revocation needs every required consumer checked.
        let all_checked = self
            .consumers
            .required()
            .iter()
            .all(|id| run.consumers.get(id).map(String::as_str) == Some("checked"));
        let offline: Vec<String> = run
            .consumers
            .iter()
            .filter(|(_, s)| s.as_str() == "offline")
            .map(|(k, _)| k.clone())
            .collect();
        if run.policy.revoke == RevokeRule::Never {
            stop(
                run,
                Status::RevocationPending("the policy never revokes automatically".into()),
            );
            run.step = Step::Finished;
            return;
        }
        if !all_checked {
            let why = if offline.is_empty() {
                "not every consumer passed its check".to_string()
            } else {
                format!("waiting for {} to come back", offline.join(", "))
            };
            stop(run, Status::RevocationPending(why));
            run.step = Step::Finished;
            return;
        }
        if run.revoked {
            run.step = Step::Finished;
            run.status = Status::Succeeded;
            return;
        }
        if run.class == Class::RollProviderGenerated {
            // A roll replaced the value under the same id: the old value died when the new one was
            // issued, and a revoke by id would delete the credential just rotated.
            run.revoked = true;
            run.step = Step::Finished;
            run.status = Status::Succeeded;
            return;
        }
        let old = Credential {
            id: run.old_id.clone(),
            value: String::new(),
        };
        match self.issuer.revoke(&old) {
            Ok(()) => {
                run.revoked = true;
                run.step = Step::Finished;
                run.status = Status::Succeeded;
            }
            Err(IssuerError::Rejected(e)) => {
                stop(
                    run,
                    Status::RevocationPending(format!(
                        "the issuer refused to revoke the old credential: {e}"
                    )),
                );
                run.step = Step::Finished;
            }
            Err(IssuerError::Lost(e)) => {
                match self.issuer.reconcile(&StepKind::Revoke, &run.old_id) {
                    Ok(Reconcile::Done) => {
                        run.revoked = true;
                        run.step = Step::Finished;
                        run.status = Status::Succeeded;
                    }
                    // Not done: repeating revoke is safe (the old credential is simply still there),
                    // but there is no reason to rush; leave it pending for the next run.
                    Ok(Reconcile::NotDone) => {
                        stop(
                            run,
                            Status::RevocationPending(
                                "the revoke did not take effect; try again".into(),
                            ),
                        );
                        run.step = Step::Finished;
                    }
                    _ => {
                        stop(
                            run,
                            Status::NeedsAttention(format!("uncertain revoke: {e}")),
                        );
                        run.step = Step::Finished;
                    }
                }
            }
        }
    }
}

// ── SQLite persistence ──────────────────────────────────────────────────────

/// Creates the rotation tables inside the encrypted vault database. Runs hold ids and states, never
/// values; `rotation_pending` holds the one value that exists nowhere else yet.
pub fn init_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS rotation_runs (
             id TEXT PRIMARY KEY, entry TEXT NOT NULL, state TEXT NOT NULL, updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS rotation_pending (
             run_id TEXT PRIMARY KEY, value TEXT NOT NULL, created_at TEXT NOT NULL
         );",
    )
    .map_err(|e| e.to_string())
}

pub struct SqlStore<'a> {
    pub conn: &'a rusqlite::Connection,
}

impl RunStore for SqlStore<'_> {
    fn save(&mut self, run: &Run) -> Result<(), String> {
        let state = serde_json::to_string(run).map_err(|e| e.to_string())?;
        self.conn
            .execute(
                "INSERT INTO rotation_runs (id, entry, state, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
                rusqlite::params![run.id, run.entry, state, crate::iso_now()],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn put_pending(&mut self, run_id: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO rotation_pending (run_id, value, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![run_id, value, crate::iso_now()],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn pending(&self, run_id: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM rotation_pending WHERE run_id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .ok()
    }

    fn clear_pending(&mut self, run_id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM rotation_pending WHERE run_id = ?1", [run_id])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// A run saved by [`SqlStore`], to resume after a crash or an unlock.
pub fn load_run(conn: &rusqlite::Connection, id: &str) -> Result<Option<Run>, String> {
    use rusqlite::OptionalExtension;
    let text: Option<String> = conn
        .query_row("SELECT state FROM rotation_runs WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    text.map(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .transpose()
}

pub fn list_runs(conn: &rusqlite::Connection) -> Result<Vec<Run>, String> {
    let mut stmt = conn
        .prepare("SELECT state FROM rotation_runs ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.map(|r| {
        r.map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
    })
    .collect()
}

#[cfg(test)]
pub(crate) mod fakes {
    //! A fake issuer, consumers and entry that inject faults at chosen calls, shared by the engine's
    //! tests and by connector-independent property tests.
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Default)]
    pub struct World {
        /// Credentials that exist at the issuer, by id, with their values.
        pub issuer: BTreeMap<String, String>,
        pub revoked: BTreeSet<String>,
        pub revoke_calls: Vec<String>,
        /// Every call, in order, for ordering assertions.
        pub calls: Vec<String>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Fault {
        None,
        /// The call fails definitively.
        Reject,
        /// The call takes effect and the answer is lost.
        LostAfter,
        /// The call does not take effect and the answer is lost.
        LostBefore,
    }

    pub struct FakeIssuer<'w> {
        pub world: &'w std::cell::RefCell<World>,
        pub classes: Vec<Class>,
        pub create: Fault,
        pub set_value: Fault,
        pub roll: Fault,
        pub verify: Fault,
        pub revoke: Fault,
        /// Reconcile cannot tell what happened.
        pub reconcile_unknown: bool,
        pub seq: u32,
    }

    impl<'w> FakeIssuer<'w> {
        pub fn new(world: &'w std::cell::RefCell<World>, class: Class) -> Self {
            world
                .borrow_mut()
                .issuer
                .insert("old".into(), "old-value".into());
            Self {
                world,
                classes: vec![class],
                create: Fault::None,
                set_value: Fault::None,
                roll: Fault::None,
                verify: Fault::None,
                revoke: Fault::None,
                reconcile_unknown: false,
                seq: 0,
            }
        }
    }

    fn lost(f: Fault) -> Option<IssuerError> {
        match f {
            Fault::LostAfter | Fault::LostBefore => Some(IssuerError::Lost("timeout".into())),
            Fault::Reject => Some(IssuerError::Rejected("no".into())),
            Fault::None => None,
        }
    }

    impl IssuerConnector for FakeIssuer<'_> {
        fn capabilities(&self) -> Vec<Class> {
            self.classes.clone()
        }
        fn verify(&mut self, c: &Credential) -> Result<(), IssuerError> {
            self.world.borrow_mut().calls.push("verify".into());
            if let Some(e) = lost(self.verify) {
                return Err(e);
            }
            let w = self.world.borrow();
            if w.issuer.get(&c.id) == Some(&c.value) && !w.revoked.contains(&c.id) {
                Ok(())
            } else {
                Err(IssuerError::Rejected("does not work".into()))
            }
        }
        fn create(&mut self, name: &str) -> Result<Credential, IssuerError> {
            self.world.borrow_mut().calls.push("create".into());
            self.seq += 1;
            let c = Credential {
                id: name.to_string(),
                value: format!("provider-value-{}", self.seq),
            };
            match self.create {
                Fault::Reject => return Err(IssuerError::Rejected("no".into())),
                Fault::LostBefore => return Err(IssuerError::Lost("timeout".into())),
                Fault::LostAfter => {
                    self.world
                        .borrow_mut()
                        .issuer
                        .insert(c.id.clone(), c.value.clone());
                    // Only the first attempt is lost: a retry after a "not done" must succeed.
                    self.create = Fault::None;
                    return Err(IssuerError::Lost("timeout".into()));
                }
                Fault::None => {}
            }
            self.world
                .borrow_mut()
                .issuer
                .insert(c.id.clone(), c.value.clone());
            Ok(c)
        }
        fn set_value(&mut self, name: &str, value: &str) -> Result<(), IssuerError> {
            self.world.borrow_mut().calls.push("set_value".into());
            match self.set_value {
                Fault::Reject => return Err(IssuerError::Rejected("no".into())),
                Fault::LostBefore => {
                    self.set_value = Fault::None;
                    return Err(IssuerError::Lost("timeout".into()));
                }
                Fault::LostAfter => {
                    self.world
                        .borrow_mut()
                        .issuer
                        .insert(name.into(), value.into());
                    self.set_value = Fault::None;
                    return Err(IssuerError::Lost("timeout".into()));
                }
                Fault::None => {}
            }
            self.world
                .borrow_mut()
                .issuer
                .insert(name.into(), value.into());
            Ok(())
        }
        fn roll(&mut self, current: &Credential) -> Result<Credential, IssuerError> {
            self.world.borrow_mut().calls.push("roll".into());
            self.seq += 1;
            let new = Credential {
                id: current.id.clone(),
                value: format!("rolled-{}", self.seq),
            };
            match self.roll {
                Fault::Reject => return Err(IssuerError::Rejected("no".into())),
                Fault::LostBefore => return Err(IssuerError::Lost("timeout".into())),
                Fault::LostAfter => {
                    self.world
                        .borrow_mut()
                        .issuer
                        .insert(new.id.clone(), new.value.clone());
                    return Err(IssuerError::Lost("timeout".into()));
                }
                Fault::None => {}
            }
            self.world
                .borrow_mut()
                .issuer
                .insert(new.id.clone(), new.value.clone());
            Ok(new)
        }
        fn revoke(&mut self, c: &Credential) -> Result<(), IssuerError> {
            {
                let mut w = self.world.borrow_mut();
                w.calls.push("revoke".into());
                w.revoke_calls.push(c.id.clone());
            }
            match self.revoke {
                Fault::Reject => return Err(IssuerError::Rejected("no".into())),
                Fault::LostBefore => return Err(IssuerError::Lost("timeout".into())),
                Fault::LostAfter => {
                    self.world.borrow_mut().revoked.insert(c.id.clone());
                    return Err(IssuerError::Lost("timeout".into()));
                }
                Fault::None => {}
            }
            self.world.borrow_mut().revoked.insert(c.id.clone());
            Ok(())
        }
        fn reconcile(&mut self, step: &StepKind, name: &str) -> Result<Reconcile, IssuerError> {
            self.world.borrow_mut().calls.push("reconcile".into());
            if self.reconcile_unknown {
                return Ok(Reconcile::Unknown);
            }
            let w = self.world.borrow();
            Ok(match step {
                StepKind::Create | StepKind::SetValue => {
                    if w.issuer.contains_key(name) {
                        Reconcile::Done
                    } else {
                        Reconcile::NotDone
                    }
                }
                StepKind::Revoke => {
                    if w.revoked.contains(name) {
                        Reconcile::Done
                    } else {
                        Reconcile::NotDone
                    }
                }
                StepKind::Roll => Reconcile::Unknown,
            })
        }
    }

    pub struct FakeConsumers {
        pub ids: Vec<String>,
        /// What each consumer does on `deploy` and `check`.
        pub deploy: BTreeMap<String, Outcome>,
        pub check: BTreeMap<String, Outcome>,
        pub deployed: BTreeMap<String, String>,
        pub redeployed: Vec<String>,
        pub redeploy_fails: bool,
    }

    impl FakeConsumers {
        pub fn new(ids: &[&str]) -> Self {
            Self {
                ids: ids.iter().map(|s| s.to_string()).collect(),
                deploy: BTreeMap::new(),
                check: BTreeMap::new(),
                deployed: BTreeMap::new(),
                redeployed: Vec::new(),
                redeploy_fails: false,
            }
        }
    }

    impl Consumers for FakeConsumers {
        fn required(&self) -> Vec<String> {
            self.ids.clone()
        }
        fn deploy(&mut self, id: &str, c: &Credential, _g: u64) -> Outcome {
            let out = self.deploy.get(id).cloned().unwrap_or(Outcome::Ok);
            if out == Outcome::Ok {
                self.deployed.insert(id.into(), c.value.clone());
            }
            out
        }
        fn check(&mut self, id: &str) -> Outcome {
            self.check.get(id).cloned().unwrap_or(Outcome::Ok)
        }
        fn redeploy_previous(&mut self, id: &str) -> Outcome {
            self.redeployed.push(id.into());
            if self.redeploy_fails {
                return Outcome::Failed("cannot".into());
            }
            self.deployed.insert(id.into(), "old-value".into());
            Outcome::Ok
        }
    }

    pub struct FakeEntry {
        pub current: Credential,
        pub history: Vec<Credential>,
        pub fail_commit: bool,
        pub fail_restore: bool,
    }

    impl FakeEntry {
        pub fn new() -> Self {
            Self {
                current: Credential {
                    id: "old".into(),
                    value: "old-value".into(),
                },
                history: Vec::new(),
                fail_commit: false,
                fail_restore: false,
            }
        }
    }

    impl EntryStore for FakeEntry {
        fn current(&self) -> Credential {
            self.current.clone()
        }
        fn commit(&mut self, new: &Credential, _g: u64) -> Result<(), String> {
            if self.fail_commit {
                return Err("disk full".into());
            }
            self.history.push(self.current.clone());
            self.current = new.clone();
            Ok(())
        }
        fn restore(&mut self, _old: &Credential) -> Result<(), String> {
            if self.fail_restore {
                return Err("read-only".into());
            }
            if let Some(prev) = self.history.pop() {
                self.current = prev;
            }
            Ok(())
        }
    }

    #[derive(Default)]
    pub struct MemStore {
        pub saved: Vec<Run>,
        pub pending: BTreeMap<String, String>,
    }

    impl RunStore for MemStore {
        fn save(&mut self, run: &Run) -> Result<(), String> {
            self.saved.push(run.clone());
            Ok(())
        }
        fn put_pending(&mut self, run_id: &str, value: &str) -> Result<(), String> {
            self.pending.insert(run_id.into(), value.into());
            Ok(())
        }
        fn pending(&self, run_id: &str) -> Option<String> {
            self.pending.get(run_id).cloned()
        }
        fn clear_pending(&mut self, run_id: &str) -> Result<(), String> {
            self.pending.remove(run_id);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::*;
    use super::*;
    use std::cell::RefCell;

    fn engine_run(
        class: Class,
        configure: impl FnOnce(&mut FakeIssuer<'_>, &mut FakeConsumers, &mut FakeEntry),
        consumers: &[&str],
    ) -> (Run, RefCell<World>, FakeConsumers, FakeEntry, MemStore) {
        let world = RefCell::new(World::default());
        let mut run = Run::plan("r1", "Entry", 2, class, Policy::default(), "old");
        let mut cons = FakeConsumers::new(consumers);
        let mut entry = FakeEntry::new();
        let mut store = MemStore::default();
        {
            let mut issuer = FakeIssuer::new(&world, class);
            configure(&mut issuer, &mut cons, &mut entry);
            let gen = || "client-generated-value".to_string();
            let mut engine = Engine {
                issuer: &mut issuer,
                consumers: &mut cons,
                entry: &mut entry,
                store: &mut store,
                generate: &gen,
            };
            engine.run(&mut run).unwrap();
        }
        (run, world, cons, entry, store)
    }

    #[test]
    fn an_overlapping_rotation_creates_deploys_checks_then_revokes_last() {
        let (run, world, cons, entry, store) =
            engine_run(Class::CreateOverlapping, |_, _, _| {}, &["web", "worker"]);
        assert_eq!(run.status, Status::Succeeded);
        assert!(run.revoked);
        let calls = world.borrow().calls.clone();
        assert_eq!(calls.first().map(String::as_str), Some("create"));
        assert_eq!(
            calls.last().map(String::as_str),
            Some("revoke"),
            "{calls:?}"
        );
        assert_eq!(entry.current.value, "provider-value-1");
        assert_eq!(
            entry.history.len(),
            1,
            "the old value is kept in the entry's history"
        );
        assert!(cons.deployed.values().all(|v| v == "provider-value-1"));
        assert!(
            store.pending.is_empty(),
            "the pending value is cleared once it is in the entry"
        );
        assert_eq!(world.borrow().revoke_calls, vec!["old".to_string()]);
    }

    #[test]
    fn a_client_generated_value_is_stored_before_the_issuer_is_told() {
        let world = RefCell::new(World::default());
        let mut issuer = FakeIssuer::new(&world, Class::SetValueClientGenerated);
        // The issuer rejects: the value must already have been stored (rule 2) and nothing revoked.
        issuer.set_value = Fault::Reject;
        let mut run = Run::plan(
            "r1",
            "E",
            2,
            Class::SetValueClientGenerated,
            Policy::default(),
            "old",
        );
        let mut cons = FakeConsumers::new(&["web"]);
        let mut entry = FakeEntry::new();
        let mut store = MemStore::default();
        let gen = || "chosen-by-the-client".to_string();
        Engine {
            issuer: &mut issuer,
            consumers: &mut cons,
            entry: &mut entry,
            store: &mut store,
            generate: &gen,
        }
        .run(&mut run)
        .unwrap();
        assert!(matches!(run.status, Status::Failed(_)));
        assert_eq!(
            store.pending.get("r1").map(String::as_str),
            Some("chosen-by-the-client")
        );
        assert!(world.borrow().revoke_calls.is_empty());
        assert_eq!(entry.current.value, "old-value", "the entry is untouched");
    }

    #[test]
    fn a_response_lost_after_create_is_found_by_name_and_stops_for_a_person() {
        let (run, world, _, entry, _) = engine_run(
            Class::CreateOverlapping,
            |i, _, _| i.create = Fault::LostAfter,
            &["web"],
        );
        assert!(
            matches!(run.status, Status::NeedsAttention(_)),
            "{:?}",
            run.status
        );
        assert!(world.borrow().calls.contains(&"reconcile".to_string()));
        assert!(
            world.borrow().revoke_calls.is_empty(),
            "nothing is revoked on an uncertain create"
        );
        assert_eq!(entry.current.value, "old-value");
    }

    #[test]
    fn a_response_lost_before_create_is_retried_safely() {
        let (run, world, _, entry, _) = engine_run(
            Class::CreateOverlapping,
            |i, _, _| i.create = Fault::LostBefore,
            &["web"],
        );
        // LostBefore keeps failing in the fake, so reconcile says NotDone each time and the guard
        // stops the run; the point is that nothing was revoked and the entry is untouched.
        assert!(
            matches!(run.status, Status::NeedsAttention(_) | Status::Failed(_)),
            "{:?}",
            run.status
        );
        assert!(world.borrow().revoke_calls.is_empty());
        assert_eq!(entry.current.value, "old-value");
    }

    #[test]
    fn a_lost_client_generated_set_value_is_reconciled_then_the_run_continues() {
        for fault in [Fault::LostAfter, Fault::LostBefore] {
            let (run, world, _, entry, _) = engine_run(
                Class::SetValueClientGenerated,
                |i, _, _| i.set_value = fault,
                &["web"],
            );
            assert_eq!(run.status, Status::Succeeded, "{fault:?}: {:?}", run.status);
            assert_eq!(entry.current.value, "client-generated-value");
            assert!(world.borrow().calls.contains(&"reconcile".to_string()));
        }
    }

    #[test]
    fn a_lost_in_place_roll_is_never_retried_and_needs_a_person() {
        let (run, world, _, entry, _) = engine_run(
            Class::RollProviderGenerated,
            |i, _, _| i.roll = Fault::LostAfter,
            &["web"],
        );
        assert!(matches!(run.status, Status::NeedsAttention(_)));
        assert_eq!(
            world.borrow().calls.iter().filter(|c| *c == "roll").count(),
            1,
            "a destructive call is never repeated"
        );
        assert!(world.borrow().revoke_calls.is_empty());
        assert_eq!(entry.current.value, "old-value");
    }

    #[test]
    fn a_successful_roll_deploys_the_rolled_value_and_never_revokes_by_id() {
        // Bug: a rolled credential keeps its id, so the engine verified and deployed the entry's
        // old value (same id) and then revoked the credential it had just rotated.
        let (run, world, cons, entry, _) =
            engine_run(Class::RollProviderGenerated, |_, _, _| {}, &["web"]);
        assert_eq!(run.status, Status::Succeeded, "{:?}", run.status);
        assert_eq!(entry.current.value, "rolled-1");
        assert_eq!(cons.deployed["web"], "rolled-1");
        assert!(world.borrow().revoke_calls.is_empty());
    }

    #[test]
    fn a_connector_with_fixed_identities_names_the_other_one_as_the_new_credential() {
        struct Roles<'a>(&'a RefCell<World>);
        impl IssuerConnector for Roles<'_> {
            fn capabilities(&self) -> Vec<Class> {
                vec![Class::SetValueClientGenerated]
            }
            fn verify(&mut self, _: &Credential) -> Result<(), IssuerError> {
                Ok(())
            }
            fn create(&mut self, _: &str) -> Result<Credential, IssuerError> {
                Err(IssuerError::Rejected("no".into()))
            }
            fn set_target(&self, current: &str, _: &str) -> String {
                if current == "app_a" { "app_b" } else { "app_a" }.into()
            }
            fn set_value(&mut self, name: &str, value: &str) -> Result<(), IssuerError> {
                self.0.borrow_mut().issuer.insert(name.into(), value.into());
                Ok(())
            }
            fn roll(&mut self, _: &Credential) -> Result<Credential, IssuerError> {
                Err(IssuerError::Rejected("no".into()))
            }
            fn revoke(&mut self, c: &Credential) -> Result<(), IssuerError> {
                self.0.borrow_mut().revoke_calls.push(c.id.clone());
                Ok(())
            }
            fn reconcile(&mut self, _: &StepKind, _: &str) -> Result<Reconcile, IssuerError> {
                Ok(Reconcile::NotDone)
            }
        }
        let world = RefCell::new(World::default());
        let mut issuer = Roles(&world);
        let mut run = Run::plan(
            "r1",
            "Db",
            2,
            Class::SetValueClientGenerated,
            Policy::default(),
            "app_a",
        );
        let mut cons = FakeConsumers::new(&["web"]);
        let mut entry = FakeEntry::new();
        let mut store = MemStore::default();
        let gen = || "fresh-password".to_string();
        Engine {
            issuer: &mut issuer,
            consumers: &mut cons,
            entry: &mut entry,
            store: &mut store,
            generate: &gen,
        }
        .run(&mut run)
        .unwrap();
        assert_eq!(run.status, Status::Succeeded, "{:?}", run.status);
        assert_eq!(
            entry.current.id, "app_b",
            "the entry now names the other role"
        );
        assert_eq!(world.borrow().revoke_calls, vec!["app_a".to_string()]);
    }

    /// Runs `run` (already placed at some step) with the given doubles and returns it.
    fn resume(
        mut run: Run,
        class: Class,
        configure: impl FnOnce(&mut FakeIssuer<'_>, &mut FakeConsumers, &mut FakeEntry, &mut MemStore),
        consumers: &[&str],
    ) -> (Run, RefCell<World>) {
        let world = RefCell::new(World::default());
        let mut cons = FakeConsumers::new(consumers);
        let mut entry = FakeEntry::new();
        let mut store = MemStore::default();
        {
            let mut issuer = FakeIssuer::new(&world, class);
            configure(&mut issuer, &mut cons, &mut entry, &mut store);
            let gen = || "generated".to_string();
            Engine {
                issuer: &mut issuer,
                consumers: &mut cons,
                entry: &mut entry,
                store: &mut store,
                generate: &gen,
            }
            .run(&mut run)
            .unwrap();
        }
        (run, world)
    }

    fn at(step: Step, class: Class) -> Run {
        let mut r = Run::plan("r1", "E", 2, class, Policy::default(), "old");
        r.step = step;
        r
    }

    #[test]
    fn a_run_that_lost_its_new_value_stops_for_a_person_at_every_step_that_needs_it() {
        // A run resumed after the pending value was lost (a restored backup, a cleared table) has
        // nothing to verify, store or deploy, and must not invent a value.
        for step in [Step::Verifying, Step::Committing, Step::Deploying] {
            let (run, world) = resume(
                at(step, Class::CreateOverlapping),
                Class::CreateOverlapping,
                |_, _, _, _| {},
                &["web"],
            );
            assert!(
                matches!(&run.status, Status::NeedsAttention(m) if m.contains("missing")),
                "{step:?}: {:?}",
                run.status
            );
            assert!(world.borrow().revoke_calls.is_empty());
        }
        let (run, _) = resume(
            at(Step::Committing, Class::SetValueClientGenerated),
            Class::SetValueClientGenerated,
            |_, _, _, _| {},
            &["web"],
        );
        assert!(
            matches!(run.status, Status::NeedsAttention(_)),
            "{:?}",
            run.status
        );
    }

    #[test]
    fn a_run_saved_as_finished_but_still_running_is_completed_without_new_calls() {
        let (run, world) = resume(
            at(Step::Finished, Class::CreateOverlapping),
            Class::CreateOverlapping,
            |_, _, _, _| {},
            &[],
        );
        assert_eq!(run.status, Status::Succeeded);
        assert!(world.borrow().calls.is_empty());
    }

    #[test]
    fn a_revoke_that_already_happened_is_not_repeated_when_a_run_resumes() {
        let mut r = at(Step::Revoking, Class::CreateOverlapping);
        r.revoked = true;
        r.consumers.insert("web".into(), "checked".into());
        let (run, world) = resume(r, Class::CreateOverlapping, |_, _, _, _| {}, &["web"]);
        assert_eq!(run.status, Status::Succeeded);
        assert!(world.borrow().revoke_calls.is_empty());
    }

    #[test]
    fn a_consumer_that_never_reported_keeps_revocation_pending_with_a_reason() {
        let mut r = at(Step::Revoking, Class::CreateOverlapping);
        r.consumers.insert("web".into(), "checked".into());
        // "worker" is required but has no state: it was added after planning.
        let (run, world) = resume(
            r,
            Class::CreateOverlapping,
            |_, _, _, _| {},
            &["web", "worker"],
        );
        assert!(
            matches!(&run.status, Status::RevocationPending(m) if m.contains("not every consumer")),
            "{:?}",
            run.status
        );
        assert!(world.borrow().revoke_calls.is_empty());
    }

    #[test]
    fn a_connector_that_cannot_tell_what_happened_stops_the_run_instead_of_guessing() {
        let (run, world, ..) = engine_run(
            Class::CreateOverlapping,
            |i, _, _| {
                i.create = Fault::LostBefore;
                i.reconcile_unknown = true;
            },
            &["web"],
        );
        assert!(
            matches!(&run.status, Status::NeedsAttention(m) if m.contains("uncertain create")),
            "{:?}",
            run.status
        );
        assert!(world.borrow().revoke_calls.is_empty());

        let (run, ..) = engine_run(
            Class::SetValueClientGenerated,
            |i, _, _| {
                i.set_value = Fault::LostBefore;
                i.reconcile_unknown = true;
            },
            &["web"],
        );
        assert!(
            matches!(&run.status, Status::NeedsAttention(m) if m.contains("uncertain set_value")),
            "{:?}",
            run.status
        );

        let (run, world, ..) = engine_run(
            Class::CreateOverlapping,
            |i, _, _| {
                i.revoke = Fault::LostAfter;
                i.reconcile_unknown = true;
            },
            &["web"],
        );
        assert!(
            matches!(&run.status, Status::NeedsAttention(m) if m.contains("uncertain revoke")),
            "{:?}",
            run.status
        );
        assert_eq!(world.borrow().revoke_calls.len(), 1, "never repeated");
    }

    #[test]
    fn a_rollback_that_cannot_put_the_old_credential_back_says_so() {
        // A consumer fails its check and putting the old credential back fails too.
        for (redeploy_fails, fail_restore) in [(true, false), (false, true)] {
            let (run, ..) = engine_run(
                Class::CreateOverlapping,
                |_, c, e| {
                    c.check
                        .insert("worker".into(), Outcome::Failed("503".into()));
                    c.redeploy_fails = redeploy_fails;
                    e.fail_restore = fail_restore;
                },
                &["web", "worker"],
            );
            assert!(
                matches!(&run.status, Status::NeedsAttention(m) if m.contains("did not fully succeed")),
                "{redeploy_fails} {fail_restore}: {:?}",
                run.status
            );
        }
    }

    #[test]
    fn a_rejected_verification_of_a_rolled_credential_needs_a_person_but_others_just_fail() {
        let (run, ..) = engine_run(
            Class::RollProviderGenerated,
            |i, _, _| i.verify = Fault::Reject,
            &["web"],
        );
        assert!(
            matches!(&run.status, Status::NeedsAttention(m) if m.contains("rolled credential")),
            "{:?}",
            run.status
        );
        let (run, ..) = engine_run(
            Class::CreateOverlapping,
            |i, _, _| i.verify = Fault::Reject,
            &["web"],
        );
        assert!(
            matches!(&run.status, Status::Failed(m) if m.contains("did not verify")),
            "{:?}",
            run.status
        );
    }

    #[test]
    fn a_run_that_cannot_make_progress_is_stopped_by_the_guard() {
        // A connector that is always "not done" for a lost create would loop for ever.
        let (run, ..) = engine_run(
            Class::CreateOverlapping,
            |i, _, _| i.create = Fault::LostBefore,
            &["web"],
        );
        assert!(
            matches!(run.status, Status::NeedsAttention(_) | Status::Failed(_)),
            "{:?}",
            run.status
        );
    }

    #[test]
    fn an_offline_required_consumer_leaves_revocation_pending_and_the_old_credential_valid() {
        let (run, world, _, _, _) = engine_run(
            Class::CreateOverlapping,
            |_, c, _| {
                c.deploy.insert("worker".into(), Outcome::Offline);
            },
            &["web", "worker"],
        );
        assert!(
            matches!(run.status, Status::RevocationPending(_)),
            "{:?}",
            run.status
        );
        assert!(world.borrow().revoke_calls.is_empty());
        assert!(!world.borrow().revoked.contains("old"));
        assert_eq!(run.consumers["worker"], "offline");
    }

    #[test]
    fn a_failing_consumer_rolls_everything_back_while_the_old_credential_is_still_valid() {
        let (run, world, cons, entry, _) = engine_run(
            Class::CreateOverlapping,
            |_, c, _| {
                c.check
                    .insert("worker".into(), Outcome::Failed("503".into()));
            },
            &["web", "worker"],
        );
        assert!(
            matches!(run.status, Status::RolledBack(_)),
            "{:?}",
            run.status
        );
        assert_eq!(entry.current.value, "old-value");
        assert!(cons.redeployed.contains(&"web".to_string()));
        assert!(
            world.borrow().revoke_calls.is_empty(),
            "the old credential was never revoked"
        );
    }

    #[test]
    fn the_stop_rollback_rule_leaves_things_for_a_person() {
        let world = RefCell::new(World::default());
        let mut issuer = FakeIssuer::new(&world, Class::CreateOverlapping);
        let mut run = Run::plan(
            "r1",
            "E",
            2,
            Class::CreateOverlapping,
            Policy {
                revoke: RevokeRule::AfterAllChecked,
                rollback: RollbackRule::Stop,
            },
            "old",
        );
        let mut cons = FakeConsumers::new(&["web"]);
        cons.check
            .insert("web".into(), Outcome::Failed("down".into()));
        let mut entry = FakeEntry::new();
        let mut store = MemStore::default();
        let gen = || String::new();
        Engine {
            issuer: &mut issuer,
            consumers: &mut cons,
            entry: &mut entry,
            store: &mut store,
            generate: &gen,
        }
        .run(&mut run)
        .unwrap();
        assert!(matches!(run.status, Status::NeedsAttention(_)));
        assert!(cons.redeployed.is_empty());
    }

    #[test]
    fn a_lost_revoke_is_reconciled_and_never_blindly_repeated() {
        let (run, world, _, _, _) = engine_run(
            Class::CreateOverlapping,
            |i, _, _| i.revoke = Fault::LostAfter,
            &["web"],
        );
        assert_eq!(run.status, Status::Succeeded);
        assert_eq!(
            world
                .borrow()
                .calls
                .iter()
                .filter(|c| *c == "revoke")
                .count(),
            1,
            "the lost revoke was reconciled as done, not repeated"
        );
        // A lost revoke that did not take effect leaves it pending, with the old credential valid.
        let (run, world, _, _, _) = engine_run(
            Class::CreateOverlapping,
            |i, _, _| i.revoke = Fault::LostBefore,
            &["web"],
        );
        assert!(
            matches!(run.status, Status::RevocationPending(_)),
            "{:?}",
            run.status
        );
        assert!(!world.borrow().revoked.contains("old"));
    }

    #[test]
    fn the_never_policy_and_a_failed_commit_stop_before_any_revoke() {
        let world = RefCell::new(World::default());
        let mut issuer = FakeIssuer::new(&world, Class::CreateOverlapping);
        let mut run = Run::plan(
            "r1",
            "E",
            2,
            Class::CreateOverlapping,
            Policy {
                revoke: RevokeRule::Never,
                rollback: RollbackRule::RestoreOnFailure,
            },
            "old",
        );
        let mut cons = FakeConsumers::new(&["web"]);
        let mut entry = FakeEntry::new();
        let mut store = MemStore::default();
        let gen = || String::new();
        Engine {
            issuer: &mut issuer,
            consumers: &mut cons,
            entry: &mut entry,
            store: &mut store,
            generate: &gen,
        }
        .run(&mut run)
        .unwrap();
        assert!(matches!(run.status, Status::RevocationPending(_)));
        assert!(world.borrow().revoke_calls.is_empty());

        let (run, world, cons, _, _) = engine_run(
            Class::CreateOverlapping,
            |_, _, e| e.fail_commit = true,
            &["web"],
        );
        assert!(
            matches!(run.status, Status::NeedsAttention(_)),
            "{:?}",
            run.status
        );
        assert!(
            cons.deployed.is_empty(),
            "nothing is deployed before the value is stored"
        );
        assert!(world.borrow().revoke_calls.is_empty());
    }

    #[test]
    fn an_issuer_that_cannot_rotate_this_way_fails_before_touching_anything() {
        let world = RefCell::new(World::default());
        let mut issuer = FakeIssuer::new(&world, Class::CreateOverlapping);
        let mut run = Run::plan(
            "r1",
            "E",
            2,
            Class::RollProviderGenerated,
            Policy::default(),
            "old",
        );
        let mut cons = FakeConsumers::new(&["web"]);
        let mut entry = FakeEntry::new();
        let mut store = MemStore::default();
        let gen = || String::new();
        Engine {
            issuer: &mut issuer,
            consumers: &mut cons,
            entry: &mut entry,
            store: &mut store,
            generate: &gen,
        }
        .run(&mut run)
        .unwrap();
        assert!(matches!(run.status, Status::Failed(_)));
        assert!(world.borrow().calls.is_empty());
    }

    /// Rule 1 as a property: whatever steps fail, in whatever combination, `revoke` is never
    /// called unless every required consumer was deployed to and checked in this run.
    #[test]
    fn revocation_never_precedes_checks_over_every_combination_of_step_failures() {
        let faults = [
            Fault::None,
            Fault::Reject,
            Fault::LostAfter,
            Fault::LostBefore,
        ];
        let outcomes = [Outcome::Ok, Outcome::Failed("x".into()), Outcome::Offline];
        let classes = [
            Class::CreateOverlapping,
            Class::SetValueClientGenerated,
            Class::RollProviderGenerated,
        ];
        let mut runs = 0;
        for class in classes {
            for create in faults {
                for set_value in faults {
                    for roll in faults {
                        for verify in faults {
                            for revoke in faults {
                                for deploy in &outcomes {
                                    for check in &outcomes {
                                        let (run, world, cons, _, _) = engine_run(
                                            class,
                                            |i, c, _| {
                                                i.create = create;
                                                i.set_value = set_value;
                                                i.roll = roll;
                                                i.verify = verify;
                                                i.revoke = revoke;
                                                c.deploy.insert("b".into(), deploy.clone());
                                                c.check.insert("b".into(), check.clone());
                                            },
                                            &["a", "b"],
                                        );
                                        runs += 1;
                                        let revoked_called =
                                            !world.borrow().revoke_calls.is_empty();
                                        if revoked_called {
                                            let all =
                                                run.consumers.values().all(|s| s == "checked")
                                                    && run.consumers.len() == 2;
                                            assert!(
                                                all,
                                                "revoked before every consumer was checked: {class:?} {create:?} {set_value:?} {roll:?} {verify:?} {revoke:?} {deploy:?} {check:?} => {:?} {:?}",
                                                run.status, run.consumers
                                            );
                                            assert!(cons.deployed.len() == 2);
                                        }
                                        assert!(
                                            world.borrow().revoke_calls.len() <= 1,
                                            "revoked twice"
                                        );
                                        assert!(runs > 0);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(runs > 5000, "the property was exercised over {runs} runs");
    }

    /// Rule 5: a process that stops after any transition resumes from the saved run and reaches the
    /// same end, without issuing a second credential or revoking twice.
    #[test]
    fn a_run_resumes_from_every_saved_transition_without_repeating_a_destructive_step() {
        // First, record the uninterrupted run's saved states.
        let (_, _, _, _, full) =
            engine_run(Class::CreateOverlapping, |_, _, _| {}, &["web", "worker"]);
        assert!(full.saved.len() >= 6);
        for (n, snapshot) in full.saved.iter().enumerate() {
            let world = RefCell::new(World::default());
            // The issuer state at that point: replay what the saved steps imply by starting fresh
            // for steps before issuing, or with the credential created for later ones.
            let mut issuer = FakeIssuer::new(&world, Class::CreateOverlapping);
            let mut entry = FakeEntry::new();
            let mut store = MemStore::default();
            let mut cons = FakeConsumers::new(&["web", "worker"]);
            if snapshot.new_id.is_some() {
                world
                    .borrow_mut()
                    .issuer
                    .insert(snapshot.name.clone(), "provider-value-1".into());
                store.put_pending(&snapshot.id, "provider-value-1").unwrap();
                issuer.seq = 1;
            }
            if snapshot.step > Step::Committing {
                entry
                    .commit(
                        &Credential {
                            id: snapshot.name.clone(),
                            value: "provider-value-1".into(),
                        },
                        2,
                    )
                    .unwrap();
                store.clear_pending(&snapshot.id).unwrap();
            }
            for (id, state) in &snapshot.consumers {
                if state == "deployed" || state == "checked" {
                    cons.deployed.insert(id.clone(), "provider-value-1".into());
                }
            }
            let mut resumed = snapshot.clone();
            let gen = || String::new();
            Engine {
                issuer: &mut issuer,
                consumers: &mut cons,
                entry: &mut entry,
                store: &mut store,
                generate: &gen,
            }
            .run(&mut resumed)
            .unwrap();
            assert_eq!(resumed.status, Status::Succeeded, "resuming after save {n}");
            let calls = world.borrow().calls.clone();
            assert!(
                calls.iter().filter(|c| *c == "create").count() <= 1,
                "{n}: {calls:?}"
            );
            assert_eq!(
                world.borrow().revoke_calls.len() + usize::from(snapshot.revoked),
                1,
                "{n}: revoked exactly once"
            );
        }
    }

    #[test]
    fn runs_and_the_pending_value_are_persisted_in_the_vault_database() {
        let dir = std::env::temp_dir().join(format!("vc-rot-{}", crate::new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        let key =
            crate::derive_key(&crate::new_uuid(), &crate::new_uuid().as_bytes()[..16]).unwrap();
        let conn = crate::open_db(&dir.join("vault.db"), &key).unwrap();
        crate::init_schema(&conn).unwrap();
        init_schema(&conn).unwrap();
        let mut store = SqlStore { conn: &conn };
        let run = Run::plan(
            "r9",
            "Entry",
            3,
            Class::SetValueClientGenerated,
            Policy::default(),
            "old",
        );
        store.save(&run).unwrap();
        store.put_pending("r9", "pending-value").unwrap();
        assert_eq!(load_run(&conn, "r9").unwrap().unwrap(), run);
        assert_eq!(store.pending("r9").as_deref(), Some("pending-value"));
        assert!(
            !serde_json::to_string(&run)
                .unwrap()
                .contains("pending-value"),
            "runs hold no values"
        );
        store.clear_pending("r9").unwrap();
        assert!(store.pending("r9").is_none());
        assert_eq!(list_runs(&conn).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
