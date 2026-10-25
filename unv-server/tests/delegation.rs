// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! A `manage-users` delegate with a narrow data scope must not be able to act as, or widen, an
//! account that reaches more than the delegate does. Tier (who outranks whom) said nothing about
//! scope, so a delegate could reset the password of, mint a token for, or clear the restriction of
//! a lower-tier user with wider access, then read what the delegate could not.

use axum::extract::ConnectInfo;
use axum::http::StatusCode;
use axum::Router;
use serde_json::{json, Value};
use std::net::SocketAddr;
use tower::ServiceExt;
use unv_server::{build_router, AppState};

struct Rig {
    router: Router,
    owner: String,
}

fn entry(id: &str, ty: &str, project: &str) -> Value {
    json!({ "id": id, "provider": format!("P-{id}"), "secretType": ty,
            "api_key": format!("synthetic-{id}"), "projectIds": [project], "environment": "prod" })
}

fn rig() -> Rig {
    let dir = std::env::temp_dir().join(format!("unv-delegation-{}-{}", std::process::id(), {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::SeqCst)
    }));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let state = AppState::new(
        dir.join("vault.db"),
        dir.join("vault.salt"),
        None,
        480,
        24,
        false,
    );
    let key = [42u8; 32];
    let owner = state.adopt_owner_key(key, "owner".into());
    let conn = vault_core::open_db(&dir.join("vault.db"), &key).unwrap();
    vault_core::init_schema(&conn).unwrap();
    vault_core::save_vault(
        &conn,
        json!({ "api_keys": [entry("e1", "api_key", "pa"), entry("e2", "ssh_key", "pb")],
                "user_categories": [],
                "projects": [{"id":"pa","name":"A"},{"id":"pb","name":"B"}] }),
        vault_core::SaveCtx::default(),
    )
    .unwrap();
    Rig {
        router: build_router(state, 8743),
        owner,
    }
}

async fn call(
    r: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let mut req = req
        .body(axum::body::Body::from(
            body.map(|b| b.to_string()).unwrap_or_default(),
        ))
        .unwrap();
    req.extensions_mut().insert(ConnectInfo::<SocketAddr>(
        "127.0.0.1:50000".parse().unwrap(),
    ));
    let resp = r.clone().oneshot(req).await.unwrap();
    let (parts, body) = resp.into_parts();
    let bytes = axum::body::to_bytes(body, 16 << 20).await.unwrap();
    (
        parts.status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn user(rig: &Rig, name: &str, pw: &str, read: &str, write: &str) -> String {
    let (st, u) = call(
        &rig.router,
        "POST",
        "/api/users",
        Some(&rig.owner),
        Some(json!({"username": name, "password": pw})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{u}");
    let id = u["id"].as_str().unwrap().to_string();
    let (st, b) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{id}/permissions"),
        Some(&rig.owner),
        Some(json!({"read": read, "write": write})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT, "{b}");
    id
}

async fn login(rig: &Rig, name: &str, pw: &str) -> String {
    let (st, b) = call(
        &rig.router,
        "POST",
        "/api/auth",
        None,
        Some(json!({"username": name, "password": pw})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    b["token"].as_str().unwrap().to_string()
}

async fn visible(rig: &Rig, token: &str) -> Vec<String> {
    let (st, v) = call(&rig.router, "GET", "/api/vault", Some(token), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    v["api_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

async fn delegate_class(rig: &Rig, delegate: &str) {
    let (_, class) = call(
        &rig.router,
        "POST",
        "/api/classes",
        Some(&rig.owner),
        Some(json!({"name": "mods", "cap_manage_users": true})),
    )
    .await;
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{delegate}/class"),
        Some(&rig.owner),
        Some(json!({"class_id": class["id"]})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_narrow_delegate_cannot_take_over_a_wider_subordinate() {
    let rig = rig();
    let delegate = user(
        &rig,
        "mod1",
        "delegate-pass-1",
        "type:api_key",
        "type:api_key",
    )
    .await;
    delegate_class(&rig, &delegate).await;
    let victim = user(&rig, "viewer1", "victim-pass-12", "type:*", "").await;
    let dtok = login(&rig, "mod1", "delegate-pass-1").await;
    assert_eq!(visible(&rig, &dtok).await, vec!["e1"]);

    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{victim}/password"),
        Some(&dtok),
        Some(json!({"password": "attacker-chosen-1"})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "password reset of a wider account"
    );
    let (st, _) = call(
        &rig.router,
        "POST",
        &format!("/api/users/{victim}/tokens"),
        Some(&dtok),
        Some(json!({"description": "x"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "token for a wider account");
    let (st, _) = call(
        &rig.router,
        "DELETE",
        &format!("/api/users/{victim}/totp"),
        Some(&dtok),
        None,
    )
    .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "second factor reset of a wider account"
    );

    // A subordinate inside the delegate's scope can still be managed.
    let inside = user(&rig, "inside", "inside-pass-12", "type:api_key", "").await;
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{inside}/password"),
        Some(&dtok),
        Some(json!({"password": "fresh-password-12"})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delegate_cannot_widen_a_subordinate_by_clearing_their_restriction_or_changing_class() {
    let rig = rig();
    // A wide class, and a subordinate in it narrowed to project A by an individual expression.
    let (_, wide) = call(
        &rig.router,
        "POST",
        "/api/classes",
        Some(&rig.owner),
        Some(json!({"name": "wide"})),
    )
    .await;
    let wide_id = wide["id"].as_str().unwrap().to_string();
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/classes/{wide_id}/permissions"),
        Some(&rig.owner),
        Some(json!({"read": "type:*", "write": ""})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let delegate = user(&rig, "d", "delegate-pass-2", "project:A", "project:A").await;
    delegate_class(&rig, &delegate).await;
    let sub = user(&rig, "s", "sub-password-12", "project:A", "").await;
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{sub}/class"),
        Some(&rig.owner),
        Some(json!({"class_id": wide_id})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let dtok = login(&rig, "d", "delegate-pass-2").await;
    assert_eq!(
        visible(&rig, &login(&rig, "s", "sub-password-12").await).await,
        vec!["e1"]
    );

    // Clearing the narrowing would leave the wide class: refused, and nothing changes.
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{sub}/permissions"),
        Some(&dtok),
        Some(json!({"read": "", "write": ""})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(
        visible(&rig, &login(&rig, "s", "sub-password-12").await).await,
        vec!["e1"],
        "the restriction is still there"
    );
}
