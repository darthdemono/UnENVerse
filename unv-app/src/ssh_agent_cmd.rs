// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv ssh-agent` (R32): serve the Ed25519 keys in the vault over the OpenSSH agent protocol, so
//! `ssh` can sign with a key that never sits under `~/.ssh`.
//!
//! The protocol and the signing are in `vault_core::ssh_agent`. This is the socket around it: a
//! Unix socket (mode 0600 inside a 0700 directory, so only the owner can connect), one thread per
//! client, and an audit row for every signature naming the key and the calling process. The agent
//! signs; it never returns a private key and cannot be told to add or remove one. Keys come from
//! entries whose secret is an OpenSSH private key; an encrypted key or another algorithm is
//! skipped and reported. Windows named pipes are not implemented.

use crate::access::Access;
use crate::error::{CliError, CliResult};
use serde_json::Value;
use vault_core::ssh_agent::AgentKey;

/// The keys found in a vault, and the entries that could not be used with the reason.
pub struct Loaded {
    pub keys: Vec<AgentKey>,
    pub skipped: Vec<(String, String)>,
}

/// Every entry whose secret is an OpenSSH private key becomes an agent key. Names only are kept for
/// the entries that fail; the reason never includes key material.
pub fn load_keys(vault: &Value) -> Loaded {
    let mut keys = Vec::new();
    let mut skipped = Vec::new();
    for e in crate::data::entries(vault) {
        let pem = e.get("api_key").and_then(Value::as_str).unwrap_or("");
        if !pem.contains("BEGIN OPENSSH PRIVATE KEY") {
            continue;
        }
        let name = crate::data::provider_of(&e).to_string();
        match AgentKey::from_pem(pem, &name) {
            Ok(k) => keys.push(k),
            Err(why) => skipped.push((name, why)),
        }
    }
    Loaded { keys, skipped }
}

/// Where the agent listens: `$XDG_RUNTIME_DIR/unenverse/ssh-agent.sock`, else beside the vault.
pub fn default_socket() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return std::path::PathBuf::from(dir)
            .join("unenverse")
            .join("ssh-agent.sock");
    }
    crate::access::default_db_path()
        .parent()
        .map(|p| p.join("ssh-agent.sock"))
        .unwrap_or_else(|| std::path::PathBuf::from("ssh-agent.sock"))
}

#[cfg(not(unix))]
pub fn run(_access: &Access, _socket: Option<&std::path::Path>, _export_env: bool) -> CliResult {
    Err(CliError::invalid(
        "the SSH agent needs Unix sockets; Windows named pipes are not implemented yet",
    ))
}

#[cfg(unix)]
pub fn run(access: &Access, socket: Option<&std::path::Path>, export_env: bool) -> CliResult {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let Access::Local(key) = access else {
        return Err(CliError::invalid(
            "the SSH agent reads keys from the local vault; it cannot run in remote mode",
        ));
    };
    let key = *key;
    let loaded = load_keys(&access.load_vault()?);
    for (name, why) in &loaded.skipped {
        eprintln!("skipped '{name}': {why}");
    }
    if loaded.keys.is_empty() {
        return Err(CliError::not_found(
            "No usable key: the vault holds no unencrypted Ed25519 OpenSSH private key",
        ));
    }
    let keys = Arc::new(loaded.keys);

    let path = socket
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(default_socket);
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| CliError::from(e.to_string()))?;
    }
    if path.exists() {
        if UnixStream::connect(&path).is_ok() {
            return Err(CliError::new(
                crate::error::Code::Conflict,
                format!("an agent is already listening on {}", path.display()),
            ));
        }
        std::fs::remove_file(&path).map_err(|e| CliError::from(e.to_string()))?;
    }
    let listener = UnixListener::bind(&path).map_err(|e| CliError::from(e.to_string()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| CliError::from(e.to_string()))?;

    if export_env {
        println!("SSH_AUTH_SOCK={}; export SSH_AUTH_SOCK;", path.display());
    }
    eprintln!(
        "Serving {} key(s) on {} (Ctrl-C to stop)",
        keys.len(),
        path.display()
    );

    let db = crate::access::default_db_path();
    let live = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if live.load(Ordering::SeqCst) >= 16 {
            continue; // dropped: the client sees EOF
        }
        live.fetch_add(1, Ordering::SeqCst);
        let keys = Arc::clone(&keys);
        let live = Arc::clone(&live);
        let db = db.clone();
        let pid = crate::bridge::peer_pid(&stream);
        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(120)));
            let _ = vault_core::ssh_agent::serve(&mut stream, &keys, |comment| {
                // A signature is on the record with the key and the process that asked.
                if let Ok(conn) = vault_core::open_db(&db, &key) {
                    let who =
                        pid.map_or_else(|| "unknown process".to_string(), |p| format!("pid {p}"));
                    let _ = vault_core::record_event(&conn, "ssh.sign", comment, Some(&who), None);
                }
            });
            live.fetch_sub(1, Ordering::SeqCst);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pem(comment: &str) -> String {
        let made =
            vault_core::generate_ssh_keypair(comment, &vault_core::entropy::Source::Os).unwrap();
        made["private_key"].as_str().unwrap().to_string()
    }

    #[test]
    fn only_entries_holding_an_openssh_key_become_agent_keys_and_failures_are_named_not_leaked() {
        let vault = json!({ "api_keys": [
            { "id": "1", "provider": "Deploy key", "api_key": pem("deploy") },
            { "id": "2", "provider": "GitHub", "api_key": "ghp_not_a_key" },
            { "id": "3", "provider": "Broken", "api_key": "-----BEGIN OPENSSH PRIVATE KEY-----\nbm90IGEga2V5\n-----END OPENSSH PRIVATE KEY-----" }
        ]});
        let l = load_keys(&vault);
        assert_eq!(l.keys.len(), 1);
        assert_eq!(l.keys[0].comment, "deploy");
        assert_eq!(l.skipped.len(), 1);
        assert_eq!(l.skipped[0].0, "Broken");
        assert!(!l.skipped[0].1.contains("bm90"), "{}", l.skipped[0].1);
    }

    #[test]
    fn an_empty_vault_has_no_keys() {
        let l = load_keys(&json!({ "api_keys": [] }));
        assert!(l.keys.is_empty() && l.skipped.is_empty());
    }

    #[test]
    fn the_default_socket_is_named_for_the_agent() {
        assert!(default_socket().ends_with("ssh-agent.sock"));
    }
}
