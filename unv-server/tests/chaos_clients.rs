// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Several clients, one server (chaos testing, clients level): the CLI's remote client
//! against a real hub, with a second client acting between its read and its save.

use std::net::SocketAddr;
use std::path::PathBuf;
use unv_app::access::RemoteClient;
use unv_server::{build_router, AppState};

fn scratch() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "unv-chaos-clients-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn entry(id: &str, key: &str) -> serde_json::Value {
    serde_json::json!({ "id": id, "provider": format!("P-{id}"), "api_key": key })
}

async fn hub(entries: Vec<serde_json::Value>) -> (String, String) {
    let d = scratch();
    let key = [3u8; 32];
    let state = AppState::new(
        d.join("vault.db"),
        d.join("vault.salt"),
        None,
        480,
        24,
        false,
    );
    let conn = vault_core::open_db(&d.join("vault.db"), &key).unwrap();
    vault_core::init_schema(&conn).unwrap();
    vault_core::save_vault(
        &conn,
        serde_json::json!({ "api_keys": entries, "user_categories": [], "projects": [] }),
        vault_core::SaveCtx::default(),
    )
    .unwrap();
    let token = state.adopt_owner_key(key, "owner".into());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = build_router(state, addr.port());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (format!("http://{addr}"), token)
}

fn ids(v: &serde_json::Value) -> Vec<String> {
    v["api_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

/// BUG: every CLI remote save was an unconditional PUT. A `unv --server ...` command that
/// read the vault, then saved after the app or another CLI had added an entry, deleted that
/// entry: the server's per-row merge treats a save with no version as the whole truth.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_remote_cli_save_keeps_what_another_client_added_since_its_read() {
    let (url, token) = hub(vec![entry("a", "k"), entry("b", "k")]).await;
    let (u1, t1, u2, t2) = (url.clone(), token.clone(), url.clone(), token.clone());
    let (merged_ids, error) = blocking(move || {
        let a = RemoteClient::with_session(&u1, &t1).unwrap();
        let b = RemoteClient::with_session(&u2, &t2).unwrap();
        // Both read the vault.
        let mut doc_a = a.get_vault().unwrap();
        let mut doc_b = b.get_vault().unwrap();
        // B adds an entry and saves.
        doc_b["api_keys"]
            .as_array_mut()
            .unwrap()
            .push(entry("new", "n"));
        b.save_vault(&doc_b).unwrap();
        // A edits a DIFFERENT entry from its (older) copy and saves.
        doc_a["api_keys"][0]["api_key"] = serde_json::json!("edited-by-a");
        let error = a.save_vault(&doc_a).err();
        let seen = RemoteClient::with_session(&u1, &t1)
            .unwrap()
            .get_vault()
            .unwrap();
        (ids(&seen), error)
    })
    .await;
    assert!(error.is_none(), "{error:?}");
    assert!(
        merged_ids.contains(&"new".to_string()),
        "the entry another client added was deleted by a stale save: {merged_ids:?}"
    );
}

/// The same race on the SAME entry is a conflict (exit 6), not a silent overwrite.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_remote_cli_save_of_an_entry_both_changed_is_a_conflict() {
    let (url, token) = hub(vec![entry("a", "k")]).await;
    let (u1, t1) = (url.clone(), token.clone());
    let err = blocking(move || {
        let a = RemoteClient::with_session(&u1, &t1).unwrap();
        let b = RemoteClient::with_session(&u1, &t1).unwrap();
        let mut doc_a = a.get_vault().unwrap();
        let mut doc_b = b.get_vault().unwrap();
        doc_b["api_keys"][0]["api_key"] = serde_json::json!("from-b");
        b.save_vault(&doc_b).unwrap();
        doc_a["api_keys"][0]["api_key"] = serde_json::json!("from-a");
        a.save_vault(&doc_a).unwrap_err()
    })
    .await;
    assert_eq!(err.code, unv_app::error::Code::Conflict, "{err}");
}
