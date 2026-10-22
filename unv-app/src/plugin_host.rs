// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Runs a plugin as a supervised child process speaking `unv-plugin-rpc/1`: one JSON object per
//! line on stdin and stdout, `{"id":N,"method":..,"params":{..}}` answered by `{"id":N,"result":..}`
//! or `{"id":N,"error":{"message":..}}`.
//!
//! The host decides what the plugin can see and for how long:
//!
//! - a **cleared environment** plus the credential slots the user bound, as `UNV_SLOT_<NAME>`
//!   variables (never arguments), and a private scratch directory as `TMPDIR` and working directory;
//! - a **deadline per call**; a plugin that does not answer is killed, and the call is `Lost` (for
//!   `execute` that means the step is uncertain and must be reconciled, never silently repeated);
//! - a **cap on each line and on stderr**, so a plugin cannot fill memory;
//! - **scrubbed stderr** ([`crate::tool_exec::scrub`]) before it can reach a log or a terminal;
//! - the **program's hash re-checked** against the installed record before the process starts;
//! - **shutdown**: asked politely, then killed at a deadline. `Drop` always kills the child, and the
//!   caller drops the plugin when the vault locks.
//!
//! This is supervision, not a sandbox: the plugin runs with the user's rights.

use crate::tool_exec::{scrub, BASE_ENV};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vault_core::plugin::{self, Installed, Manifest};

const MAX_LINE: usize = 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// The plugin could not be started (changed on disk, missing, not executable).
    Start(String),
    /// No answer in time; the process was killed. The call may have taken effect.
    Lost(String),
    /// The process ended without answering.
    Crashed(String),
    /// The plugin answered something that is not the protocol.
    Protocol(String),
    /// The plugin said no.
    Remote(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start(m)
            | Self::Lost(m)
            | Self::Crashed(m)
            | Self::Protocol(m)
            | Self::Remote(m) => f.write_str(m),
        }
    }
}

pub struct Plugin {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Result<String, String>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    next_id: u64,
    scratch: PathBuf,
    pub manifest: Manifest,
}

