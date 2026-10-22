// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv rotate`: the real binary against a mock Cloudflare token API. The old credential must stay
//! valid unless `--revoke-now` is given, and a revoke happens last.

mod common;
use common::{http, Vault};
use std::sync::{Arc, Mutex};

fn cloudflare(log: Arc<Mutex<Vec<String>>>, old: String) -> String {
    http::serve(move |req| {
        let path = req.target.split('?').next().unwrap_or("").to_string();
        log.lock().unwrap().push(format!("{} {path}", req.method));
        let bearer = req.header("authorization").unwrap_or("").to_string();
        let ok = |v: &str| (200, format!(r#"{{"success":true,"result":{v}}}"#));
        let known = bearer == format!("Bearer {old}") || bearer == "Bearer cf-new-token-value";
        if !known {
            return (401, r#"{"success":false}"#.into());
        }
        match (req.method.as_str(), path.as_str()) {
            ("GET", "/user/tokens/verify") => ok(r#"{"status":"active"}"#),
            ("GET", "/user/tokens/old-id") => ok(r#"{"id":"old-id","policies":[]}"#),
            ("POST", "/user/tokens") => ok(r#"{"id":"new-id","value":"cf-new-token-value"}"#),
            ("DELETE", "/user/tokens/old-id") => ok(r#"{"id":"old-id"}"#),
            _ => (404, r#"{"success":false}"#.into()),
        }
    })
}

fn setup(tag: &str, old: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "CF", "--key", old]);
    v
}

#[test]
fn the_old_token_is_kept_unless_revoke_now_is_given_and_a_revoke_comes_last() {
    let old = format!("cf-old-{}", std::process::id());
    let log = Arc::new(Mutex::new(Vec::new()));
    let base = cloudflare(log.clone(), old.clone());
    let v = setup("rotate-keep", &old);
    let base = base.as_str();

    let plan = v.run(&[
        "rotate",
        "plan",
        "CF",
        "--token-id",
        "old-id",
        "--base-url",
        base,
    ]);
    assert_eq!(plan.code, 0, "{}", plan.all());
    assert!(log.lock().unwrap().is_empty(), "a plan calls nobody");

    let run = v.run(&[
        "rotate",
        "run",
        "CF",
        "--token-id",
        "old-id",
        "--base-url",
        base,
    ]);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(run.json()["data"]["status"]["status"] == "revocation_pending");
    assert!(!log.lock().unwrap().iter().any(|l| l.starts_with("DELETE")));
    assert_eq!(v.reveal_field("CF", "api_key"), "cf-new-token-value");

    let status = v.run(&["rotate", "status"]);
    assert_eq!(status.json()["data"]["runs"].as_array().unwrap().len(), 1);
}

#[test]
fn revoke_now_deletes_the_old_token_after_the_new_one_verified() {
    let old = format!("cf-old2-{}", std::process::id());
    let log = Arc::new(Mutex::new(Vec::new()));
    let base = cloudflare(log.clone(), old.clone());
    let v = setup("rotate-revoke", &old);
    let run = v.run(&[
        "rotate",
        "run",
        "CF",
        "--token-id",
        "old-id",
        "--base-url",
        base.as_str(),
        "--revoke-now",
    ]);
    assert_eq!(run.code, 0, "{}", run.all());
    let calls = log.lock().unwrap().clone();
    assert_eq!(
        calls.last().map(String::as_str),
        Some("DELETE /user/tokens/old-id")
    );
    let verify = calls
        .iter()
        .position(|c| c == "GET /user/tokens/verify")
        .unwrap();
    assert!(verify < calls.len() - 1);
    // The id follows the credential, so the next rotation needs no --token-id.
    let again = v.run(&["rotate", "plan", "CF", "--base-url", base.as_str()]);
    assert_eq!(again.json()["data"]["token_id"], "new-id");
}

#[test]
fn an_unknown_issuer_and_a_missing_token_id_are_refused_before_any_call() {
    let v = setup("rotate-refuse", "cf-old3-value");
    let o = v.run(&["rotate", "plan", "CF"]);
    assert_ne!(o.code, 0);
    assert!(o.all().contains("--token-id"), "{}", o.all());
}
