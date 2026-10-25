// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Leak-scan import: turn what the user's own `gitleaks` finds in a folder into migration
//! candidates.
//!
//! gitleaks is run through [`crate::tool_exec`] (argv only, a cleared environment, a deadline, an
//! output cap) and is never bundled. Its JSON report lists secrets that already sit in plain text
//! on disk; each one becomes a candidate with its file and line, and nothing is written unless the
//! caller applies the plan. A scan prints fingerprints, never values. With `template`, the migration
//! also stores the file as a `${...}` template, so the secret can leave the file and the file can
//! still be rebuilt from the vault ([`crate::migrate`]).
//!
//! Only the working tree is scanned (`gitleaks dir`). A secret that lives only in Git history has no
//! text to cut out of, so it is out of scope: rotate it instead.
//!
//! TruffleHog is not used: it is AGPL-3.0 and its verification sends the secret to the live API.

use crate::access::Access;
use crate::error::{CliError, CliResult};
use crate::migrate::{self, LeakHit, Preview};
use crate::tool_exec::{self, ToolError, ToolSpec};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

/// `gitleaks dir` arrived in 8.19; before it the same job was `detect --no-git`.
const MIN_MINOR: u64 = 19;
const SCAN_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_REPORT: usize = 64 * 1024 * 1024;
const MAX_FILE: u64 = 8 * 1024 * 1024;
/// Where gitleaks reads its rules from, when the user points it somewhere other than the folder.
const GITLEAKS_ENV: &[&str] = &["GITLEAKS_CONFIG", "GITLEAKS_CONFIG_TOML"];

/// One secret gitleaks found. Holds the value, so it has no `Debug` and never reaches a report.
pub struct Finding {
    pub rule: String,
    pub description: String,
    /// Path relative to the scanned folder, with `/` separators.
    pub file: String,
    pub line: usize,
    pub secret: String,
}

impl Finding {
    /// A short hash of the value: enough to tell two findings apart, useless to anyone who lacks
    /// the secret.
    pub fn fingerprint(&self) -> String {
        hex::encode(Sha256::digest(self.secret.as_bytes()))[..12].to_string()
    }
}

#[derive(Serialize)]
pub struct Candidate {
    pub rule: String,
    pub description: String,
    pub file: String,
    pub line: usize,
    pub fingerprint: String,
}

pub fn candidates(findings: &[Finding]) -> Vec<Candidate> {
    findings
        .iter()
        .map(|f| Candidate {
            rule: f.rule.clone(),
            description: f.description.clone(),
            file: f.file.clone(),
            line: f.line,
            fingerprint: f.fingerprint(),
        })
        .collect()
}

fn tool_error(e: ToolError) -> CliError {
    match e {
        ToolError::Missing(_) => CliError::not_found(format!("gitleaks: {e}")),
        _ => CliError::invalid(format!("gitleaks: {e}")),
    }
}

/// `gitleaks version` prints a bare `v8.30.1`; refuse anything older than the report format this
/// reader was checked against.
pub fn check_version(tool: &str) -> CliResult<String> {
    let line = tool_exec::version(tool, &["version"])
        .ok_or_else(|| tool_error(ToolError::Missing(tool.to_string())))?;
    let numbers: Vec<u64> = line
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split('.')
        .map_while(|p| p.parse().ok())
        .collect();
    match numbers.as_slice() {
        [8, minor, ..] if *minor >= MIN_MINOR => Ok(line),
        _ => Err(CliError::invalid(format!(
            "gitleaks 8.{MIN_MINOR} or newer is required (found `{line}`)"
        ))),
    }
}

