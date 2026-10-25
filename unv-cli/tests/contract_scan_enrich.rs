// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! `unv scan` and `unv enrich` ("Safety checks", README):
//!
//! * scan reports weak, expired, duplicated, compromised and never-rotated
//!   secrets and dangling `${...}` references, by severity, and never prints a
//!   secret value;
//! * `--severity` narrows what is shown and the counts agree with the list;
//! * enrich proposes metadata from public key prefixes and changes nothing until
//!   `--apply`; an existing value survives unless `--force`.

mod common;
use common::Vault;
use serde_json::Value;

fn issues(v: &Vault, extra: &[&str]) -> Vec<Value> {
    let mut a = vec!["scan"];
    a.extend_from_slice(extra);
    v.ok(&a)["issues"].as_array().cloned().unwrap_or_default()
}

fn has(issues: &[Value], subject: &str, needle: &str) -> bool {
    issues.iter().any(|i| {
        i["subject"].as_str().unwrap().contains(subject)
            && i["message"].as_str().unwrap().contains(needle)
    })
}

const DUP: &str = "same-secret-value-12345";

fn seeded(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "Weak", "--key", "abc"]);
    v.ok(&["entry", "add", "Dup1", "--key", DUP]);
    v.ok(&["entry", "add", "Dup2", "--key", DUP]);
    v.ok(&[
        "entry",
        "add",
        "Old",
        "--key",
        "other-secret-value-12345",
        "--expires",
        "2020-01-01",
    ]);
    v.ok(&["entry", "add", "Pw", "--key", "password-looking-value-1"]);
    v
}

#[test]
fn scan_finds_weak_expired_duplicate_and_common_passwords_without_printing_values() {
    let v = seeded("scanbasic");
    let all = issues(&v, &[]);
    assert!(has(&all, "Weak", "Short or weak"), "{all:?}");
    assert!(has(&all, "Old", "Expired on 2020-01-01"), "{all:?}");
    assert!(
        has(&all, "Dup1", "Same secret value in 2 entries"),
        "{all:?}"
    );
    assert!(has(&all, "Pw", "common weak value"), "{all:?}");
    let o = v.run(&["scan"]);
    assert!(
        !o.all().contains(DUP) && !o.all().contains("password-looking-value-1"),
        "a report never carries a secret"
    );
}

#[test]
fn scan_severity_narrows_the_list_and_the_counts_agree() {
    let v = seeded("scansev");
    let all = v.ok(&["scan"]);
    let n = |s: &str| {
        all["issues"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["severity"] == s)
            .count() as u64
    };
    assert_eq!(
        (
            all["high"].as_u64().unwrap(),
            all["medium"].as_u64().unwrap(),
            all["low"].as_u64().unwrap()
        ),
        (n("high"), n("med"), n("low"))
    );
    assert_eq!(
        all["count"].as_u64().unwrap(),
        all["issues"].as_array().unwrap().len() as u64
    );

    let high = v.ok(&["scan", "--severity", "high"]);
    assert!(high["issues"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["severity"] == "high"));
    assert_eq!(
        high["count"].as_u64().unwrap(),
        all["high"].as_u64().unwrap()
    );
    let med = v.ok(&["scan", "--severity", "med"]);
    assert!(
        med["issues"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["severity"] != "low"),
        "med includes high and med only"
    );
    assert!(v.run(&["scan", "--severity", "bogus"]).code != 0);
}

#[test]
fn a_compromised_entry_is_high_and_a_dangling_chunk_reference_is_reported_once() {
    let v = Vault::new("scancomp");
    v.ok(&[
        "entry",
        "add",
        "Leaky",
        "--key",
        "long-enough-secret-value-1",
    ]);
    v.ok(&["entry", "compromise", "Leaky"]);
    v.ok(&["project", "add", "Web"]);
    v.ok(&[
        "project", "chunk", "add", "Web", "app", "--type", "env_file",
    ]);
    v.ok(&[
        "project",
        "chunk",
        "set",
        "Web",
        "app",
        "A=${Nowhere/KEY}",
        "B=${Nowhere/KEY}",
    ]);
    let all = issues(&v, &[]);
    assert!(
        all.iter().any(|i| i["subject"] == "Leaky"
            && i["severity"] == "high"
            && i["message"].as_str().unwrap().contains("COMPROMISED")),
        "{all:?}"
    );
    let dangling: Vec<_> = all
        .iter()
        .filter(|i| i["message"].as_str().unwrap().contains("Nowhere"))
        .collect();
    assert_eq!(
        dangling.len(),
        1,
        "the same dangling reference in two fields is one finding: {all:?}"
    );
}

