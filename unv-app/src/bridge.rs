// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! R18: let `unv` read allowed values from the **unlocked desktop** over a local-only socket.
//!
//! The desktop listens on a Unix domain socket (mode 0600, inside a 0700 directory) only while it
//! is unlocked *and* the user switched "Allow local scripts" on. A script's `unv` then asks the
//! desktop for a snapshot and works on it exactly as it would on the vault it opened itself.
//!
//! What is served is decided here, once: [`served_vault`] keeps only entries flagged
//! `local_scripts: true`; everything else, and every non-entry part of the document, is dropped
//! before the bytes leave the process. The bridge is read-only: [`Access::save`](crate::access)
//! refuses on a bridge, so a script cannot write through it.
//!
//! Wire format: a 4-byte big-endian length, then JSON. One request per connection. A request is
//! capped at [`MAX_REQUEST`] so a hostile writer cannot make the desktop buffer without limit.
//!
//! Threat model: any process of the same OS user can connect and read the allow-listed entries
//! (the 0600 socket stops other users, not other processes). That is why it is off by default and
//! per entry. Windows named pipes are not implemented yet; there `serve` reports so and the CLI
//! never finds a socket.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;

/// Largest request body accepted; the only request is a few bytes.
pub const MAX_REQUEST: usize = 4096;
/// Largest response the client will read (a vault of flagged entries).
pub const MAX_RESPONSE: usize = 64 * 1024 * 1024;
/// How long either side waits on the other; a locked desktop must answer well inside a second.
pub const IO_TIMEOUT_MS: u64 = 800;

/// Where the desktop listens: `UNV_BRIDGE_SOCKET`, else `$XDG_RUNTIME_DIR/unenverse/bridge.sock`,
/// else `bridge.sock` beside the vault.
pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("UNV_BRIDGE_SOCKET") {
        return PathBuf::from(p);
    }
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir).join("unenverse").join("bridge.sock");
    }
    crate::access::default_db_path()
        .parent()
        .map(|p| p.join("bridge.sock"))
        .unwrap_or_else(|| PathBuf::from("bridge.sock"))
}

/// The document a script may see: entries with `local_scripts: true`, nothing else.
pub fn served_vault(vault: &Value) -> Value {
    let kept: Vec<Value> = crate::data::entries(vault)
        .into_iter()
        .filter(|e| e.get("local_scripts").and_then(Value::as_bool) == Some(true))
        .collect();
    json!({ "api_keys": kept })
}

/// Read one length-prefixed frame of at most `max` bytes.
pub fn read_frame(r: &mut impl Read, max: usize) -> Result<Vec<u8>, String> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).map_err(|e| e.to_string())?;
    let n = u32::from_be_bytes(len) as usize;
    if n > max {
        return Err(format!("frame of {n} bytes exceeds the {max} byte limit"));
    }
    let mut body = vec![0u8; n];
    r.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok(body)
}

pub fn write_frame(w: &mut impl Write, body: &[u8]) -> Result<(), String> {
    let len = u32::try_from(body.len()).map_err(|_| "frame too large".to_string())?;
    w.write_all(&len.to_be_bytes()).map_err(|e| e.to_string())?;
    w.write_all(body).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())
}

/// Turn one request frame into the response JSON. Pure, so the parser can be fuzzed directly.
pub fn handle_request(body: &[u8], provider: &dyn Fn() -> Result<Value, String>) -> Value {
    let req: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return json!({ "ok": false, "code": "invalid", "error": "request is not JSON" }),
    };
    match req.get("op").and_then(Value::as_str) {
        Some("ping") => json!({ "ok": true }),
        Some("snapshot") => match provider() {
            Ok(vault) => json!({ "ok": true, "vault": served_vault(&vault) }),
            Err(e) => json!({ "ok": false, "code": "locked", "error": e }),
        },
        _ => json!({ "ok": false, "code": "invalid", "error": "unknown op" }),
    }
}