/// Reads a gitleaks JSON report. `root` is the folder that was scanned, so paths come out relative
/// to it; a finding outside it, or without a secret, is dropped.
pub fn parse_report(root: &Path, json: &[u8]) -> CliResult<Vec<Finding>> {
    let value: Value = serde_json::from_slice(json)
        .map_err(|e| CliError::invalid(format!("gitleaks report is not JSON: {e}")))?;
    let items = value
        .as_array()
        .ok_or_else(|| CliError::invalid("gitleaks report is not a list"))?;
    let text = |item: &Value, key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let mut out = Vec::new();
    for item in items {
        let secret = text(item, "Secret");
        let full = text(item, "File");
        let Ok(rel) = Path::new(&full).strip_prefix(root) else {
            continue;
        };
        if secret.is_empty() {
            continue;
        }
        out.push(Finding {
            rule: text(item, "RuleID"),
            description: text(item, "Description"),
            file: rel.to_string_lossy().replace('\\', "/"),
            line: item
                .get("StartLine")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize,
            secret,
        });
    }
    out.sort_by(|a, b| (&a.file, a.line, &a.rule).cmp(&(&b.file, b.line, &b.rule)));
    Ok(out)
}

/// Runs the user's gitleaks over `root`, working tree only.
pub fn scan(root: &Path, tool: &str) -> CliResult<Vec<Finding>> {
    let root = root
        .canonicalize()
        .map_err(|e| CliError::not_found(format!("Cannot read {}: {e}", root.display())))?;
    check_version(tool)?;
    let spec = ToolSpec::new(
        tool,
        &[
            "dir",
            "--report-format",
            "json",
            "--report-path",
            "-",
            "--exit-code",
            "0",
            "--no-banner",
            "--log-level",
            "error",
        ],
    )
    .arg(root.to_string_lossy())
    .pass_env(GITLEAKS_ENV)
    .timeout(SCAN_TIMEOUT)
    .max_stdout(MAX_REPORT);
    let output = tool_exec::run(&spec).map_err(tool_error)?;
    parse_report(&root, &output.stdout)
}

/// One plan per file that holds findings. Nothing is written here.
pub fn plan(
    access: &Access,
    root: &Path,
    findings: &[Finding],
    template: bool,
) -> CliResult<Vec<migrate::Plan>> {
    let root = root
        .canonicalize()
        .map_err(|e| CliError::not_found(format!("Cannot read {}: {e}", root.display())))?;
    let mut by_file: BTreeMap<&str, Vec<LeakHit>> = BTreeMap::new();
    for f in findings {
        by_file.entry(&f.file).or_default().push(LeakHit {
            rule: f.rule.clone(),
            line: f.line,
            secret: f.secret.clone(),
        });
    }
    let mut plans = Vec::new();
    for (file, hits) in by_file {
        let path = root.join(file);
        // A link inside the folder must not lead the read outside it.
        let real = path
            .canonicalize()
            .map_err(|e| CliError::not_found(format!("Cannot read {file}: {e}")))?;
        if !real.starts_with(&root) {
            return Err(CliError::invalid(format!(
                "{file} resolves outside the folder"
            )));
        }
        let size = std::fs::metadata(&real).map(|m| m.len()).unwrap_or(0);
        if size > MAX_FILE {
            return Err(CliError::invalid(format!("{file} is larger than 8 MiB")));
        }
        let text = std::fs::read_to_string(&real)
            .map_err(|e| CliError::invalid(format!("Cannot read {file} as text: {e}")))?;
        plans.push(migrate::plan_leaks(access, file, &text, &hits, template)?);
    }
    Ok(plans)
}

/// Writes every plan, or none: one conflicting file stops the whole import.
pub fn apply(access: &Access, plans: Vec<migrate::Plan>) -> CliResult<Vec<Preview>> {
    let conflicts: usize = plans.iter().map(|p| p.preview.counts.conflicts).sum();
    if conflicts > 0 {
        return Err(CliError::conflict(format!(
            "{conflicts} finding(s) conflict with the vault; nothing was written"
        )));
    }
    plans
        .into_iter()
        .map(|p| migrate::apply(access, p))
        .collect()
}
