// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Terminal formatting helpers and the destructive-action confirm prompt.

use crate::error::{CliError, CliResult};
use std::io::{IsTerminal, Write};

/// Pad or ellipsise `s` to exactly `width` display columns.
pub fn cell(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > width {
        let cut: String = chars[..width.saturating_sub(1)].iter().collect();
        format!("{cut}…")
    } else {
        format!("{s:width$}")
    }
}

pub fn fmt_entries(entries: &[serde_json::Value]) {
    println!(
        "{:<30} {:<20} {:<16} {:<12}",
        "Provider", "Account", "Type", "Expires"
    );
    println!("{}", "-".repeat(82));
    for e in entries {
        let provider = e.get("provider").and_then(|v| v.as_str()).unwrap_or("—");
        let account = e
            .get("account_name")
            .and_then(|v| v.as_str())
            .unwrap_or("—");
        let stype = e
            .get("secretType")
            .and_then(|v| v.as_str())
            .unwrap_or("api_key");
        let expires = e.get("expires_at").and_then(|v| v.as_str()).unwrap_or("—");
        let exp: String = expires.chars().take(12).collect();
        println!(
            "{} {} {} {}",
            cell(provider, 30),
            cell(account, 20),
            cell(stype, 16),
            exp
        );
    }
}

/// Ask before doing something irreversible.
///
/// `assume_yes` is the `--yes` flag. A non-tty stdin (a pipe, a cron job) is
/// refused rather than silently confirmed: a script that forgot `--yes` must
/// fail loudly, not delete.
pub fn confirm(question: &str, assume_yes: bool) -> CliResult<bool> {
    if assume_yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err(CliError::new(
            crate::error::Code::NeedsConfirmation,
            format!("{question} — refusing without a terminal; pass --yes to confirm"),
        ));
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| CliError::from(e.to_string()))?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// Read a value from stdin when the caller passed `-` or used a `--stdin` flag.
pub fn read_stdin() -> CliResult<String> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::from(e.to_string()))?;
    Ok(buf.trim_end_matches('\n').to_string())
}

/// Create or truncate `path` and write `content`, readable by the owner only.
///
/// The file is opened with mode 0600 and, because opening an existing file keeps
/// its old mode, tightened to 0600 *before* anything is written. A secret never
/// sits in a world-readable file, not even for the instant between `write` and
/// a later `chmod`. On other platforms the directory's access list applies.
fn write_owner_only(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(content.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
    }
}

/// Write credential material to `path` with 0600 permissions.
///
/// Separate from [`emit`], which is for artefacts: this is for a value that is
/// itself a secret, so the mode is not optional and there is no stdout branch.
pub fn write_secret_file(path: &std::path::Path, content: &str) -> CliResult {
    write_owner_only(path, content)
        .map_err(|e| CliError::from(format!("Cannot write {}: {e}", path.display())))?;
    crate::matlog::note("file", &path.display().to_string(), content);
    Ok(())
}

/// Write `content` to `path`, or to stdout when `path` is `None`.
pub fn emit(content: &str, path: Option<&std::path::Path>) -> CliResult {
    match path {
        Some(p) => {
            // Whatever `--out` receives may hold a real value (the help of each
            // `--out` says 0600), so every file written here is owner-only.
            write_owner_only(p, content)
                .map_err(|e| CliError::from(format!("Cannot write {}: {e}", p.display())))?;
            eprintln!("Wrote {}", p.display());
            crate::matlog::note("file", &p.display().to_string(), content);
            Ok(())
        }
        None => {
            if crate::out::revealing() {
                crate::matlog::note("stdout", "--reveal", content);
            }
            print!("{content}");
            if !content.ends_with('\n') {
                println!();
            }
            Ok(())
        }
    }
}
