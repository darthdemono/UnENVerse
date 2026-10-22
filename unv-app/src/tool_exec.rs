// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Runs another program (`sops`, `gpg`, `gopass`) on the user's behalf, under rules a reviewer can
//! read in one place.
//!
//! The migration sources never re-implement age, PGP or a key service: they ask the user's own tool
//! to decrypt, which keeps the keys with the tool and the agent prompt with the user. That makes the
//! child a trust boundary, so every call goes through [`run`]:
//!
//! - **argv, never a shell.** Arguments are passed as separate strings, so a file name with a space,
//!   a quote or `;` cannot become a command.
//! - **a cleared environment.** The child sees only `PATH`, a few locale and home variables and the
//!   names the caller allow-lists (`GNUPGHOME`, `SOPS_AGE_KEY_FILE`, ...). A vault password or token
//!   in the parent's environment is not inherited.
//! - **a deadline.** A tool waiting on an agent prompt that never comes is killed, not waited on.
//! - **an output cap.** Stdout is read up to a limit; a tool that streams without end is killed.
//! - **both pipes drained at once**, so a chatty stderr cannot deadlock a tool that is still writing
//!   stdout.
//! - **scrubbed stderr.** Tools echo values in error messages. Anything that looks like a secret is
//!   replaced before the text can reach a report or a terminal.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Environment variables every child gets, because most tools break without them.
const BASE_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TERM",
    "SYSTEMROOT",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
];

/// One call to an external program.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub program: String,
    pub args: Vec<String>,
    /// Names copied from this process's environment in addition to [`BASE_ENV`].
    pub pass_env: Vec<&'static str>,
    /// Values set explicitly, whatever the parent has.
    pub set_env: Vec<(String, String)>,
    pub timeout: Duration,
    pub max_stdout: usize,
    pub stdin: Option<Vec<u8>>,
    pub cwd: Option<PathBuf>,
}

