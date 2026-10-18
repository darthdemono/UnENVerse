// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Signing in as a named user ("Signing in as a named user", README), through
//! the real `unv` against a real `unv-server`:
//!
//! * `login` caches a session in `sessions.json` (0600), after which commands
//!   need no password;
//! * the token never appears in anything `unv` prints;
//! * sessions are filed by subject, so naming a subject selects one;
//! * a session the server no longer accepts is cleared and the error tells you
//!   to log in again, rather than failing the same way forever;
//! * `logout` clears them.

mod common;
#[path = "common/server.rs"]
mod server;

use common::Vault;
use serde_json::Value;
use server::Server;

const ALICE: &str = "alice-password-12345";
const BOB: &str = "bob-password-67890";

fn setup(tag: &str) -> (Vault, Server) {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "WebKey", "--key", "ws-secret-11"]);
    let s = Server::start(&v, false);
    assert_eq!(
        v.on(&s, &["user", "add", "alice", "--user-password", ALICE])
            .code,
        0
    );
    assert_eq!(
        v.on(&s, &["user", "add", "bob", "--user-password", BOB])
            .code,
        0
    );
    for who in ["alice", "bob"] {
        assert_eq!(
            v.on(&s, &["perm", "set", "user", who, "--read", "vault:*"])
                .code,
            0
        );
    }
    (v, s)
}

