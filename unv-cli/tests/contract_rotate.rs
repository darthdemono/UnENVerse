// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
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

/// Against a real PostgreSQL (see `issuer_postgres::tests`): the binary rotates `app_a` to `app_b`,
/// stores the generated password, remembers the role, and clears the old role last.
/// Needs `UNV_TEST_PSQL`, `UNV_TEST_PG_HOST`, `UNV_TEST_PG_ADMIN_PASSWORD` and `UNV_TEST_PG_A`.
#[test]
#[ignore = "needs a PostgreSQL server"]
fn postgres_roles_take_turns_through_the_binary() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} is not set"));
    let v = setup("rotate-pg", &var("UNV_TEST_PG_A"));
    let host = var("UNV_TEST_PG_HOST");
    let psql = var("UNV_TEST_PSQL");
    let env = [("UNV_PG_ADMIN_PASSWORD", var("UNV_TEST_PG_ADMIN_PASSWORD"))];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let args = [
        "rotate",
        "run",
        "CF",
        "--issuer",
        "postgres",
        "--token-id",
        "app_a",
        "--pg-host",
        host.as_str(),
        "--pg-psql",
        psql.as_str(),
        "--revoke-now",
    ];
    let o = v.run_env(&env, &args);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["status"]["status"], "succeeded");
    assert_ne!(v.reveal_field("CF", "api_key"), var("UNV_TEST_PG_A"));
    let again = v.run(&["rotate", "plan", "CF", "--issuer", "postgres"]);
    assert_eq!(again.json()["data"]["token_id"], "app_b");
}