impl ToolSpec {
    pub fn new(program: impl Into<String>, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            pass_env: Vec::new(),
            set_env: Vec::new(),
            timeout: Duration::from_secs(60),
            max_stdout: 16 * 1024 * 1024,
            stdin: None,
            cwd: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn pass_env(mut self, names: &[&'static str]) -> Self {
        self.pass_env.extend_from_slice(names);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn max_stdout(mut self, bytes: usize) -> Self {
        self.max_stdout = bytes;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// The program is not installed (or not on `PATH`).
    Missing(String),
    Timeout(Duration),
    /// More output than the cap allows.
    TooLarge(usize),
    /// The tool ran and said no.
    Failed {
        code: Option<i32>,
        stderr: String,
    },
    Io(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(program) => write!(
                f,
                "`{program}` is not installed or not on PATH. Install it, or point the command at it."
            ),
            Self::Timeout(d) => write!(
                f,
                "the tool did not finish within {}s (is it waiting for a passphrase prompt?)",
                d.as_secs()
            ),
            Self::TooLarge(n) => write!(f, "the tool printed more than {n} bytes"),
            Self::Failed { code, stderr } => write!(
                f,
                "the tool failed{}{}",
                code.map(|c| format!(" (exit {c})")).unwrap_or_default(),
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {stderr}")
                }
            ),
            Self::Io(e) => write!(f, "could not run the tool: {e}"),
        }
    }
}

impl std::error::Error for ToolError {}

#[derive(Debug)]
pub struct ToolOutput {
    pub stdout: Vec<u8>,
    /// Already scrubbed with [`scrub`].
    pub stderr: String,
}

/// Replaces anything in `text` that could be a secret and shortens the result.
///
/// A run of 20 or more characters from the base64, hex and token alphabets, or a PEM block,
/// becomes `[...]`. Error text keeps its shape (the tool's message is still readable) while a value
/// the tool echoed does not survive.
pub fn scrub(text: &str) -> String {
    const MAX: usize = 600;
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.chars().count() >= 20 {
            out.push_str("[...]");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-') {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    let trimmed = out.trim();
    if trimmed.chars().count() > MAX {
        let cut: String = trimmed.chars().take(MAX).collect();
        format!("{cut}...")
    } else {
        trimmed.to_string()
    }
}

fn is_missing(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

/// Runs the tool and returns its stdout, or a [`ToolError`]. Never panics and never leaves the
/// child running.
pub fn run(spec: &ToolSpec) -> Result<ToolOutput, ToolError> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    command.env_clear();
    for name in BASE_ENV.iter().chain(spec.pass_env.iter()) {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (k, v) in &spec.set_env {
        command.env(k, v);
    }
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    command
        .stdin(if spec.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // A program that was written a moment ago can still be open for writing in another thread's
    // forked child ("text file busy"); that clears within milliseconds, so retry briefly.
    let mut attempts = 0;
    let mut child = loop {
        match command.spawn() {
            Ok(child) => break child,
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && attempts < 20 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) if is_missing(&e) => return Err(ToolError::Missing(spec.program.clone())),
            Err(e) => return Err(ToolError::Io(e.to_string())),
        }
    };

    if let (Some(bytes), Some(mut pipe)) = (spec.stdin.clone(), child.stdin.take()) {
        // A thread, so a tool that never reads its stdin cannot block the deadline below.
        std::thread::spawn(move || {
            let _ = pipe.write_all(&bytes);
        });
    }

    let (tx, rx) = mpsc::channel::<(bool, Vec<u8>, bool)>();
    let cap = spec.max_stdout;
    let spawn_reader = |mut source: Box<dyn Read + Send>, is_out: bool, limit: usize| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            let mut over = false;
            loop {
                match source.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if buf.len() + n > limit {
                            over = true;
                            break;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                    }
                }
            }
            let _ = tx.send((is_out, buf, over));
        });
    };
    if let Some(out) = child.stdout.take() {
        spawn_reader(Box::new(out), true, cap);
    }
    if let Some(err) = child.stderr.take() {
        spawn_reader(Box::new(err), false, 64 * 1024);
    }
    drop(tx);

    let deadline = Instant::now() + spec.timeout;
    let mut stdout = None;
    let mut stderr = Vec::new();
    let mut too_large = false;
    let mut readers = 0;
    let expected = 2;
    let mut exit = None;
    loop {
        while let Ok((is_out, buf, over)) = rx.try_recv() {
            readers += 1;
            if is_out {
                too_large |= over;
                stdout = Some(buf);
            } else {
                stderr = buf;
            }
        }
        if too_large {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ToolError::TooLarge(cap));
        }
        if exit.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => exit = Some(status),
                Ok(None) => {}
                Err(e) => return Err(ToolError::Io(e.to_string())),
            }
        }
        if exit.is_some() && readers >= expected {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ToolError::Timeout(spec.timeout));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let status = exit.expect("loop exits only with a status");
    let stderr = scrub(&String::from_utf8_lossy(&stderr));
    if !status.success() {
        return Err(ToolError::Failed {
            code: status.code(),
            stderr,
        });
    }
    Ok(ToolOutput {
        stdout: stdout.unwrap_or_default(),
        stderr,
    })
}

