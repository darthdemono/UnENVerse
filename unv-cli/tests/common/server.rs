// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! A real `unv-server` for the tests that need one, started in-process on an
//! ephemeral loopback port over a vault the test already created with the CLI.
//!
//! Include with `#[path = "common/server.rs"] mod server;` (next to
//! `mod common;`) so test files that never need a server do not link one.
#![allow(dead_code)]

use crate::common::Vault;
use std::net::SocketAddr;

pub struct Server {
    pub url: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    stopped: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

/// Stop the server and wait for its runtime to wind down. A server thread still
/// running when the test binary exits races the process's exit handlers (the
/// SQLCipher and TLS libraries tear down static state), which once crashed a
/// finished binary with SIGSEGV after every test had passed.
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Ok(rx) = self.stopped.lock() {
            let _ = rx.recv_timeout(std::time::Duration::from_secs(10));
        }
    }
}

impl Server {
    /// The vault must already exist (any CLI write creates it). The server
    /// starts locked: the owner unlocks it by presenting the master password,
    /// as `unv --server` does, so the whole login path is what is under test.
    pub fn start(vault: &Vault, nodes: bool) -> Server {
        Self::start_with(vault, nodes, false)
    }

    /// Like [`Server::start`], optionally with the unique-ID registry enabled.
    pub fn start_with(vault: &Vault, nodes: bool, uid_registry: bool) -> Server {
        let db = vault.db();
        let salt = vault.dir.join("vault.salt");
        assert!(
            db.exists() && salt.exists(),
            "create the vault with the CLI first"
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let (done_tx, stopped) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let mut state = unv_server::AppState::new(db, salt, None, 480, 24, false);
                if nodes {
                    state = state.with_nodes(true, None);
                }
                if uid_registry {
                    state = state.with_uid_registry(true, None);
                }
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                let app = unv_server::build_router(state, addr.port());
                tx.send(addr).unwrap();
                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .with_graceful_shutdown(async {
                    let _ = stop_rx.await;
                })
                .await
                .unwrap();
            });
            // Drop the runtime first: it joins the workers and any blocking
            // (Argon2) thread. Signalling before that let the process exit while
            // they were still tearing down, which is the race `Drop` guards.
            drop(rt);
            let _ = done_tx.send(());
        });
        let addr = rx.recv().unwrap();
        Server {
            url: format!("http://{addr}"),
            stop: Some(stop),
            stopped: std::sync::Mutex::new(stopped),
        }
    }
}

/// One plain HTTP GET: `(status, headers lower-cased as "name: value" lines, body)`.
/// Enough to fetch a calendar feed without pulling in an HTTP client.
pub fn http_get(url: &str) -> (u16, String, String) {
    use std::io::{Read, Write};
    let rest = url.strip_prefix("http://").expect("http url");
    let (host, path) = rest
        .split_once('/')
        .map(|(h, p)| (h, format!("/{p}")))
        .unwrap_or((rest, "/".into()));
    let mut stream = std::net::TcpStream::connect(host).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let headers = head
        .lines()
        .skip(1)
        .map(|l| l.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    (status, headers, body.to_string())
}

impl Vault {
    /// Run a command as the vault owner against `server`.
    pub fn on(&self, server: &Server, args: &[&str]) -> crate::common::Out {
        self.run_with(&["--server", &server.url], args)
    }

    /// Same, with the human (non-JSON) output.
    pub fn on_text(&self, server: &Server, args: &[&str]) -> crate::common::Out {
        let mut all = vec!["--server", server.url.as_str()];
        all.extend_from_slice(args);
        self.run_text(&all)
    }

    /// Run as a named user with that user's password.
    pub fn as_user(
        &self,
        server: &Server,
        user: &str,
        password: &str,
        args: &[&str],
    ) -> crate::common::Out {
        let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
        for v in [
            "UNV_SERVER_URL",
            "UNV_ENV_FILE",
            "UNV_PROJECT",
            "UNV_TOKEN",
            "UNV_TOTP",
        ] {
            c.env_remove(v);
        }
        c.env("UNV_PASSWORD", password)
            .env("HOME", self.dir.join("home"))
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .stdin(std::process::Stdio::null())
            .args(["--server", &server.url, "--user", user, "--yes", "--json"])
            .args(args);
        let o = c.output().unwrap();
        crate::common::Out {
            code: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    /// Run with an API token.
    pub fn with_token(&self, server: &Server, token: &str, args: &[&str]) -> crate::common::Out {
        let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
        for v in [
            "UNV_SERVER_URL",
            "UNV_ENV_FILE",
            "UNV_PROJECT",
            "UNV_USER",
            "UNV_TOTP",
            "UNV_PASSWORD",
        ] {
            c.env_remove(v);
        }
        c.env("HOME", self.dir.join("home"))
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .stdin(std::process::Stdio::null())
            .args(["--server", &server.url, "--token", token, "--yes", "--json"])
            .args(args);
        let o = c.output().unwrap();
        crate::common::Out {
            code: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }
}
