// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv guard check` (R35): the real binary reads text on stdin, matches it against the exact
//! values the vault holds, and answers with names only. A secret with no recognisable prefix is
//! found, token-shaped text that is not in the vault is not, and nothing printed or returned holds
//! a value.

mod common;
use common::Vault;

const SECRET: &str = "plain-value-with-no-known-prefix-4821";

fn vault(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "Billing", "--key", SECRET]);
    v
}

#[test]
fn text_holding_a_vault_value_fails_with_exit_10_and_names_the_entry_only() {
    let v = vault("guard-hit");
    let o = v.run_stdin(
        &["guard", "check"],
        &format!("why does this fail: {SECRET} is rejected"),
    );
    assert_eq!(o.code, 10, "{}", o.all());
    assert!(o.all().contains("Billing"), "{}", o.all());
    assert!(!o.all().contains(SECRET), "a value must never be printed");
}

#[test]
fn the_json_report_has_names_and_counts_and_no_value() {
    let v = vault("guard-json");
    let o = v.run_stdin(&["guard", "check"], &format!("a {SECRET} b"));
    assert_eq!(o.code, 10, "{}", o.all());
    assert!(!o.all().contains(SECRET), "{}", o.all());
    let d = &o.json()["error"]["details"];
    assert_eq!(d["blocking"], 1, "{}", o.stdout);
    assert_eq!(d["hits"][0]["entry"], "Billing");
    assert_eq!(d["hits"][0]["field"], "api_key");
}

#[test]
fn ordinary_text_and_token_shaped_text_outside_the_vault_pass() {
    let v = vault("guard-clean");
    let o = v.run_stdin(
        &["guard", "check"],
        "no secrets here, only ghp_0123456789abcdefghij and AKIAIOSFODNN7EXAMPLE",
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["clean"], true);
}

#[test]
fn a_vault_that_cannot_be_opened_is_never_reported_clean() {
    let v = Vault::new("guard-nopw");
    // No vault exists here and no --init: the check must fail, not say "clean".
    let dir = std::env::temp_dir().join("unv-guard-missing-vault");
    let _ = std::fs::remove_dir_all(&dir);
    let o = v.run_env(
        &[("XDG_DATA_HOME", dir.to_str().unwrap())],
        &["guard", "check"],
    );
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains("No vault values found"), "{}", o.all());
}