impl Plugin {
    /// Starts the plugin installed at `dir`. `installed` carries the hash recorded at install; pass
    /// `None` only for conformance runs of a plugin that is not installed.
    pub fn start(
        dir: &std::path::Path,
        manifest: &Manifest,
        installed: Option<&Installed>,
        slots: &[(String, String)],
    ) -> Result<Self, PluginError> {
        if let Some(rec) = installed {
            plugin::check_provenance(rec, manifest).map_err(PluginError::Start)?;
        }
        let scratch = std::env::temp_dir().join(format!("unv-plugin-{}", vault_core::new_uuid()));
        vault_core::create_private_dir(&scratch).map_err(PluginError::Start)?;
        let program = dir.join(&manifest.exec);
        let mut command = Command::new(&program);
        command
            .args(&manifest.args)
            .env_clear()
            .current_dir(&scratch);
        for name in BASE_ENV {
            if let Some(v) = std::env::var_os(name) {
                command.env(name, v);
            }
        }
        command.env("TMPDIR", &scratch);
        for (slot, value) in slots {
            if manifest.slots.contains(slot) {
                command.env(
                    format!("UNV_SLOT_{}", slot.to_uppercase().replace('-', "_")),
                    value,
                );
            }
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut attempts = 0;
        let mut child = loop {
            match command.spawn() {
                Ok(c) => break c,
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && attempts < 20 => {
                    attempts += 1;
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&scratch);
                    return Err(PluginError::Start(format!("{}: {e}", program.display())));
                }
            }
        };
        let stdin = child.stdin.take();
        let (tx, lines) = mpsc::channel();
        if let Some(out) = child.stdout.take() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(out.take(u64::MAX));
                loop {
                    let mut line = Vec::new();
                    let mut limited = (&mut reader).take(MAX_LINE as u64 + 1);
                    match limited.read_until(b'\n', &mut line) {
                        Ok(0) => break,
                        Ok(_) if line.len() > MAX_LINE => {
                            let _ = tx.send(Err("a reply line exceeded the size limit".into()));
                            break;
                        }
                        Ok(_) => {
                            let text = String::from_utf8_lossy(&line).trim().to_string();
                            if !text.is_empty() && tx.send(Ok(text)).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e.to_string()));
                            break;
                        }
                    }
                }
            });
        }
        let stderr = Arc::new(Mutex::new(Vec::new()));
        if let Some(mut err) = child.stderr.take() {
            let sink = stderr.clone();
            std::thread::spawn(move || {
                let mut chunk = [0u8; 4096];
                while let Ok(n) = err.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    let mut buf = sink.lock().unwrap();
                    if buf.len() < MAX_STDERR {
                        let room = MAX_STDERR - buf.len();
                        buf.extend_from_slice(&chunk[..n.min(room)]);
                    }
                }
            });
        }
        Ok(Self {
            child,
            stdin,
            lines,
            stderr,
            next_id: 1,
            scratch,
            manifest: manifest.clone(),
        })
    }

    /// What the plugin wrote to stderr so far, scrubbed.
    pub fn stderr(&self) -> String {
        scrub(&String::from_utf8_lossy(&self.stderr.lock().unwrap()))
    }

    /// The raw stderr, for the conformance check that looks for leaked secrets. Never print it.
    pub(crate) fn stderr_raw(&self) -> String {
        String::from_utf8_lossy(&self.stderr.lock().unwrap()).into_owned()
    }

    pub fn scratch(&self) -> &std::path::Path {
        &self.scratch
    }

    pub fn call(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, PluginError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "id": id, "method": method, "params": params });
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(PluginError::Crashed("the plugin is not running".into()));
        };
        if writeln!(stdin, "{request}")
            .and_then(|_| stdin.flush())
            .is_err()
        {
            return Err(PluginError::Crashed(
                self.crash_note("the plugin closed its input"),
            ));
        }
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(Ok(line)) => {
                    let reply: Value = serde_json::from_str(&line).map_err(|_| {
                        PluginError::Protocol("the plugin printed a line that is not JSON".into())
                    })?;
                    if reply["id"] != json!(id) {
                        continue; // a late answer to an earlier call
                    }
                    if let Some(err) = reply.get("error") {
                        return Err(PluginError::Remote(scrub(
                            err["message"]
                                .as_str()
                                .unwrap_or("the plugin reported an error"),
                        )));
                    }
                    return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
                }
                Ok(Err(e)) => return Err(PluginError::Protocol(e)),
                Err(RecvTimeoutError::Timeout) => {
                    self.kill();
                    return Err(PluginError::Lost(format!(
                        "no answer to {method} in {}s; the plugin was stopped",
                        timeout.as_secs()
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(PluginError::Crashed(
                        self.crash_note("the plugin exited without answering"),
                    ));
                }
            }
        }
    }

    fn crash_note(&mut self, base: &str) -> String {
        let status = self.child.wait().ok().and_then(|s| s.code());
        let err = self.stderr();
        match (status, err.is_empty()) {
            (Some(c), true) => format!("{base} (exit {c})"),
            (Some(c), false) => format!("{base} (exit {c}): {err}"),
            (None, _) => base.to_string(),
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stdin = None;
    }

    /// Asks the plugin to stop, then kills it at the deadline. Returns whether it stopped by itself.
    pub fn shutdown(&mut self, deadline: Duration) -> bool {
        let _ = self.call("shutdown", json!({}), deadline.min(Duration::from_secs(2)));
        self.stdin = None; // closing stdin is the second, protocol-free hint
        let end = Instant::now() + deadline;
        while Instant::now() < end {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.kill();
        false
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        self.kill();
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

fn check(name: &'static str, result: Result<(), String>) -> Check {
    match result {
        Ok(()) => Check {
            name,
            ok: true,
            detail: String::new(),
        },
        Err(detail) => Check {
            name,
            ok: false,
            detail,
        },
    }
}

const CANARY: &str = "canary-9f3a7c2e41b8d6";

/// The conformance suite behind `unv plugin test`: what every plugin must do for the host to trust
/// a step it ran. Each check starts its own process.
pub fn conformance(dir: &std::path::Path, manifest: &Manifest) -> Vec<Check> {
    let t = Duration::from_secs(5);
    let slots: Vec<(String, String)> = manifest
        .slots
        .iter()
        .map(|s| (s.clone(), CANARY.to_string()))
        .collect();
    let start = || Plugin::start(dir, manifest, None, &slots).map_err(|e| e.to_string());
    let mut out = Vec::new();

    out.push(check(
        "handshake",
        (|| {
            let mut p = start()?;
            let r = p
                .call(
                    "handshake",
                    json!({ "host": "unv", "protocol": plugin::PROTOCOL }),
                    t,
                )
                .map_err(|e| e.to_string())?;
            if r["protocol"] != plugin::PROTOCOL {
                return Err(format!("answered protocol {}", r["protocol"]));
            }
            if r["name"] != manifest.name.as_str() {
                return Err(format!(
                    "answered name {}, the manifest says {}",
                    r["name"], manifest.name
                ));
            }
            Ok(())
        })(),
    ));

    out.push(check(
        "plan_has_no_side_effects",
        (|| {
            let mut p = start()?;
            let before = listing(p.scratch());
            let a = p
                .call("plan", json!({ "op": "probe", "params": {} }), t)
                .map_err(|e| e.to_string())?;
            let b = p
                .call("plan", json!({ "op": "probe", "params": {} }), t)
                .map_err(|e| e.to_string())?;
            if listing(p.scratch()) != before {
                return Err("plan wrote to its scratch directory".into());
            }
            if a != b {
                return Err("planning twice gave different answers".into());
            }
            Ok(())
        })(),
    ));

    out.push(check(
        "execute_is_idempotent",
        (|| {
            let mut p = start()?;
            let params = json!({ "op": "probe", "params": {}, "idempotency_key": "k-1" });
            let a = p
                .call("execute", params.clone(), t)
                .map_err(|e| e.to_string())?;
            let b = p.call("execute", params, t).map_err(|e| e.to_string())?;
            if a != b {
                return Err(
                    "the same idempotency key gave a different result the second time".into(),
                );
            }
            Ok(())
        })(),
    ));

    out.push(check(
        "reconcile_answers",
        (|| {
            let mut p = start()?;
            let r = p
                .call(
                    "reconcile",
                    json!({ "op": "probe", "idempotency_key": "k-never-run" }),
                    t,
                )
                .map_err(|e| e.to_string())?;
            match r["state"].as_str() {
                Some("done" | "not_done" | "unknown") => Ok(()),
                other => Err(format!(
                    "state must be done, not_done or unknown, got {other:?}"
                )),
            }
        })(),
    ));

    out.push(check(
        "shutdown_within_deadline",
        (|| {
            let mut p = start()?;
            p.call(
                "handshake",
                json!({ "host": "unv", "protocol": plugin::PROTOCOL }),
                t,
            )
            .map_err(|e| e.to_string())?;
            if p.shutdown(Duration::from_secs(2)) {
                Ok(())
            } else {
                Err("still running two seconds after shutdown; it was killed".into())
            }
        })(),
    ));

    out.push(check(
        "stderr_holds_no_secrets",
        (|| {
            let mut p = start()?;
            p.call(
                "handshake",
                json!({ "host": "unv", "protocol": plugin::PROTOCOL }),
                t,
            )
            .map_err(|e| e.to_string())?;
            p.call(
                "execute",
                json!({ "op": "probe", "params": {}, "idempotency_key": "k-2" }),
                t,
            )
            .map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(50));
            if p.stderr_raw().contains(CANARY) {
                return Err("the plugin wrote a credential slot's value to stderr".into());
            }
            Ok(())
        })(),
    ));
    out
}

fn listing(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}