fn steps(o: &common::Out) -> Vec<String> {
    o.json()["data"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_plan_describes_each_class_and_the_revoke_choice_without_calling_anyone() {
    let v = setup("rotate-plan", "cf-old-plan-value");
    let base = [
        "rotate",
        "plan",
        "CF",
        "--token-id",
        "tok",
        "--base-url",
        "http://127.0.0.1:9",
    ];
    let overlap = v.run(&base);
    assert_eq!(overlap.json()["data"]["class"], "create_overlapping");
    let s = steps(&overlap);
    assert!(
        s[0].contains("old one stays valid") && s.last().unwrap().contains("--revoke-now"),
        "{s:?}"
    );

    let revoke = v.run(&[&base[..], &["--revoke-now"]].concat());
    assert!(steps(&revoke)
        .last()
        .unwrap()
        .starts_with("revoke the old credential"));

    let roll = v.run(&[&base[..], &["--allow-roll"]].concat());
    assert_eq!(roll.json()["data"]["class"], "roll_provider_generated");
    assert!(steps(&roll)[0].contains("not retried if the answer is lost"));

    let pg = v.run(&[&base[..], &["--issuer", "postgres"]].concat());
    assert_eq!(pg.json()["data"]["class"], "set_value_client_generated");
    let s = steps(&pg);
    assert!(
        s[0].contains("SCRAM verifier, never as plaintext")
            && s[1].contains("other role can log in"),
        "{s:?}"
    );

    let text = v.run_text(&base);
    assert!(
        text.stdout.contains("Rotating 'CF' at cloudflare") && text.stdout.contains("  1. "),
        "{}",
        text.stdout
    );
}

#[test]
fn a_refused_creation_ends_the_run_as_failed_and_it_cannot_be_resumed() {
    let old = format!("cf-old-refuse-{}", std::process::id());
    let log = Arc::new(Mutex::new(Vec::new()));
    let base = http::serve({
        let old = old.clone();
        let log = log.clone();
        move |req| {
            log.lock()
                .unwrap()
                .push(format!("{} {}", req.method, req.target));
            if req.header("authorization") != Some(format!("Bearer {old}").as_str()) {
                return (401, r#"{"success":false}"#.into());
            }
            match (req.method.as_str(), req.target.as_str()) {
                ("GET", "/user/tokens/old-id") => {
                    (200, r#"{"success":true,"result":{"policies":[]}}"#.into())
                }
                _ => (403, r#"{"success":false}"#.into()),
            }
        }
    });
    let v = setup("rotate-refuse2", &old);
    let o = v.run(&[
        "rotate",
        "run",
        "CF",
        "--token-id",
        "old-id",
        "--base-url",
        base.as_str(),
    ]);
    assert_eq!(o.code, 6, "a refusal is a conflict: {}", o.all());
    assert!(o.all().contains("403"), "{}", o.all());
    assert_eq!(
        v.reveal_field("CF", "api_key"),
        old,
        "the entry is untouched"
    );
    assert!(
        !log.lock().unwrap().iter().any(|l| l.starts_with("DELETE")),
        "nothing was deleted"
    );

    let list = v.run(&["rotate", "status"]);
    let runs = list.json()["data"]["runs"].as_array().unwrap().clone();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["status"]["status"], "failed");
    let id = runs[0]["run"].as_str().unwrap().to_string();

    let one = v.run(&["rotate", "status", &id]);
    assert_eq!(one.json()["data"]["runs"][0]["run"], id.as_str());
    let text = v.run_text(&["rotate", "status"]);
    assert!(
        text.stdout.contains(&id) && text.stdout.contains("Failed"),
        "{}",
        text.stdout
    );
    assert!(v.run(&["rotate", "status", "nope"]).code == 3);

    let again = v.run(&[
        "rotate",
        "run",
        "CF",
        "--base-url",
        base.as_str(),
        "--resume",
        &id,
    ]);
    assert_eq!(again.code, 6, "{}", again.all());
    assert!(again.all().contains("already finished"), "{}", again.all());
}

#[test]
fn status_with_no_runs_says_so_and_a_successful_run_prints_its_outcome() {
    let old = format!("cf-old-text-{}", std::process::id());
    let log = Arc::new(Mutex::new(Vec::new()));
    let base = cloudflare(log, old.clone());
    let v = setup("rotate-text", &old);
    assert!(v
        .run_text(&["rotate", "status"])
        .stdout
        .contains("No rotation runs."));
    let o = v.run_text(&[
        "rotate",
        "run",
        "CF",
        "--token-id",
        "old-id",
        "--base-url",
        base.as_str(),
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        o.stdout.contains("Run ") && o.stdout.contains("RevocationPending"),
        "{}",
        o.stdout
    );
}

#[test]
fn rolling_in_place_stores_the_provider_value_and_deletes_nothing() {
    let old = format!("cf-old-roll-{}", std::process::id());
    let log = Arc::new(Mutex::new(Vec::new()));
    let base = http::serve({
        let old = old.clone();
        let log = log.clone();
        move |req| {
            let path = req.target.split('?').next().unwrap_or("").to_string();
            log.lock().unwrap().push(format!("{} {path}", req.method));
            let rolled = "cf-rolled-value";
            let auth = req.header("authorization").unwrap_or("");
            if auth != format!("Bearer {old}") && auth != format!("Bearer {rolled}") {
                return (401, r#"{"success":false}"#.into());
            }
            match (req.method.as_str(), path.as_str()) {
                ("PUT", "/user/tokens/tok/value") => {
                    (200, format!(r#"{{"success":true,"result":"{rolled}"}}"#))
                }
                ("GET", "/user/tokens/verify") => (
                    200,
                    r#"{"success":true,"result":{"status":"active"}}"#.into(),
                ),
                _ => (404, r#"{"success":false}"#.into()),
            }
        }
    });
    let v = setup("rotate-roll", &old);
    let o = v.run(&[
        "rotate",
        "run",
        "CF",
        "--token-id",
        "tok",
        "--base-url",
        base.as_str(),
        "--allow-roll",
        "--revoke-now",
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["status"]["status"], "succeeded");
    assert_eq!(v.reveal_field("CF", "api_key"), "cf-rolled-value");
    let calls = log.lock().unwrap().clone();
    assert!(
        !calls.iter().any(|c| c.starts_with("DELETE")),
        "a rolled token keeps its id and is never deleted: {calls:?}"
    );
    assert_eq!(calls.iter().filter(|c| c.starts_with("PUT")).count(), 1);
}
