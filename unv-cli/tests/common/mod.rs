// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Shared harness for the behaviour tests that run the real `unv` binary.
//!
//! Every run is isolated: its own vault under a unique temp directory, its own
//! HOME and XDG directories (so sessions, pools and pins never touch the
//! developer's real ones), and the variables that would send a command to a
//! real server or unlock with a real password are removed first.
#![allow(dead_code)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

pub const PASSWORD: &str = "contract-test-password-12345";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    /// Both streams, for "this must appear nowhere" assertions.
    pub fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// The `--json` envelope printed on stdout.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {}", self.stdout))
    }

    /// The `error.code` of a failed envelope. Failures print their JSON on stdout.
    pub fn error_code(&self) -> String {
        self.json()["error"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }
}

pub struct Vault {
    pub dir: PathBuf,
}

impl Vault {
    pub fn new(tag: &str) -> Vault {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("unv-contract-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("home")).unwrap();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::create_dir_all(dir.join("config")).unwrap();
        std::fs::create_dir_all(dir.join("state")).unwrap();
        Vault { dir }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn db(&self) -> PathBuf {
        self.dir.join("vault.db")
    }

    fn base(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_unv"));
        self.environment(&mut c);
        c
    }

    /// A shell script run in the same clean environment as `unv` itself, with `$UNV` naming the
    /// binary and `$VAULT` the database: a recipe that pipes or redirects.
    #[cfg(unix)]
    pub fn sh(&self, script: &str) -> Out {
        let mut c = Command::new("sh");
        self.environment(&mut c);
        c.args(["-c", script])
            .env("UNV", env!("CARGO_BIN_EXE_unv"))
            .env("VAULT", self.db());
        Self::collect(c, None)
    }

    fn environment(&self, c: &mut Command) {
        for v in [
            "UNV_SERVER_URL",
            "UNV_ENV_FILE",
            "UNV_PROJECT",
            "UNV_TOKEN",
            "UNV_USER",
            "UNV_TOTP",
            "UNV_PASSWORD_FILE",
            "UNV_PASSWORD_COMMAND",
            "UNV_ENV_CASE",
            "UNV_ENV_PREFIX",
            "UNV_SESSION_FILE",
            "UNV_POOL_FILE",
        ] {
            c.env_remove(v);
        }
        c.env("UNV_PASSWORD", PASSWORD)
            .env("HOME", self.dir.join("home"))
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            // Sessions and pool cursors live under the state directory; a developer
            // with this variable set must not have tests touch the real ones.
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .stdin(Stdio::null());
        pin_state_files(c, &self.dir);
    }

    fn collect(mut c: Command, stdin: Option<&str>) -> Out {
        if let Some(text) = stdin {
            use std::io::Write;
            c.stdin(Stdio::piped());
            let mut child = c
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
            let o = child.wait_with_output().unwrap();
            return Out {
                code: o.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            };
        }
        let o = c.output().unwrap();
        Out {
            code: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    /// `unv --db-path <vault> --init --yes --json <args>`: the machine-readable
    /// path an orchestrator uses. `--init` is harmless once the vault exists.
    pub fn run(&self, args: &[&str]) -> Out {
        let mut c = self.base();
        c.arg("--db-path")
            .arg(self.db())
            .args(["--init", "--yes", "--json"])
            .args(args);
        Self::collect(c, None)
    }

    /// Same, with the human (non-JSON) output.
    pub fn run_text(&self, args: &[&str]) -> Out {
        let mut c = self.base();
        c.arg("--db-path")
            .arg(self.db())
            .args(["--init", "--yes"])
            .args(args);
        Self::collect(c, None)
    }

    /// Without `--yes`, and with no terminal: what a script that forgot `--yes` does.
    pub fn run_no_yes(&self, args: &[&str]) -> Out {
        let mut c = self.base();
        c.arg("--db-path")
            .arg(self.db())
            .args(["--init", "--json"])
            .args(args);
        Self::collect(c, None)
    }

    /// Anything before the subcommand, e.g. `--reveal` or `--dry-run`, then the rest.
    pub fn run_with(&self, global: &[&str], args: &[&str]) -> Out {
        let mut c = self.base();
        c.arg("--db-path")
            .arg(self.db())
            .args(["--init", "--yes", "--json"])
            .args(global)
            .args(args);
        Self::collect(c, None)
    }

    /// Same as [`Vault::run`], with extra environment variables for the process.
    pub fn run_env(&self, env: &[(&str, &str)], args: &[&str]) -> Out {
        let mut c = self.base();
        c.arg("--db-path")
            .arg(self.db())
            .args(["--init", "--yes", "--json"])
            .args(args);
        for (k, v) in env {
            c.env(k, v);
        }
        Self::collect(c, None)
    }

    pub fn run_stdin(&self, args: &[&str], stdin: &str) -> Out {
        let mut c = self.base();
        c.arg("--db-path")
            .arg(self.db())
            .args(["--init", "--yes", "--json"])
            .args(args);
        Self::collect(c, Some(stdin))
    }

    /// Run and require success, returning the parsed `data`.
    pub fn ok(&self, args: &[&str]) -> Value {
        let o = self.run(args);
        assert_eq!(o.code, 0, "{args:?} failed:\n{}", o.all());
        let j = o.json();
        assert_eq!(j["ok"], true, "{args:?}: {}", o.stdout);
        j["data"].clone()
    }

    /// The raw (revealed) value of a field, read the way a human at a terminal would.
    pub fn reveal_field(&self, provider: &str, field: &str) -> String {
        let o = self.run_with(&["--reveal"], &["get", provider, "--field", field]);
        assert_eq!(o.code, 0, "{}", o.all());
        o.json()["data"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(unix)]
pub fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// Windows resolves the state directory from the profile (`%LOCALAPPDATA%`) and
/// ignores `XDG_STATE_HOME`, so every test would share the runner's real session
/// and pool files. Name them explicitly, at the path the Unix default produces.
pub fn pin_state_files(c: &mut Command, dir: &Path) {
    if cfg!(windows) {
        let state = dir.join("state").join("unv");
        c.env("UNV_SESSION_FILE", state.join("sessions.json"))
            .env("UNV_POOL_FILE", state.join("pools.json"));
    }
}

/// A small HTTP/1.1 server for tests of commands that talk to a service: each request is answered
/// by a closure that sees `(target, header lookup, body)`.
pub mod http {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    pub struct Req {
        pub method: String,
        pub target: String,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Req {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        }
    }

    /// Starts the server on a free local port and returns its base URL.
    pub fn serve(handler: impl Fn(&Req) -> (u16, String) + Send + Sync + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let end = loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break None;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(i);
                    }
                };
                let Some(end) = end else { continue };
                let head = String::from_utf8_lossy(&buf[..end]).into_owned();
                let mut lines = head.lines();
                let request_line = lines.next().unwrap_or("");
                let method = request_line.split(' ').next().unwrap_or("").to_string();
                let target = request_line.split(' ').nth(1).unwrap_or("").to_string();
                let headers: Vec<(String, String)> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .collect();
                let length: usize = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse().ok())
                    .unwrap_or(0);
                let mut body = buf[end + 4..].to_vec();
                while body.len() < length {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&chunk[..n]);
                }
                let (status, text) = handler(&Req {
                    method,
                    target,
                    headers,
                    body: String::from_utf8_lossy(&body).into_owned(),
                });
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                        text.len()
                    )
                    .as_bytes(),
                );
            }
        });
        base
    }
}