/// First line of `program --version`, scrubbed, or `None` when it cannot be run.
pub fn version(program: &str, args: &[&str]) -> Option<String> {
    let spec = ToolSpec::new(program, args).timeout(Duration::from_secs(10));
    let out = run(&spec).ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| scrub(line))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn stub(name: &str, body: &str) -> String {
        let dir = std::env::temp_dir().join(format!("unv-tool-{}", vault_core::new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn returns_stdout_and_passes_arguments_without_a_shell() {
        let tool = stub("echoargs", r#"for a in "$@"; do printf '[%s]' "$a"; done"#);
        let out = run(&ToolSpec::new(tool, &["a b", "c;d", "$HOME"])).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "[a b][c;d][$HOME]");
    }

    #[test]
    fn a_missing_program_says_to_install_it() {
        let err = run(&ToolSpec::new("unv-no-such-tool-xyz", &[])).unwrap_err();
        assert_eq!(err, ToolError::Missing("unv-no-such-tool-xyz".into()));
        assert!(err.to_string().contains("Install it"));
    }

    #[test]
    fn the_child_gets_a_cleared_environment_plus_only_what_is_allowed() {
        std::env::set_var("UNV_TEST_SECRET_TOKEN", "do-not-inherit");
        std::env::set_var("UNV_TEST_ALLOWED", "yes");
        let tool = stub(
            "env",
            r#"printf '%s|%s' "${UNV_TEST_SECRET_TOKEN:-unset}" "${UNV_TEST_ALLOWED:-unset}""#,
        );
        let out = run(&ToolSpec::new(tool, &[]).pass_env(&["UNV_TEST_ALLOWED"])).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "unset|yes");
    }

    #[test]
    fn a_failing_tool_reports_its_exit_code_and_a_scrubbed_stderr() {
        let tool = stub(
            "fail",
            r#"echo "decryption failed for key AGE-SECRET-KEY-1QQQQQQQQQQQQQQQQQQQQQQQQQQQQQ" >&2; exit 2"#,
        );
        let err = run(&ToolSpec::new(tool, &[])).unwrap_err();
        let ToolError::Failed { code, stderr } = err else {
            panic!("expected Failed");
        };
        assert_eq!(code, Some(2));
        assert!(stderr.contains("decryption failed"));
        assert!(!stderr.contains("QQQQQQQQ"), "{stderr}");
    }

    #[test]
    fn a_tool_that_never_finishes_is_killed_at_the_deadline() {
        let tool = stub("sleepy", "sleep 30");
        let started = Instant::now();
        let err = run(&ToolSpec::new(tool, &[]).timeout(Duration::from_millis(300))).unwrap_err();
        assert_eq!(err, ToolError::Timeout(Duration::from_millis(300)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn output_over_the_cap_kills_the_tool() {
        let tool = stub("endless", "yes");
        let err = run(&ToolSpec::new(tool, &[]).max_stdout(1024)).unwrap_err();
        assert_eq!(err, ToolError::TooLarge(1024));
    }

    #[test]
    fn a_chatty_stderr_does_not_deadlock_stdout() {
        // 200 KB on each pipe: with a single reader the child blocks on the full stderr pipe.
        let tool = stub(
            "chatty",
            r#"head -c 200000 /dev/zero | tr '\0' 'e' >&2; head -c 200000 /dev/zero | tr '\0' 'o'"#,
        );
        let out = run(&ToolSpec::new(tool, &[]).timeout(Duration::from_secs(20))).unwrap();
        assert_eq!(out.stdout.len(), 200_000);
    }

    #[test]
    fn stdin_reaches_the_tool_and_scrub_keeps_short_text() {
        let tool = stub("cat", "cat");
        let mut spec = ToolSpec::new(tool, &[]);
        spec.stdin = Some(b"hello".to_vec());
        assert_eq!(run(&spec).unwrap().stdout, b"hello");
        assert_eq!(scrub("not found: file.txt"), "not found: file.txt");
        assert!(scrub(&"ab ".repeat(300)).ends_with("..."));
        assert_eq!(scrub(&"x".repeat(700)), "[...]");
    }

    #[test]
    fn version_returns_the_first_line_or_none() {
        let tool = stub("ver", "echo 'sops 3.9.0 (latest)'; echo second");
        assert_eq!(
            version(&tool, &["--version"]).as_deref(),
            Some("sops 3.9.0 (latest)")
        );
        assert_eq!(version("unv-no-such-tool-xyz", &["--version"]), None);
    }
}
