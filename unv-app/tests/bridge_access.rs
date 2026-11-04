// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! R18 end to end on the library side: a served desktop, `open_access`, and what a script gets.
#![cfg(unix)]

use serde_json::json;
use std::sync::Arc;
use unv_app::access::{open_access, Access, AuthOpts, Bridge};
use unv_app::bridge::serve;

fn opts(bridge: Bridge) -> AuthOpts<'static> {
    AuthOpts {
        server: None,
        password: None,
        user: None,
        token: None,
        session_token: None,
        totp: None,
        init: false,
        bridge,
    }
}

#[test]
fn a_script_reads_flagged_entries_only_and_cannot_write_or_wait() {
    let dir = std::env::temp_dir().join(format!("unv-bridge-access-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("b.sock");
    std::env::set_var("UNV_BRIDGE_SOCKET", &path);

    // Locked desktop: the socket exists but every snapshot is refused. Exit 7, inside a second.
    let locked = serve(path.clone(), Arc::new(|| Err("Vault is locked".into()))).unwrap();
    let t = std::time::Instant::now();
    let err = open_access(&opts(Bridge::Use))
        .err()
        .expect("locked is an error");
    assert_eq!(err.code.as_str(), "unavailable");
    assert!(t.elapsed().as_millis() < 1000, "locked answers fast");
    drop(locked);

    let vault = json!({ "api_keys": [
        { "id": "a", "provider": "Shared", "api_key": "shared-value-1234", "local_scripts": true },
        { "id": "b", "provider": "Private", "api_key": "private-value-5678" }
    ]});
    let server = serve(path.clone(), Arc::new(move || Ok(vault.clone()))).unwrap();
    let access = open_access(&opts(Bridge::Use)).unwrap();
    assert!(matches!(access, Access::Bridge(_)));
    let seen = access.load_vault().unwrap().to_string();
    assert!(seen.contains("shared-value-1234"));
    assert!(
        !seen.contains("private-value-5678"),
        "an unflagged entry is never served"
    );

    // Read-only: the single write point refuses on a bridge.
    let denied = access.save(&json!({ "api_keys": [] })).unwrap_err();
    assert_eq!(denied.code.as_str(), "denied");

    // Opting out never touches the socket.
    drop(server);
    assert!(!path.exists());
    std::env::remove_var("UNV_BRIDGE_SOCKET");
    let _ = std::fs::remove_dir_all(&dir);
}