#[cfg(unix)]
pub use unix::{fetch_snapshot, serve, BridgeServer, Seen};

#[cfg(not(unix))]
pub fn fetch_snapshot() -> Result<Option<Value>, crate::error::CliError> {
    Ok(None)
}

/// Windows named pipes are not implemented yet; the type exists so callers compile.
#[cfg(not(unix))]
pub struct BridgeServer;

#[cfg(not(unix))]
pub fn serve(
    _path: PathBuf,
    _provider: std::sync::Arc<dyn Fn() -> Result<Value, String> + Send + Sync>,
) -> Result<BridgeServer, String> {
    Err("Local scripts are not available on Windows yet".into())
}

#[cfg(unix)]
mod unix {
    use super::*;
    use crate::error::CliError;
    use std::collections::VecDeque;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const MAX_CONNECTIONS: usize = 8;
    /// How many recent requesters the server remembers.
    const SEEN_KEPT: usize = 50;

    /// One connection to the bridge: when, and which process asked (R18). The socket is open to
    /// every process of the same user, so this is how the person can see who actually used it.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Seen {
        pub at: String,
        /// `None` where the platform gives no peer process id.
        pub pid: Option<i32>,
    }

    /// The process on the other end of a connected socket. Linux only (`SO_PEERCRED`).
    #[cfg(target_os = "linux")]
    fn peer_pid(s: &UnixStream) -> Option<i32> {
        use std::os::fd::AsRawFd;
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: `cred` and `len` are live locals of exactly the size passed, and the fd is a
        // connected socket owned by `s` for the whole call.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage (one getsockopt call; both pointers are live locals of the size passed)
        let rc = unsafe {
            libc::getsockopt(
                s.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                std::ptr::addr_of_mut!(cred).cast(),
                &mut len,
            )
        };
        (rc == 0).then_some(cred.pid)
    }

    #[cfg(not(target_os = "linux"))]
    fn peer_pid(_s: &UnixStream) -> Option<i32> {
        None
    }

    /// A running listener. Dropping it stops the thread and removes the socket, which is how a
    /// lock or a quit takes the bridge down.
    pub struct BridgeServer {
        stop: Arc<AtomicBool>,
        path: PathBuf,
        thread: Option<std::thread::JoinHandle<()>>,
        seen: Arc<Mutex<VecDeque<Seen>>>,
    }

    impl BridgeServer {
        /// The most recent connections, oldest first, at most [`SEEN_KEPT`].
        pub fn recent(&self) -> Vec<Seen> {
            self.seen
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .cloned()
                .collect()
        }
    }

    impl Drop for BridgeServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
            let _ = std::fs::remove_file(&self.path);
        }
    }

    /// Start listening at `path`. A socket left by a crashed desktop (nobody answers on it) is
    /// removed first; one a live desktop answers on is an error, not something to steal.
    pub fn serve(
        path: PathBuf,
        provider: Arc<dyn Fn() -> Result<Value, String> + Send + Sync>,
    ) -> Result<BridgeServer, String> {
        if let Some(dir) = path.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .map_err(|e| e.to_string())?;
        }
        if path.exists() {
            if UnixStream::connect(&path).is_ok() {
                return Err("another UnENVerse already serves local scripts".into());
            }
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let live = Arc::new(AtomicUsize::new(0));
        let flag = Arc::clone(&stop);
        let seen: Arc<Mutex<VecDeque<Seen>>> = Arc::default();
        let log = Arc::clone(&seen);
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if live.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                            continue; // dropped: the client sees EOF and exits 7
                        }
                        live.fetch_add(1, Ordering::SeqCst);
                        {
                            let mut q = log.lock().unwrap_or_else(|e| e.into_inner());
                            if q.len() == SEEN_KEPT {
                                q.pop_front();
                            }
                            q.push_back(Seen {
                                at: vault_core::iso_now(),
                                pid: peer_pid(&stream),
                            });
                        }
                        let provider = Arc::clone(&provider);
                        let live = Arc::clone(&live);
                        std::thread::spawn(move || {
                            let _ = answer(stream, provider.as_ref());
                            live.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        });
        Ok(BridgeServer {
            stop,
            path,
            thread: Some(thread),
            seen,
        })
    }

    fn answer(
        mut s: UnixStream,
        provider: &dyn Fn() -> Result<Value, String>,
    ) -> Result<(), String> {
        s.set_nonblocking(false).map_err(|e| e.to_string())?;
        let t = Some(Duration::from_millis(IO_TIMEOUT_MS));
        s.set_read_timeout(t).map_err(|e| e.to_string())?;
        s.set_write_timeout(t).map_err(|e| e.to_string())?;
        let body = read_frame(&mut s, MAX_REQUEST)?;
        let resp = handle_request(&body, provider);
        write_frame(&mut s, resp.to_string().as_bytes())
    }

    /// Ask the desktop for the served vault. `Ok(None)`: no socket, so use the normal path.
    /// An error means the socket exists but the desktop is locked or gone (exit 7, fast).
    pub fn fetch_snapshot() -> Result<Option<Value>, CliError> {
        let path = socket_path();
        if !path.exists() {
            return Ok(None);
        }
        let mut s = match UnixStream::connect(&path) {
            Ok(s) => s,
            // A stale socket: nobody is listening, the desktop is closed.
            Err(_) => {
                return Err(CliError::unavailable(
                    "The UnENVerse desktop is not serving local scripts (closed or locked). \
                     Unlock it and switch on Allow local scripts, or pass --no-bridge.",
                ))
            }
        };
        let t = Some(Duration::from_millis(IO_TIMEOUT_MS));
        let io = |e: String| CliError::unavailable(format!("The desktop bridge failed: {e}"));
        s.set_read_timeout(t).map_err(|e| io(e.to_string()))?;
        s.set_write_timeout(t).map_err(|e| io(e.to_string()))?;
        write_frame(&mut s, br#"{"op":"snapshot"}"#).map_err(io)?;
        let body = read_frame(&mut s, MAX_RESPONSE).map_err(io)?;
        let resp: Value =
            serde_json::from_slice(&body).map_err(|e| io(format!("bad response: {e}")))?;
        if resp.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(resp.get("vault").cloned());
        }
        let msg = resp
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the desktop refused");
        Err(CliError::unavailable(format!(
            "The desktop is locked: {msg}"
        )))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // UNV_BRIDGE_SOCKET is process-wide: the socket tests take turns.
    static ENV: Mutex<()> = Mutex::new(());

    fn vault() -> Value {
        json!({ "api_keys": [
            { "id": "1", "provider": "Shared", "api_key": "shared-secret-value", "local_scripts": true },
            { "id": "2", "provider": "Private", "api_key": "private-secret-value" },
            { "id": "3", "provider": "Off", "api_key": "off-secret-value", "local_scripts": false }
        ], "projects": [{ "name": "p" }] })
    }

    #[test]
    fn only_flagged_entries_leave_the_process() {
        let served = served_vault(&vault()).to_string();
        assert!(served.contains("shared-secret-value"));
        assert!(!served.contains("private-secret-value"), "{}", served.len());
        assert!(!served.contains("off-secret-value"));
        assert!(!served.contains("projects"));
    }

    #[test]
    fn requests_are_parsed_defensively() {
        let ok = |_: &[u8]| handle_request(br#"{"op":"snapshot"}"#, &|| Ok(vault()));
        assert_eq!(ok(b"")["ok"], true);
        for bad in [
            &b""[..],
            b"{",
            b"[]",
            b"null",
            b"{\"op\":5}",
            b"{\"op\":\"dump\"}",
            &[0xff, 0xfe],
        ] {
            let r = handle_request(bad, &|| Ok(vault()));
            assert_eq!(r["ok"], false);
            assert_eq!(r["code"], "invalid");
        }
        let locked = handle_request(br#"{"op":"snapshot"}"#, &|| Err("Vault is locked".into()));
        assert_eq!(locked["code"], "locked");
    }

    #[test]
    fn frames_over_the_limit_are_refused_without_reading_the_body() {
        let mut huge = (MAX_REQUEST as u32 + 1).to_be_bytes().to_vec();
        huge.extend_from_slice(b"x");
        assert!(read_frame(&mut huge.as_slice(), MAX_REQUEST).is_err());
        // A short body behind an honest length is an error, never a hang or a partial frame.
        let mut short = 10u32.to_be_bytes().to_vec();
        short.extend_from_slice(b"abc");
        assert!(read_frame(&mut short.as_slice(), MAX_REQUEST).is_err());
    }

    /// Hand-rolled mutation fuzz: no byte string may panic the handler.
    #[test]
    fn mutated_requests_never_panic() {
        let seed = br#"{"op":"snapshot","x":[1,2,{"a":null}]}"#;
        let mut state = 0x2545F491u32;
        for _ in 0..4000 {
            let mut b = seed.to_vec();
            for _ in 0..4 {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let i = (state >> 8) as usize % b.len();
                b[i] = (state >> 24) as u8;
            }
            let _ = handle_request(&b, &|| Ok(vault()));
        }
    }

    #[test]
    fn socket_is_private_serves_flagged_entries_and_is_removed_on_drop() {
        use std::os::unix::fs::PermissionsExt;
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("unv-bridge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("b.sock");
        let server = serve(path.clone(), Arc::new(|| Ok(vault()))).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let dmode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(dmode, 0o700);

        std::env::set_var("UNV_BRIDGE_SOCKET", &path);
        let got = fetch_snapshot().unwrap().unwrap().to_string();
        assert!(got.contains("shared-secret-value") && !got.contains("private-secret-value"));

        // A second desktop must not steal a live socket.
        assert!(serve(path.clone(), Arc::new(|| Ok(vault()))).is_err());

        drop(server);
        assert!(!path.exists(), "lock or quit unlinks the socket");
        // Missing socket: the normal path applies.
        assert!(fetch_snapshot().unwrap().is_none());
        std::env::remove_var("UNV_BRIDGE_SOCKET");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_server_remembers_which_process_asked() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("unv-bridge-seen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("b.sock");
        let server = serve(path.clone(), Arc::new(|| Ok(vault()))).unwrap();
        assert!(server.recent().is_empty());
        std::env::set_var("UNV_BRIDGE_SOCKET", &path);
        fetch_snapshot().unwrap().unwrap();
        std::env::remove_var("UNV_BRIDGE_SOCKET");
        // The accept loop records the connection before it hands it on, so it is there by the
        // time the reply has been read.
        let seen = server.recent();
        assert_eq!(seen.len(), 1);
        assert!(!seen[0].at.is_empty());
        #[cfg(target_os = "linux")]
        assert_eq!(
            seen[0].pid,
            Some(i32::try_from(std::process::id()).unwrap())
        );
        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_socket_is_cleaned_and_a_locked_desktop_answers_fast() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("unv-bridge-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("b.sock");
        // A crashed desktop leaves a socket file nobody listens on.
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists());

        std::env::set_var("UNV_BRIDGE_SOCKET", &path);
        let t = std::time::Instant::now();
        let err = fetch_snapshot().unwrap_err();
        assert_eq!(err.code.as_str(), "unavailable");
        assert!(t.elapsed().as_millis() < 1000);

        let server = serve(path.clone(), Arc::new(|| Err("Vault is locked".into()))).unwrap();
        let t = std::time::Instant::now();
        let err = fetch_snapshot().unwrap_err();
        assert_eq!(err.code.as_str(), "unavailable", "locked is unavailable");
        assert!(t.elapsed().as_millis() < 1000);
        drop(server);
        std::env::remove_var("UNV_BRIDGE_SOCKET");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
