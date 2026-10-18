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
        c
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
