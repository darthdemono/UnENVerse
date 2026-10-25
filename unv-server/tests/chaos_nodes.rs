// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Several actors, one hub (chaos testing, nodes level). A real `Agent` from `unv-cli`
//! against a real hub over a real TCP socket, with the owner acting at the worst moment.
//!
//! Each test names the bug it was written for; revert the fix and it must fail.

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::Request;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tower::ServiceExt;
use unv_app::node_agent::{self, Agent};
use unv_server::{build_router, AppState};

fn scratch(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "unv-chaos-nodes-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn vault(value: &str) -> serde_json::Value {
    serde_json::json!({
        "api_keys": [], "user_categories": [],
        "projects": [{
            "id": "edge", "name": "edge", "project_type": "generic",
            "chunks": [{
                "id": "c1", "name": "app.env", "chunk_type": "env_file",
                "fields": [{ "key": "TOKEN", "value": value }]
            }]
        }]
    })
}

struct Hub {
    state: AppState,
    owner: String,
    url: String,
}

async fn start() -> Hub {
    let d = scratch("hub");
    let key = [9u8; 32];
    let state = AppState::new(
        d.join("vault.db"),
        d.join("vault.salt"),
        None,
        480,
        24,
        false,
    )
    .with_nodes(true, None);
    let conn = vault_core::open_db(&d.join("vault.db"), &key).unwrap();
    vault_core::init_schema(&conn).unwrap();
    vault_core::save_vault(&conn, vault("first"), vault_core::SaveCtx::default()).unwrap();
    let owner = state.adopt_owner_key(key, "owner".into());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = build_router(state.clone(), addr.port());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    Hub {
        state,
        owner,
        url: format!("http://{addr}"),
    }
}

