// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Phase 34 — the filesystem half of a node: hash a target, apply bytes to it
//! transactionally, and keep the last few versions.
//!
//! The rules, in the order they matter:
//!
//! 1. The bytes must hash to what the hub said they hash to, **before anything
//!    is touched**. Phase 37's approval token binds to that hash.
//! 2. The write is temp file in the same directory, `fsync`, rename. A reader
//!    sees the old file or the new one, never half of either.
//! 3. The previous file is copied aside first and the last [`KEEP_VERSIONS`]
//!    are kept.
//! 4. `validate` runs against the file in place. Failing it restores the
//!    previous file and does not reload.
//! 5. `reload` runs last. Failing it restores the previous file and does not
//!    try again: the service is in a state only the operator can judge.
//!
//! Commands come from the node's own config and nowhere else.

use crate::nodes::Target;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const KEEP_VERSIONS: usize = 3;
/// How long `validate` or `reload` may run.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Hash of the file at `path`: `Ok(None)` when it does not exist.
pub fn hash_file(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(sha256_hex(&b))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Runs `cmd` through the platform shell and returns `Ok(())` on exit 0.
/// The error carries a few scrubbed lines of output, never the file's content.
///
/// Both pipes are drained on their own threads while the child runs. Waiting for the exit first and
/// reading afterwards deadlocks a validator that prints more than a pipe buffer (about 64 KB): it
/// blocks writing, never exits, and the node reports a timeout for a tool that was working.
fn run(cmd: &str, content: &str, timeout: Duration) -> Result<(), String> {
    #[cfg(windows)]
    let mut c = {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    };
    #[cfg(not(windows))]
    let mut c = {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };
    let mut child = c
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start '{cmd}': {e}"))?;
    // Each reader keeps the first 8 KiB and discards the rest, so a validator that prints a megabyte
    // costs nothing but the read.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        if let Some(mut pipe) = pipe {
            std::thread::spawn(move || {
                let mut head = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    match pipe.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if head.len() < 8192 {
                                let take = n.min(8192 - head.len());
                                head.extend_from_slice(&chunk[..take]);
                            }
                        }
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&head).into_owned());
            });
        }
        rx
    };
    let out_rx = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err_rx = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let start = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(s) => break s,
            None if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "'{cmd}' did not finish in {}s",
                    timeout.as_secs().max(1)
                ));
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    };
    if status.success() {
        return Ok(());
    }
    // A grandchild can keep a pipe open after the shell exits; do not wait for it.
    let mut text = err_rx
        .recv_timeout(Duration::from_millis(500))
        .unwrap_or_default();
    if text.trim().is_empty() {
        text = out_rx
            .recv_timeout(Duration::from_millis(500))
            .unwrap_or_default();
    }
    Err(format!(
        "'{cmd}' exited {}: {}",
        status.code().map_or("by signal".into(), |c| c.to_string()),
        scrub(&text, content)
    ))
}

