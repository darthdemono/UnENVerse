// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv glossary`: answers from the embedded glossary file, with no vault and no password.
use std::process::{Command, Stdio};

fn unv(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_unv"))
        .env_remove("UNV_SERVER_URL")
        .env_remove("UNV_PASSWORD")
        .env_remove("UNV_PASSWORD_FILE")
        .stdin(Stdio::null())
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn a_term_is_explained_without_a_vault_or_a_password() {
    let o = unv(&["glossary", "key pool"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("Key Pool"), "{text}");
    assert!(text.contains("interchangeable credentials"), "{text}");
}

#[test]
fn the_json_envelope_carries_the_whole_term() {
    let o = unv(&["--json", "glossary", "api_key"]);
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["id"], "api-key");
    assert!(v["data"]["definition"]
        .as_str()
        .unwrap()
        .contains("primary secret"));
}

#[test]
fn an_unknown_term_is_not_found_and_a_near_miss_is_suggested() {
    let o = unv(&["glossary", "rate lim"]);
    assert_eq!(o.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&o.stderr).contains("Did you mean"));
    let o = unv(&["glossary", "zzzzqqq"]);
    assert_eq!(o.status.code(), Some(3));
}

#[test]
fn with_no_term_it_summarises_and_a_category_lists_its_terms() {
    let o = unv(&["glossary"]);
    assert!(o.status.success());
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("secret type"), "{text}");
    let o = unv(&["glossary", "--category", "secret type"]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("Recovery Codes"));
    assert_eq!(
        unv(&["glossary", "--category", "nonsense"]).status.code(),
        Some(3)
    );
}