impl Hub {
    async fn owner(
        &self,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> (u16, serde_json::Value) {
        let app = build_router(self.state.clone(), 8743).layer(MockConnectInfo(
            "127.0.0.1:1".parse::<SocketAddr>().unwrap(),
        ));
        let r = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {}", self.owner))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let st = r.status().as_u16();
        let b = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (
            st,
            serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null),
        )
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

async fn enroll(h: &Hub, dir: &Path, name: &str) {
    let (st, tok) = h
        .owner(
            "POST",
            "/api/nodes/tokens",
            serde_json::json!({ "name": name, "projects": ["edge"] }),
        )
        .await;
    assert_eq!(st, 200, "{tok}");
    let (dir, url, token) = (
        dir.to_path_buf(),
        h.url.clone(),
        tok["token"].as_str().unwrap().to_string(),
    );
    blocking(move || node_agent::enroll(&dir, &url, &token, None, false, None).map(|_| ()))
        .await
        .unwrap();
}

fn config(dir: &Path, out: &Path) -> PathBuf {
    let p = dir.join("node.toml");
    std::fs::write(
        &p,
        format!(
            "[[target]]\nid=\"env-main\"\npath='{}'\nproject=\"edge\"\nexporter=\"env\"\napply=true\nvalidate=\"exit 0\"\n",
            out.display()
        ),
    )
    .unwrap();
    p
}

/// BUG: a held (long-polling) beat authenticated once, waited up to 25 seconds, and then
/// built its reply from the node record it had cloned at the start. Revoking the node
/// during the wait did not wake it and was never re-checked, so a save landing afterwards
/// was pushed, in full, to a node the owner had already revoked.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_revoked_node_with_a_beat_in_flight_receives_nothing() {
    let h = start().await;
    let dir = scratch("agent");
    let out = dir.join("app.env");
    enroll(&h, &dir, "vps-01").await;
    let cfg = config(&dir, &out);
    let (d2, c2) = (dir.clone(), cfg.clone());
    let mut agent = blocking(move || Agent::new(&d2, &c2)).await.unwrap();
    // Two beats: the file is pushed, then reported in sync.
    for _ in 0..2 {
        let (a, r) = blocking(move || {
            let r = agent.beat_once(0);
            (agent, r)
        })
        .await;
        agent = a;
        r.unwrap();
    }
    assert!(std::fs::read_to_string(&out)
        .unwrap()
        .contains("TOKEN=first"));

    // The node holds a beat open...
    let held = tokio::spawn(blocking(move || {
        let r = agent.beat_once(10);
        (agent, r)
    }));
    tokio::time::sleep(Duration::from_millis(400)).await;
    // ...the owner revokes it...
    let (_, list) = h.owner("GET", "/api/nodes", serde_json::json!({})).await;
    let id = list["nodes"][0]["id"].as_str().unwrap().to_string();
    let (st, _) = h
        .owner("DELETE", &format!("/api/nodes/{id}"), serde_json::json!({}))
        .await;
    assert_eq!(st, 204);
    // ...and then the vault changes, which wakes every held beat.
    let (st, _) = h.owner("PUT", "/api/vault", vault("second")).await;
    assert!(st == 200 || st == 204, "{st}");

    let (_agent, reply) = held.await.unwrap();
    let pushed = reply
        .as_ref()
        .map(|r| !r.actions.is_empty())
        .unwrap_or(false);
    assert!(!pushed, "a revoked node was sent content: {reply:?}");
    assert!(
        std::fs::read_to_string(&out)
            .unwrap()
            .contains("TOKEN=first"),
        "the revoked node's file was overwritten"
    );
    assert!(
        reply.is_err(),
        "a revoked node's held beat must be refused, not answered"
    );
}

async fn pull_agent(h: &Hub, name: &str, file: &Path) -> (Agent, String, PathBuf) {
    let dir = scratch(&format!("agent-{name}"));
    enroll(h, &dir, name).await;
    let cfg = dir.join("node.toml");
    std::fs::write(
        &cfg,
        format!(
            "[[target]]\nid=\"wg\"\npath='{}'\nproject=\"edge\"\nexporter=\"wireguard\"\nmode=\"pull\"\n",
            file.display()
        ),
    )
    .unwrap();
    let (d2, c2) = (dir.clone(), cfg.clone());
    let mut agent = blocking(move || Agent::new(&d2, &c2)).await.unwrap();
    let (a, _) = blocking(move || {
        let r = agent.beat_once(0);
        (agent, r)
    })
    .await;
    let (_, list) = h.owner("GET", "/api/nodes", serde_json::json!({})).await;
    let id = list["nodes"][0]["id"].as_str().unwrap().to_string();
    (a, id, dir)
}

async fn beat(mut agent: Agent) -> Agent {
    let (a, _) = blocking(move || {
        let r = agent.beat_once(0);
        (agent, r)
    })
    .await;
    a
}

/// BUG: `request_pull` only recorded a want. Bytes uploaded by an earlier pull that nobody
/// fetched stayed for two minutes, and a second pull's fetch returned those OLD bytes
/// before the node had uploaded the new ones; `--into-chunk --apply` then wrote the stale
/// file into the vault.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_second_pull_never_returns_the_bytes_of_the_first() {
    let h = start().await;
    let file = scratch("pullfile").join("wg0.conf");
    std::fs::write(&file, "[Interface]\nPrivateKey = OLD-VALUE\n").unwrap();
    let (mut agent, id, _dir) = pull_agent(&h, "laptop", &file).await;
    // Pull #1: requested, uploaded, and never fetched.
    h.owner(
        "POST",
        &format!("/api/nodes/{id}/pull"),
        serde_json::json!({ "target": "wg" }),
    )
    .await;
    agent = beat(agent).await;
    // The file changes on the node. The owner pulls again, before the node beats.
    std::fs::write(&file, "[Interface]\nPrivateKey = NEW-VALUE\n").unwrap();
    h.owner(
        "POST",
        &format!("/api/nodes/{id}/pull"),
        serde_json::json!({ "target": "wg" }),
    )
    .await;
    let (st, body) = h
        .owner(
            "GET",
            &format!("/api/nodes/{id}/content/wg"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(
        st, 204,
        "the fetch returned bytes the node had not uploaded for THIS request: {body}"
    );
    // After the node beats, the new file is what arrives.
    let _agent = beat(agent).await;
    let (st, got) = h
        .owner(
            "GET",
            &format!("/api/nodes/{id}/content/wg"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(st, 200);
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(got["content_b64"].as_str().unwrap())
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("NEW-VALUE"));
}

/// BUG: `accept` recorded whatever the node last reported as the accepted file. A file that
/// changed between the owner's fetch and the accept was accepted as in sync without
/// anyone having reviewed it. With the fetched hash named, a change is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn accepting_a_file_that_changed_after_it_was_fetched_is_refused() {
    let h = start().await;
    let file = scratch("pullfile2").join("wg0.conf");
    std::fs::write(&file, "[Interface]\nPrivateKey = REVIEWED\n").unwrap();
    let (mut agent, id, _dir) = pull_agent(&h, "phone", &file).await;
    h.owner(
        "POST",
        &format!("/api/nodes/{id}/pull"),
        serde_json::json!({ "target": "wg" }),
    )
    .await;
    agent = beat(agent).await;
    let (_, got) = h
        .owner(
            "GET",
            &format!("/api/nodes/{id}/content/wg"),
            serde_json::json!({}),
        )
        .await;
    let reviewed = got["sha256"].as_str().unwrap().to_string();
    // The file changes, and the node reports the new hash before the owner accepts.
    std::fs::write(&file, "[Interface]\nPrivateKey = NEVER-SEEN\n").unwrap();
    let _agent = beat(agent).await;
    let (st, body) = h
        .owner(
            "POST",
            &format!("/api/nodes/{id}/accept"),
            serde_json::json!({ "target": "wg", "sha256": reviewed }),
        )
        .await;
    assert_eq!(st, 409, "{body}");
    // Without a hash the old behaviour stands (older clients), and the fetched hash of the
    // CURRENT file is accepted.
    let (st, _) = h
        .owner(
            "POST",
            &format!("/api/nodes/{id}/accept"),
            serde_json::json!({ "target": "wg" }),
        )
        .await;
    assert_eq!(st, 200);
}