#[test]
fn a_clean_vault_has_nothing_high() {
    let v = Vault::new("scanclean");
    v.ok(&[
        "entry",
        "add",
        "Good",
        "--key",
        "a-long-random-looking-value-9f3k2",
    ]);
    let high = v.ok(&["scan", "--severity", "high"]);
    assert_eq!(high["count"], 0, "{high}");
}

const GH: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

#[test]
fn enrich_previews_from_public_prefixes_and_changes_nothing_without_apply() {
    let v = Vault::new("enrichprev");
    v.ok(&["entry", "add", "GitHub", "--key", GH]);
    v.ok(&[
        "entry",
        "add",
        "Plain",
        "--key",
        "no-known-prefix-value-1234",
    ]);
    let before = v.ok(&["get", "GitHub"])["entries"][0].clone();
    let o = v.ok(&["enrich"]);
    assert_eq!(o["applied"], false, "{o}");
    assert_eq!(o["entries_matched"], 1, "only the recognised entry: {o}");
    let fields: Vec<_> = o["plans"][0]["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["field"].as_str().unwrap().to_string())
        .collect();
    assert!(
        fields.contains(&"api_url".to_string()) && fields.contains(&"tags".to_string()),
        "{fields:?}"
    );
    assert!(
        o["plans"][0]["fingerprint"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"),
        "identified by fingerprint, never the key"
    );
    assert!(!v.run(&["enrich"]).all().contains(GH));
    assert_eq!(
        v.ok(&["get", "GitHub"])["entries"][0],
        before,
        "a preview writes nothing"
    );
}

#[test]
fn enrich_apply_writes_the_proposals_and_a_second_run_has_nothing_to_add() {
    let v = Vault::new("enrichapply");
    v.ok(&["entry", "add", "GitHub", "--key", GH]);
    let o = v.ok(&["enrich", "--apply"]);
    assert_eq!(o["applied"], true, "{o}");
    let e = v.ok(&["get", "GitHub"])["entries"][0].clone();
    assert_eq!(e["api_url"], "https://api.github.com", "{e}");
    assert_eq!(e["tags"], serde_json::json!(["github"]), "{e}");
    let again = v.ok(&["enrich"]);
    // Bug it pins: `api_key` counts as a blank secretType, so every run proposed
    // the same `api_key` again and `entries_matched` never reached 0.
    assert_eq!(again["fields"], 0, "nothing left to fill: {again}");
    assert_eq!(again["entries_matched"], 0, "{again}");
}

#[test]
fn enrich_keeps_an_existing_value_unless_forced_and_only_limits_the_entries() {
    let v = Vault::new("enrichforce");
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        GH,
        "--url",
        "https://git.example.org",
    ]);
    v.ok(&["enrich", "--apply"]);
    assert_eq!(
        v.ok(&["get", "GitHub"])["entries"][0]["api_url"],
        "https://git.example.org",
        "a value you set is yours"
    );
    v.ok(&["enrich", "--apply", "--force"]);
    assert_eq!(
        v.ok(&["get", "GitHub"])["entries"][0]["api_url"],
        "https://api.github.com",
        "--force replaces it"
    );
    let none = v.ok(&["enrich", "--only", "nomatch"]);
    assert_eq!(none["entries_matched"], 0, "{none}");
}

#[test]
fn catalogue_show_names_the_bundled_table_and_diff_needs_a_cached_one() {
    let v = Vault::new("catalogue");
    let s = v.ok(&["catalogue", "show"]);
    assert_eq!(s["source"], "bundled", "{s}");
    assert!(s["providers"].as_u64().unwrap() > 10);
    assert_eq!(
        v.run(&["catalogue", "diff"]).code,
        3,
        "no verified catalogue is cached"
    );
}

#[test]
fn an_unsigned_build_cannot_check_for_updates() {
    let v = Vault::new("updatecheck");
    let o = v.run(&["update", "check"]);
    assert_eq!(
        o.code,
        5,
        "no pinned key means nothing can be verified, so it is denied: {}",
        o.all()
    );
}
