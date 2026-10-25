// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The error envelope and exit codes the README documents ("A machine-readable
//! envelope"). A caller branches on these, so each one is checked against the
//! situation it names, through the real binary:
//!
//! 2 usage, 3 not_found, 4 ambiguous (with candidates), 5 denied, 6 conflict,
//! 7 unavailable, 8 needs_confirmation, 9 redacted, 10 invalid.
//!
//! The difference between 5 and 7 is the one the README stresses: a caller
//! retries a 7, and must never be told "unreachable" about a refusal.

mod common;
use common::Vault;

#[test]
fn a_success_is_an_ok_envelope_with_the_command_name() {
    let v = Vault::new("ok");
    let o = v.run(&["entry", "add", "GitHub", "--key", "k-one"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let j = o.json();
    assert_eq!(j["ok"], true);
    assert_eq!(j["command"], "entry.add");
    assert!(j["data"].is_object());
}

#[test]
fn a_failure_is_an_error_envelope_with_a_code_and_a_message() {
    let v = Vault::new("fail");
    let o = v.run(&["get", "Nope"]);
    let j = o.json();
    assert_eq!(j["ok"], false);
    assert!(j["error"]["code"].is_string());
    assert!(!j["error"]["message"].as_str().unwrap_or("").is_empty());
}

#[test]
fn an_unknown_entry_is_not_found_3() {
    let v = Vault::new("nf");
    v.ok(&["entry", "add", "GitHub", "--key", "k-one"]);
    for args in [
        vec!["get", "Nope"],
        vec!["entry", "set", "Nope", "--key", "x"],
        vec!["entry", "rm", "Nope"],
    ] {
        let o = v.run(&args);
        assert_eq!(o.code, 3, "{args:?}: {}", o.all());
        assert_eq!(o.error_code(), "not_found");
    }
}

#[test]
fn a_name_matching_several_entries_is_ambiguous_4_and_lists_them() {
    let v = Vault::new("amb");
    v.ok(&["entry", "add", "GitHub", "--key", "k-one"]);
    v.ok(&["entry", "add", "GitLab", "--key", "k-two"]);
    // `entry rm git` with both present lists both and exits 4. It never guesses.
    let o = v.run(&["entry", "rm", "git"]);
    assert_eq!(o.code, 4, "{}", o.all());
    assert_eq!(o.error_code(), "ambiguous");
    let candidates = o.json()["error"]["details"]["candidates"].clone();
    let names: Vec<&str> = candidates
        .as_array()
        .expect("details.candidates must list the matches")
        .iter()
        .filter_map(|c| c.as_str())
        .collect();
    assert!(
        names.contains(&"GitHub") && names.contains(&"GitLab"),
        "{names:?}"
    );
    // And nothing was deleted by the refused command.
    let list = v.ok(&["list"]);
    assert_eq!(list["count"], 2);
}

#[test]
fn creating_something_that_exists_is_a_conflict_6() {
    let v = Vault::new("conflict");
    v.ok(&["entry", "add", "GitHub", "--key", "k-one"]);
    let o = v.run(&["entry", "add", "GitHub", "--key", "k-two"]);
    assert_eq!(o.code, 6, "{}", o.all());
    assert_eq!(o.error_code(), "conflict");
    // The existing secret is untouched by the refused add.
    assert_eq!(v.reveal_field("GitHub", "api_key"), "k-one");
}

#[test]
fn a_destructive_command_without_yes_and_without_a_terminal_needs_confirmation_8() {
    let v = Vault::new("confirm");
    v.ok(&["entry", "add", "GitHub", "--key", "k-one"]);
    // No --yes, stdin is not a terminal: a script that forgot --yes must fail
    // loudly instead of deleting something.
    let o = v.run_no_yes(&["entry", "rm", "GitHub"]);
    assert_eq!(o.code, 8, "{}", o.all());
    assert_eq!(o.error_code(), "needs_confirmation");
    assert_eq!(v.ok(&["list"])["count"], 1, "the entry must still be there");
}

#[test]
fn output_that_would_contain_secrets_is_redacted_9() {
    let v = Vault::new("redacted");
    v.ok(&["entry", "add", "GitHub", "--key", "k-one"]);
    let o = v.run(&["export"]);
    assert_eq!(o.code, 9, "{}", o.all());
    assert_eq!(o.error_code(), "redacted");
}

#[test]
fn input_that_is_well_formed_but_unusable_is_invalid_10() {
    let v = Vault::new("invalid");
    // `get` with neither an entry nor --pool names nothing: the command was
    // well-formed, it just did not say which entry.
    let o = v.run(&["get"]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert_eq!(o.error_code(), "invalid");
}

#[test]
fn a_malformed_command_line_is_a_usage_error_2() {
    let v = Vault::new("usage");
    let o = v.run(&["entry", "add"]);
    assert_eq!(o.code, 2, "{}", o.all());
}

#[test]
fn a_wrong_master_password_is_denied_5_not_a_nameless_error() {
    let v = Vault::new("denied");
    v.ok(&["entry", "add", "GitHub", "--key", "k-one"]);
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
    let o = c
        .env_remove("UNV_SERVER_URL")
        .env_remove("UNV_ENV_FILE")
        .env("UNV_PASSWORD", "not-the-password-at-all")
        .env("HOME", v.path("home"))
        .env("XDG_DATA_HOME", v.path("data"))
        .env("XDG_CONFIG_HOME", v.path("config"))
        .env("XDG_STATE_HOME", v.path("state"))
        .args([
            "--db-path",
            v.db().to_str().unwrap(),
            "--yes",
            "--json",
            "list",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert_eq!(o.status.code(), Some(5), "{stdout}");
    let j: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(j["error"]["code"], "denied", "{stdout}");
}

#[test]
fn an_unreachable_server_is_unavailable_7_so_a_caller_may_retry() {
    let v = Vault::new("unavailable");
    // Port 1 on loopback is closed: the server is not there at all.
    let o = v.run_with(&["--server", "http://127.0.0.1:1"], &["list"]);
    assert_eq!(o.code, 7, "{}", o.all());
    assert_eq!(o.error_code(), "unavailable");
}