fn login(v: &Vault, s: &Server, user: &str, password: &str) -> common::Out {
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
    c.env_remove("UNV_SERVER_URL")
        .env_remove("UNV_TOKEN")
        .env("UNV_PASSWORD", password)
        .env("HOME", v.path("home"))
        .env("XDG_DATA_HOME", v.path("data"))
        .env("XDG_CONFIG_HOME", v.path("config"))
        .env("XDG_STATE_HOME", v.path("state"))
        .stdin(std::process::Stdio::null());
    common::pin_state_files(&mut c, &v.dir);
    c.args(["login", "--server", &s.url, "--user", user]);
    let o = c.output().unwrap();
    common::Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

/// `unv` with no password and no token: only a cached session can authenticate it.
fn cached(v: &Vault, s: &Server, extra: &[&str], args: &[&str]) -> common::Out {
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
    for var in [
        "UNV_SERVER_URL",
        "UNV_TOKEN",
        "UNV_PASSWORD",
        "UNV_USER",
        "UNV_ENV_FILE",
    ] {
        c.env_remove(var);
    }
    c.env("HOME", v.path("home"))
        .env("XDG_DATA_HOME", v.path("data"))
        .env("XDG_CONFIG_HOME", v.path("config"))
        .env("XDG_STATE_HOME", v.path("state"))
        .stdin(std::process::Stdio::null());
    common::pin_state_files(&mut c, &v.dir);
    c.args(["--server", &s.url, "--yes", "--json"])
        .args(extra)
        .args(args);
    let o = c.output().unwrap();
    common::Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn session_file(v: &Vault) -> std::path::PathBuf {
    v.dir.join("state").join("unv").join("sessions.json")
}

fn tokens_in(v: &Vault) -> Vec<String> {
    let raw = std::fs::read_to_string(session_file(v)).unwrap_or_default();
    let doc: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let mut found = Vec::new();
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                for (k, val) in m {
                    if k == "token" {
                        if let Some(s) = val.as_str() {
                            out.push(s.to_string());
                        }
                    }
                    walk(val, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(&doc, &mut found);
    found
}

#[cfg(unix)]
#[test]
fn login_caches_a_private_session_after_which_no_password_is_needed() {
    use common::mode_of;
    let (v, s) = setup("login");
    let o = login(&v, &s, "alice", ALICE);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(
        mode_of(&session_file(&v)),
        0o600,
        "the session file holds a live bearer token"
    );

    let list = cached(&v, &s, &[], &["list"]);
    assert_eq!(
        list.code,
        0,
        "no password, no token: only the cached session: {}",
        list.all()
    );
    assert_eq!(list.json()["data"]["count"], 1);
}

#[test]
fn a_session_token_is_never_printed() {
    let (v, s) = setup("notoken");
    let login_out = login(&v, &s, "alice", ALICE);
    let tokens = tokens_in(&v);
    assert!(
        !tokens.is_empty(),
        "the session must hold a token: {}",
        std::fs::read_to_string(session_file(&v)).unwrap()
    );
    let mut printed = login_out.all();
    for args in [vec!["whoami"], vec!["sessions"], vec!["list"]] {
        printed.push_str(&cached(&v, &s, &[], &args).all());
    }
    for t in tokens {
        assert!(
            !printed.contains(&t),
            "a session token reached the terminal"
        );
    }
}

#[test]
fn whoami_and_sessions_describe_the_cached_login_without_secrets() {
    let (v, s) = setup("whoami");
    login(&v, &s, "alice", ALICE);
    let who = cached(&v, &s, &[], &["whoami"]);
    assert_eq!(who.code, 0, "{}", who.all());
    let data = &who.json()["data"];
    assert_eq!(data["mode"], "remote");
    assert_eq!(data["session_valid"], true);
    assert_eq!(data["server"], s.url.as_str());

    let sessions = cached(&v, &s, &[], &["sessions"]).json();
    let rows = sessions["data"]["sessions"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["subject"], "alice");
}

#[test]
fn naming_a_subject_selects_its_session() {
    let (v, s) = setup("subjects");
    login(&v, &s, "alice", ALICE);
    login(&v, &s, "bob", BOB);
    let sessions = cached(&v, &s, &[], &["sessions"]).json();
    assert_eq!(
        sessions["data"]["sessions"].as_array().unwrap().len(),
        2,
        "{sessions}"
    );

    for who in ["alice", "bob"] {
        let o = cached(&v, &s, &["--user", who], &["whoami"]);
        assert_eq!(o.code, 0, "{}", o.all());
        let cached_subjects = o.json()["data"]["cached_subjects"].to_string();
        assert!(
            cached_subjects.contains("alice") && cached_subjects.contains("bob"),
            "{cached_subjects}"
        );
    }
}

#[test]
fn a_session_the_server_no_longer_accepts_is_cleared_and_the_error_says_to_log_in_again() {
    let (v, s) = setup("rejected");
    login(&v, &s, "alice", ALICE);
    assert_eq!(cached(&v, &s, &[], &["list"]).code, 0);

    // Changing a user's credentials ends that user's sessions on the server
    // (clearing the password is the non-interactive way to change them).
    let changed = v.on(&s, &["user", "passwd", "alice", "--clear"]);
    assert_eq!(changed.code, 0, "{}", changed.all());

    let o = cached(&v, &s, &[], &["list"]);
    assert_eq!(
        o.code,
        5,
        "a rejected session is a denial, not a nameless error: {}",
        o.all()
    );
    assert!(
        o.all().to_lowercase().contains("log in") || o.all().to_lowercase().contains("login"),
        "the error must say what to do next: {}",
        o.all()
    );
    assert!(
        tokens_in(&v).is_empty(),
        "the dead session must be cleared, or every later command fails the same way"
    );
}

#[test]
fn logout_clears_every_session_for_the_server() {
    let (v, s) = setup("logout");
    login(&v, &s, "alice", ALICE);
    login(&v, &s, "bob", BOB);
    let o = cached(&v, &s, &[], &["logout"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        tokens_in(&v).is_empty(),
        "{}",
        std::fs::read_to_string(session_file(&v)).unwrap_or_default()
    );
    let after = cached(&v, &s, &[], &["list"]);
    assert_eq!(after.code, 5, "{}", after.all());
}

#[test]
fn a_wrong_password_at_login_is_denied_and_caches_nothing() {
    let (v, s) = setup("badlogin");
    let o = login(&v, &s, "alice", "definitely-wrong-password");
    assert_eq!(o.code, 5, "{}", o.all());
    assert!(tokens_in(&v).is_empty());
}