/// The last three lines of `output`, each cut to 200 characters, with any line
/// that is also a line of the file being applied replaced. A validator that
/// echoes the offending line of a config would otherwise put a secret into an
/// error that travels to the hub and into its audit log.
pub fn scrub(output: &str, content: &str) -> String {
    let file_lines: std::collections::HashSet<&str> = content
        .lines()
        .map(str::trim)
        .filter(|l| l.len() >= 8)
        .collect();
    let lines: Vec<&str> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(3)..];
    tail.iter()
        .map(|l| {
            if file_lines.contains(l) || file_lines.iter().any(|f| l.contains(f)) {
                "[a line of the file elided]".to_string()
            } else {
                l.chars().take(200).collect()
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn versions_dir(state_dir: &Path, id: &str) -> PathBuf {
    state_dir.join("versions").join(id)
}

static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn write_atomic(
    path: &Path,
    bytes: &[u8],
    mode_from: Option<&std::fs::Metadata>,
) -> Result<(), String> {
    use std::io::Write;
    let dir = path.parent().ok_or("target has no parent directory")?;
    let name = path.file_name().ok_or("target has no file name")?;
    // A name no other writer shares: a fixed one let two applies truncate and rename each
    // other's half-written file.
    let tmp = dir.join(format!(
        ".{}.unv-node.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        // Keep the mode the operator gave the file; a new one is private.
        opts.mode(mode_from.map_or(0o600, |m| m.mode() & 0o7777));
    }
    #[cfg(not(unix))]
    let _ = mode_from;
    let mut f = opts
        .open(&tmp)
        .map_err(|e| format!("cannot write beside {}: {e}", path.display()))?;
    f.write_all(bytes).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    crate::platform::replace_file(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot replace {}: {e}", path.display())
    })?;
    #[cfg(unix)]
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Puts the previous file back, but only if the file still holds what this apply wrote.
/// Validation takes seconds; if somebody edited the file meanwhile, rolling back would
/// destroy their edit, so it is left alone and the failure says so.
fn restore(
    path: &Path,
    wrote: &[u8],
    previous: &Option<Vec<u8>>,
    meta: Option<&std::fs::Metadata>,
) -> Result<(), String> {
    if std::fs::read(path).ok().as_deref() != Some(wrote) {
        return Err(
            "the file was changed by someone else meanwhile, so it was left as it is".into(),
        );
    }
    match previous {
        Some(b) => write_atomic(path, b, meta),
        None => std::fs::remove_file(path).map_err(|e| e.to_string()),
    }
}

fn keep_version(state_dir: &Path, id: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = versions_dir(state_dir, id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = format!("{secs:012}-{}", &sha256_hex(bytes)[..8]);
    let p = dir.join(name);
    // 0600 through the same helper: a kept copy of wg0.conf is as secret as
    // wg0.conf.
    write_atomic(&p, bytes, None)?;
    let mut all: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    all.sort();
    while all.len() > KEEP_VERSIONS {
        let _ = std::fs::remove_file(all.remove(0));
    }
    Ok(())
}

/// The versions kept for a target, oldest first.
pub fn kept_versions(state_dir: &Path, id: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(versions_dir(state_dir, id))
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

#[derive(Debug, PartialEq)]
pub enum Applied {
    /// The file already had these bytes.
    Unchanged,
    Written,
}

/// The file a root-owned systemd `.path` unit watches to reload a target's service.
/// The agent writes it only after validation passed. Watching the target file itself
/// reloaded the service on the rename, before the agent had validated anything, and
/// again when a failed validation restored the previous file.
pub fn reload_signal_path(state_dir: &Path, id: &str) -> PathBuf {
    state_dir.join("reload").join(id)
}

/// The same location as text for a systemd unit, which is Linux syntax whatever host
/// generates it: always `/`, where `Path::display` on Windows would write `\`.
pub fn reload_signal_unit_path(state_dir: &str, id: &str) -> String {
    format!("{}/reload/{id}", state_dir.trim_end_matches('/'))
}

fn signal_reload(state_dir: &Path, id: &str, sha: &str) -> Result<(), String> {
    let p = reload_signal_path(state_dir, id);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // The content differs on every applied version, so the watcher sees a change.
    write_atomic(&p, format!("{sha}\n").as_bytes(), None)
}

/// The steps a successful apply went through, in order: `written`, `validated`, `reloaded`,
/// `checked`. A step the target does not configure is absent, so a report says exactly what ran.
pub type Phases = Vec<&'static str>;

/// A built-in health probe for `check`: `http://host:port/path` gets one GET and wants a 2xx.
/// Anything else is run as a command (use `curl -fsS https://...` for TLS).
fn probe_http(url: &str, timeout: Duration) -> Result<(), String> {
    use std::io::Write;
    let rest = url.strip_prefix("http://").ok_or("not an http:// URL")?;
    let (hostport, path) = rest
        .split_once('/')
        .map_or((rest, "/".to_string()), |(h, p)| (h, format!("/{p}")));
    let addr = if hostport.contains(':') {
        hostport.to_string()
    } else {
        format!("{hostport}:80")
    };
    let sock = std::net::ToSocketAddrs::to_socket_addrs(&addr)
        .map_err(|e| format!("{addr}: {e}"))?
        .next()
        .ok_or_else(|| format!("{addr}: no address"))?;
    let mut stream = std::net::TcpStream::connect_timeout(&sock, timeout)
        .map_err(|e| format!("could not connect to {addr}: {e}"))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    stream
        .write_all(
            format!("GET {path} HTTP/1.0\r\nHost: {hostport}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
    let mut head = [0u8; 64];
    let n = stream
        .read(&mut head)
        .map_err(|e| format!("no answer from {addr}: {e}"))?;
    let line = String::from_utf8_lossy(&head[..n]);
    let code = line.split_whitespace().nth(1).unwrap_or("");
    if code.starts_with('2') {
        Ok(())
    } else {
        Err(format!(
            "{url} answered {}",
            if code.is_empty() {
                "nothing readable"
            } else {
                code
            }
        ))
    }
}

fn run_check(check: &str, text: &str) -> Result<(), String> {
    if check.starts_with("http://") {
        probe_http(check, COMMAND_TIMEOUT)
    } else {
        run(check, text, COMMAND_TIMEOUT)
    }
}

/// Applies `content` to `target`. See the module comment for the contract.
pub fn apply(
    target: &Target,
    content: &[u8],
    expected_sha: &str,
    state_dir: &Path,
) -> Result<Applied, String> {
    apply_with_phases(target, content, expected_sha, state_dir).0
}

/// [`apply`], also returning the phases that completed, so a node can report `written`,
/// `validated`, `reloaded` and `checked` separately instead of one verdict.
///
/// Order: the new bytes are written beside the target; a `validate` command that contains `{file}`
/// runs against that temporary file and the target is replaced only if it passes (the live file is
/// never invalid); a `validate` without `{file}` runs against the file in place, as before. Then
/// `reload`, then `check` (a command, or an `http://` URL that must answer 2xx). A failing check
/// puts the previous file back and reloads again, because the service is now running the new
/// configuration.
pub fn apply_with_phases(
    target: &Target,
    content: &[u8],
    expected_sha: &str,
    state_dir: &Path,
) -> (Result<Applied, String>, Phases) {
    let mut phases: Phases = Vec::new();
    let result = apply_inner(target, content, expected_sha, state_dir, &mut phases);
    (result, phases)
}

fn apply_inner(
    target: &Target,
    content: &[u8],
    expected_sha: &str,
    state_dir: &Path,
    phases: &mut Phases,
) -> Result<Applied, String> {
    if target.mode != "push" || !target.apply {
        return Err(format!(
            "target '{}' is not set to apply; the node's config decides that, not the hub",
            target.id
        ));
    }
    if sha256_hex(content) != expected_sha {
        return Err(
            "content does not match the hash the hub announced; nothing was written".into(),
        );
    }
    let text = String::from_utf8_lossy(content).into_owned();
    let path = &target.path;
    let meta = std::fs::metadata(path).ok();
    let previous = match std::fs::read(path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if previous.as_deref() == Some(content) {
        return Ok(Applied::Unchanged);
    }
    if let Some(p) = &previous {
        keep_version(state_dir, &target.id, p)?;
    }

    // A path-taking validator checks a temporary file first, so the live file is never invalid.
    let path_validator = target.validate.as_deref().filter(|v| v.contains("{file}"));
    if let Some(v) = path_validator {
        let tmp = temp_beside(path)?;
        write_atomic(&tmp, content, meta.as_ref())?;
        let outcome = run(
            &v.replace("{file}", &shell_quote(&tmp)),
            &text,
            COMMAND_TIMEOUT,
        );
        let _ = std::fs::remove_file(&tmp);
        if let Err(e) = outcome {
            return Err(format!("validate failed, the file was not replaced: {e}"));
        }
        phases.push("validated");
    }
    write_atomic(path, content, meta.as_ref())?;
    phases.insert(0, "written");

    if let Some(v) = target.validate.as_deref().filter(|v| !v.contains("{file}")) {
        if let Err(e) = run(v, &text, COMMAND_TIMEOUT) {
            return Err(match restore(path, content, &previous, meta.as_ref()) {
                Ok(()) => format!("validate failed, previous file restored: {e}"),
                Err(r) => {
                    format!("validate failed AND restoring the previous file failed ({r}): {e}")
                }
            });
        }
        phases.push("validated");
    }
    if let Some(r) = &target.reload {
        if let Err(e) = run(r, &text, COMMAND_TIMEOUT) {
            return Err(match restore(path, content, &previous, meta.as_ref()) {
                Ok(()) => format!(
                    "reload failed, previous file restored and the service was not reloaded again: {e}"
                ),
                Err(x) => format!("reload failed AND restoring the previous file failed ({x}): {e}"),
            });
        }
        phases.push("reloaded");
    }
    if let Some(c) = &target.check {
        if let Err(e) = run_check(c, &text) {
            let restored = restore(path, content, &previous, meta.as_ref());
            // The service is running the new file: put the old one back and reload it.
            let reloaded = match (&restored, &target.reload) {
                (Ok(()), Some(r)) => run(r, &text, COMMAND_TIMEOUT).err(),
                _ => None,
            };
            return Err(match (restored, reloaded) {
                (Ok(()), None) => format!("check failed, previous file restored and reloaded: {e}"),
                (Ok(()), Some(x)) => format!(
                    "check failed, previous file restored but reloading it failed ({x}): {e}"
                ),
                (Err(x), _) => {
                    format!("check failed AND restoring the previous file failed ({x}): {e}")
                }
            });
        }
        phases.push("checked");
    }
    if let Err(e) = signal_reload(state_dir, &target.id, expected_sha) {
        return Err(match restore(path, content, &previous, meta.as_ref()) {
            Ok(()) => format!("could not signal the reload, previous file restored: {e}"),
            Err(x) => format!("could not signal the reload AND restoring failed ({x}): {e}"),
        });
    }
    Ok(Applied::Written)
}

/// A new, unused name beside `path` for a file that exists only to be validated.
fn temp_beside(path: &Path) -> Result<PathBuf, String> {
    let dir = path.parent().ok_or("target has no parent directory")?;
    let name = path.file_name().ok_or("target has no file name")?;
    Ok(dir.join(format!(
        ".{}.unv-check.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )))
}

/// A path as one shell word.
fn shell_quote(path: &Path) -> String {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    {
        format!("\"{text}\"")
    }
    #[cfg(not(windows))]
    {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        // A counter beside the process id, not the clock: two tests starting within one clock tick
        // (macOS has a coarse one) shared a directory and failed with "database is locked".
        let n = {
            use std::sync::atomic::{AtomicU64, Ordering};
            static SEQ: AtomicU64 = AtomicU64::new(0);
            format!(
                "{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::SeqCst)
            )
        };
        let d = std::env::temp_dir().join(format!("unv-apply-{tag}-{n}"));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn target(dir: &Path, validate: Option<&str>, reload: Option<&str>) -> Target {
        Target {
            id: "t".into(),
            path: dir.join("app.conf"),
            project: "p".into(),
            exporter: "nginx".into(),
            mode: "push".into(),
            apply: true,
            validate: validate.map(String::from),
            reload: reload.map(String::from),
            require_approval: false,
            check: None,
            accept_rule_approval: false,
        }
    }

    #[test]
    fn a_hash_mismatch_writes_nothing() {
        let d = scratch("hash");
        let t = target(&d, None, None);
        let e = apply(&t, b"new", &sha256_hex(b"other"), &d).unwrap_err();
        assert!(e.contains("does not match"));
        assert!(!t.path.exists());
    }

    #[test]
    fn apply_is_refused_unless_the_nodes_own_config_allows_it() {
        let d = scratch("noapply");
        let mut t = target(&d, None, None);
        t.apply = false;
        assert!(apply(&t, b"x", &sha256_hex(b"x"), &d)
            .unwrap_err()
            .contains("not set to apply"));
        t.apply = true;
        t.mode = "pull".into();
        assert!(apply(&t, b"x", &sha256_hex(b"x"), &d).is_err());
        assert!(!t.path.exists());
    }

    #[test]
    fn writes_the_file_and_keeps_the_previous_one() {
        let d = scratch("write");
        let t = target(&d, None, None);
        std::fs::write(&t.path, "old").unwrap();
        assert_eq!(
            apply(&t, b"new", &sha256_hex(b"new"), &d).unwrap(),
            Applied::Written
        );
        assert_eq!(std::fs::read_to_string(&t.path).unwrap(), "new");
        let kept = kept_versions(&d, "t");
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read_to_string(&kept[0]).unwrap(), "old");
        // Applying the same bytes again is a no-op, not another version.
        assert_eq!(
            apply(&t, b"new", &sha256_hex(b"new"), &d).unwrap(),
            Applied::Unchanged
        );
        assert_eq!(kept_versions(&d, "t").len(), 1);
    }

    #[test]
    fn only_the_last_three_versions_are_kept() {
        let d = scratch("keep");
        let t = target(&d, None, None);
        std::fs::write(&t.path, "v0").unwrap();
        for i in 1..=6 {
            let c = format!("v{i}");
            // Distinct seconds are not guaranteed; the hash in the name keeps
            // the files distinct and the sort stable enough for a count.
            apply(&t, c.as_bytes(), &sha256_hex(c.as_bytes()), &d).unwrap();
        }
        assert_eq!(kept_versions(&d, "t").len(), KEEP_VERSIONS);
    }

    // N5: the reload trigger is written after validation, not by the rename.
    #[cfg(unix)]
    #[test]
    fn the_reload_signal_is_written_only_after_a_file_validated() {
        let d = scratch("signal");
        let ok = target(&d, Some("exit 0"), None);
        std::fs::write(&ok.path, "old").unwrap();
        apply(&ok, b"new", &sha256_hex(b"new"), &d).unwrap();
        let sig = std::fs::read_to_string(reload_signal_path(&d, "t")).unwrap();
        assert_eq!(sig.trim(), sha256_hex(b"new"));

        let d2 = scratch("signal-bad");
        let bad = target(&d2, Some("exit 1"), None);
        std::fs::write(&bad.path, "old").unwrap();
        apply(&bad, b"new", &sha256_hex(b"new"), &d2).unwrap_err();
        assert!(
            !reload_signal_path(&d2, "t").exists(),
            "a rejected file must not trigger a reload"
        );
    }

    // N10: a failed validation put the old bytes back over whatever the file held by
    // then, including an edit made by someone else while the validator ran.
    #[cfg(unix)]
    #[test]
    fn a_rollback_never_overwrites_an_edit_made_during_validation() {
        let d = scratch("clobber");
        let t = target(&d, None, None);
        let mut t = t;
        t.validate = Some(format!(
            "echo edited-by-hand > '{}'; exit 1",
            t.path.display()
        ));
        std::fs::write(&t.path, "old").unwrap();
        let e = apply(&t, b"new", &sha256_hex(b"new"), &d).unwrap_err();
        assert!(e.contains("changed by someone else"), "{e}");
        assert_eq!(
            std::fs::read_to_string(&t.path).unwrap().trim(),
            "edited-by-hand"
        );
    }

    #[test]
    fn two_writes_never_share_a_temp_file() {
        let d = scratch("tmpname");
        let p = d.join("app.conf");
        write_atomic(&p, b"a", None).unwrap();
        write_atomic(&p, b"b", None).unwrap();
        let strays = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(strays, 0);
        assert_eq!(std::fs::read(&p).unwrap(), b"b");
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_validate_restores_the_old_file_and_never_reloads() {
        let d = scratch("validate");
        let marker = d.join("reloaded");
        let t = target(
            &d,
            Some("echo bad config >&2; exit 1"),
            Some(&format!("touch {}", marker.display())),
        );
        std::fs::write(&t.path, "old").unwrap();
        let e = apply(&t, b"new", &sha256_hex(b"new"), &d).unwrap_err();
        assert!(e.contains("validate failed, previous file restored"), "{e}");
        assert!(e.contains("bad config"), "{e}");
        assert_eq!(std::fs::read_to_string(&t.path).unwrap(), "old");
        assert!(!marker.exists(), "reload ran after a failed validate");
    }

    #[test]
    fn a_failing_validate_on_a_new_file_removes_it() {
        let d = scratch("validate-new");
        let t = target(&d, Some("exit 3"), None);
        assert!(apply(&t, b"new", &sha256_hex(b"new"), &d).is_err());
        assert!(!t.path.exists());
    }

    #[test]
    fn a_failing_reload_restores_the_old_file() {
        let d = scratch("reload");
        let t = target(&d, Some("exit 0"), Some("exit 2"));
        std::fs::write(&t.path, "old").unwrap();
        let e = apply(&t, b"new", &sha256_hex(b"new"), &d).unwrap_err();
        assert!(e.contains("reload failed, previous file restored"), "{e}");
        assert_eq!(std::fs::read_to_string(&t.path).unwrap(), "old");
    }

    #[cfg(unix)]
    #[test]
    fn validate_sees_the_new_file_in_place() {
        let d = scratch("inplace");
        let t = target(
            &d,
            Some(&format!("grep -q NEW {}", d.join("app.conf").display())),
            None,
        );
        std::fs::write(&t.path, "old").unwrap();
        assert!(apply(&t, b"NEW", &sha256_hex(b"NEW"), &d).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_runaway_command_is_killed_at_the_timeout() {
        let t0 = Instant::now();
        let e = run("sleep 30", "", Duration::from_millis(300)).unwrap_err();
        assert!(e.contains("did not finish"), "{e}");
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "waited for the command"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_files_mode_is_kept_and_a_new_one_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("mode");
        let t = target(&d, None, None);
        std::fs::write(&t.path, "old").unwrap();
        std::fs::set_permissions(&t.path, std::fs::Permissions::from_mode(0o640)).unwrap();
        apply(&t, b"new", &sha256_hex(b"new"), &d).unwrap();
        assert_eq!(
            std::fs::metadata(&t.path).unwrap().permissions().mode() & 0o777,
            0o640
        );

        let t2 = Target {
            id: "u".into(),
            path: d.join("fresh.conf"),
            ..t
        };
        apply(&t2, b"x", &sha256_hex(b"x"), &d).unwrap();
        assert_eq!(
            std::fs::metadata(&t2.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // The kept copy of a secret file is private too.
        let kept = kept_versions(&d, "t");
        assert_eq!(
            std::fs::metadata(&kept[0]).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn scrub_elides_a_line_of_the_file_and_keeps_the_rest() {
        let content = "server {\n  secret_token abcdef123456;\n}\n";
        let out = scrub(
            "nginx: [emerg] unknown directive in conf:2\nsecret_token abcdef123456;\n",
            content,
        );
        assert!(!out.contains("abcdef123456"));
        assert!(out.contains("unknown directive"));
        assert!(out.contains("elided"));
    }

    #[test]
    fn a_missing_file_has_no_hash_and_a_present_one_does() {
        let d = scratch("hashfile");
        assert_eq!(hash_file(&d.join("nope")).unwrap(), None);
        std::fs::write(d.join("f"), "abc").unwrap();
        assert_eq!(
            hash_file(&d.join("f")).unwrap().unwrap(),
            sha256_hex(b"abc")
        );
    }

    // R12: a validator that prints far more than a pipe holds used to block on its own output
    // while the node waited for it to exit, and the apply failed with a timeout.
    #[cfg(unix)]
    #[test]
    fn a_validator_printing_a_megabyte_neither_deadlocks_nor_fails() {
        let d = scratch("megabyte");
        let t = target(
            &d,
            Some("head -c 1048576 /dev/zero | tr '\\0' 'x'; head -c 1048576 /dev/zero | tr '\\0' 'e' >&2; exit 0"),
            None,
        );
        let started = Instant::now();
        apply(&t, b"new", &sha256_hex(b"new"), &d).unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
        // A failing one reports its (scrubbed, shortened) tail instead of hanging.
        let bad = target(
            &d,
            Some("head -c 1048576 /dev/zero | tr '\\0' 'e' >&2; exit 3"),
            None,
        );
        std::fs::write(&bad.path, "old").unwrap();
        let err = apply(&bad, b"new2", &sha256_hex(b"new2"), &d).unwrap_err();
        assert!(err.contains("exited 3"), "{err}");
        assert_eq!(std::fs::read_to_string(&bad.path).unwrap(), "old");
    }

    #[cfg(unix)]
    #[test]
    fn a_path_validator_checks_a_temporary_file_and_the_live_file_is_never_invalid() {
        let d = scratch("pathvalidate");
        let marker = d.join("saw");
        // Fails unless the file it is given contains "good"; records what path it saw.
        let validator = format!(
            "echo {{file}} > '{}'; grep -q good {{file}}",
            marker.display()
        );
        let t = target(&d, Some(&validator), None);
        std::fs::write(&t.path, "old-good").unwrap();
        let err = apply(&t, b"bad content", &sha256_hex(b"bad content"), &d).unwrap_err();
        assert!(err.contains("the file was not replaced"), "{err}");
        assert_eq!(std::fs::read_to_string(&t.path).unwrap(), "old-good");
        let seen = std::fs::read_to_string(&marker).unwrap();
        assert!(
            seen.trim() != t.path.to_string_lossy() && seen.contains(".unv-check."),
            "the validator saw the temporary file, not the target: {seen}"
        );
        let left: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".unv-check."))
            .collect();
        assert!(left.is_empty(), "the temporary file is removed");
        let (result, phases) = apply_with_phases(&t, b"good new", &sha256_hex(b"good new"), &d);
        assert_eq!(result.unwrap(), Applied::Written);
        assert_eq!(phases, vec!["written", "validated"]);
        assert_eq!(std::fs::read_to_string(&t.path).unwrap(), "good new");
    }

    #[cfg(unix)]
    #[test]
    fn phases_name_each_step_and_a_failing_check_restores_and_reloads_the_previous_file() {
        let d = scratch("check");
        let reloads = d.join("reloads");
        let mut t = target(
            &d,
            Some("exit 0"),
            Some(&format!("echo r >> '{}'", reloads.display())),
        );
        t.check = Some("exit 0".into());
        std::fs::write(&t.path, "old").unwrap();
        let (result, phases) = apply_with_phases(&t, b"new", &sha256_hex(b"new"), &d);
        assert_eq!(result.unwrap(), Applied::Written);
        assert_eq!(phases, vec!["written", "validated", "reloaded", "checked"]);

        // A failing check: previous file back, and the service is reloaded onto it.
        let mut failing = target(
            &d,
            None,
            Some(&format!("echo r >> '{}'", reloads.display())),
        );
        failing.check = Some("echo 'unhealthy' >&2; exit 1".into());
        failing.id = "t2".into();
        std::fs::write(&failing.path, "stable").unwrap();
        let before = std::fs::read_to_string(&reloads).unwrap().lines().count();
        let (result, phases) = apply_with_phases(&failing, b"broken", &sha256_hex(b"broken"), &d);
        let err = result.unwrap_err();
        assert!(
            err.contains("check failed, previous file restored and reloaded"),
            "{err}"
        );
        assert_eq!(std::fs::read_to_string(&failing.path).unwrap(), "stable");
        assert_eq!(phases, vec!["written", "reloaded"]);
        let after = std::fs::read_to_string(&reloads).unwrap().lines().count();
        assert_eq!(
            after - before,
            2,
            "once for the new file and once back onto the old"
        );
        assert!(
            !reload_signal_path(&d, "t2").exists(),
            "a failed check never signals a reload"
        );
    }

    #[test]
    fn an_http_check_wants_a_2xx_answer() {
        let ok = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = ok.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            use std::io::Write;
            for (n, stream) in ok.incoming().enumerate().take(2) {
                let mut s = stream.unwrap();
                let mut buf = [0u8; 256];
                let _ = s.read(&mut buf);
                let line = if n == 0 {
                    "HTTP/1.0 200 OK\r\n\r\n"
                } else {
                    "HTTP/1.0 503 Service Unavailable\r\n\r\n"
                };
                let _ = s.write_all(line.as_bytes());
            }
        });
        assert!(probe_http(&format!("http://{addr}/health"), Duration::from_secs(5)).is_ok());
        let err = probe_http(&format!("http://{addr}/health"), Duration::from_secs(5)).unwrap_err();
        assert!(err.contains("503"), "{err}");
        handle.join().unwrap();
        assert!(probe_http("https://example.test", Duration::from_secs(1)).is_err());
    }
}
