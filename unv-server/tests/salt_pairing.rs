// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! An existing vault whose salt file is missing must be reported, not given a fresh salt by
//! whoever calls `/api/unlock` first: a minted salt can never open the existing database.

use axum::extract::ConnectInfo;
use axum::http::StatusCode;
use std::net::SocketAddr;
use tower::ServiceExt;
use unv_server::{build_router, AppState};

#[tokio::test]
async fn unlock_does_not_mint_a_salt_for_an_existing_vault() {
    let dir = std::env::temp_dir().join(format!("unv-salt-pairing-{}", std::process::id()));
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
    // A vault exists, under some key, and its salt file is gone.
    let conn = vault_core::open_db(&dir.join("vault.db"), &[7u8; 32]).unwrap();
    vault_core::init_schema(&conn).unwrap();
    drop(conn);
    assert!(!dir.join("vault.salt").exists());

    let mut req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/unlock")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            r#"{"password":"some-password-12345"}"#,
        ))
        .unwrap();
    req.extensions_mut().insert(ConnectInfo::<SocketAddr>(
        "203.0.113.7:50000".parse().unwrap(),
    ));
    let resp = build_router(state, 8743).oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::OK);
    assert!(
        !dir.join("vault.salt").exists(),
        "a new salt was minted for an existing vault"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
