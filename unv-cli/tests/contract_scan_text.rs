// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The health scan as a person reads it, and the checks that go with specific
//! credential types:
//!
//! * an empty vault is reported as empty rather than as clean; otherwise there is one row
//!   per issue, with a summary that adds up;
//! * a recovery-code set is flagged at two left (medium) and none left (high), because
//!   the next lockout is permanent;
//! * a wallet phrase that fails its BIP39 checksum is a high issue, and the phrase is
//!   never printed;
//! * a stored browser session nobody has confirmed is flagged until it is verified, and
//!   is not nagged about rotation, which nothing can do for a session;
//! * recording a rotation clears "never rotated";
//! * `status` says where the CLI points and what the vault holds, and warns that the
//!   salt file must be backed up with the database.

mod common;

use common::Vault;
use serde_json::Value;

fn issues(v: &Vault) -> Vec<Value> {
    v.ok(&["scan"])["issues"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn has(list: &[Value], subject: &str, needle: &str) -> Option<Value> {
    list.iter()
        .find(|i| {
            i["subject"].as_str().unwrap_or("").contains(subject)
                && i["message"].as_str().unwrap_or("").contains(needle)
        })
        .cloned()
}

const STRONG: &str = "a-long-enough-secret-value-1";

#[test]
fn an_empty_vault_is_not_scanned_and_says_so() {
    let v = Vault::new("scan-text-empty");
    let o = v.run_text(&["scan"]);
    assert_eq!(o.code, 3, "{}", o.all());
    assert!(o.all().contains("Vault is empty"), "{}", o.all());
}

#[test]
fn the_text_report_has_a_row_per_issue_and_a_summary_that_adds_up() {
    let v = Vault::new("scan-text-rows");
    v.ok(&["entry", "add", "Weak", "--key", "password123"]);
    v.ok(&["entry", "add", "Fine", "--key", STRONG]);
    let o = v.run_text(&["scan"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let json = issues(&v);
    assert!(has(&json, "Weak", "weak").is_some(), "{json:?}");
    assert!(
        o.stdout.contains("Sev") && o.stdout.contains("Issue"),
        "{}",
        o.stdout
    );
    let row = o
        .stdout
        .lines()
        .find(|l| l.contains("Weak"))
        .expect("a row for Weak");
    assert!(row.starts_with("HIGH"), "{row}");
    let (high, med, low) = ["high", "medium", "low"]
        .map(|k| v.ok(&["scan"])[k].as_u64().unwrap())
        .into();
    assert!(
        o.stdout.contains(&format!(
            "{} issue(s): {high} high, {med} medium, {low} low",
            json.len()
        )),
        "{}",
        o.stdout
    );
    assert!(
        !o.stdout.contains("password123"),
        "an issue names the entry, never the secret"
    );
}

#[test]
fn a_recovery_code_set_is_flagged_when_two_are_left_and_more_loudly_when_none_are() {
    let v = Vault::new("scan-text-codes");
    let codes = |used: usize| {
        (0..4)
            .map(|n| {
                if n < used {
                    format!("code-{n}\tUSED 2026-01-01")
                } else {
                    format!("code-{n}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    for (name, used) in [("Plenty", 1), ("Two", 2), ("None", 4)] {
        v.ok(&[
            "entry",
            "add",
            name,
            "--type",
            "recovery_codes",
            "--key",
            STRONG,
            "--var",
            &format!("codes={}", codes(used)),
        ]);
    }
    let list = issues(&v);
    assert!(
        has(&list, "Plenty", "recovery codes left").is_none(),
        "{list:?}"
    );
    let two = has(&list, "Two", "Only 2 of 4 recovery codes left").expect("flagged at two");
    assert_eq!(two["severity"], "med");
    let none = has(&list, "None", "Only 0 of 4 recovery codes left").expect("flagged at none");
    assert_eq!(none["severity"], "high");
    assert!(
        !format!("{list:?}").contains("code-0"),
        "codes are never printed"
    );
}

#[test]
fn a_wallet_phrase_that_fails_its_checksum_is_high_and_is_never_printed() {
    let v = Vault::new("scan-text-wallet");
    let good = format!("{} about", ["abandon"; 11].join(" "));
    let bad = ["abandon"; 12].join(" ");
    v.ok(&[
        "entry",
        "add",
        "Good",
        "--type",
        "crypto_wallet",
        "--key",
        &good,
    ]);
    v.ok(&[
        "entry",
        "add",
        "Bad",
        "--type",
        "crypto_wallet",
        "--key",
        &bad,
    ]);
    let o = v.run(&["scan"]);
    assert!(
        !o.all().contains("abandon"),
        "the phrase must not reach the report: {}",
        o.all()
    );
    let list = issues(&v);
    let issue = has(&list, "Bad", "Recovery phrase does not validate").expect("flagged");
    assert_eq!(issue["severity"], "high");
    assert!(has(&list, "Good", "Recovery phrase").is_none(), "{list:?}");
}

#[test]
fn a_browser_session_is_flagged_until_verified_and_is_not_nagged_about_rotation() {
    let v = Vault::new("scan-text-session");
    v.ok(&[
        "entry", "add", "Site", "--type", "cookie", "--key", "sid=abc",
    ]);
    let before = issues(&v);
    assert!(
        has(&before, "Site", "Session never verified").is_some(),
        "{before:?}"
    );
    assert!(
        has(&before, "Site", "Never rotated").is_none(),
        "rotating a session means nothing: {before:?}"
    );
    v.ok(&["entry", "verify", "Site"]);
    assert!(has(&issues(&v), "Site", "Session never verified").is_none());
}

#[test]
fn recording_a_rotation_clears_never_rotated_and_a_compromise_is_critical_until_then() {
    let v = Vault::new("scan-text-rotate");
    v.ok(&["entry", "add", "Key", "--key", STRONG]);
    assert!(has(&issues(&v), "Key", "Never rotated").is_some());
    v.ok(&["entry", "compromise", "Key"]);
    let flagged = issues(&v);
    let leaked = flagged
        .iter()
        .find(|i| i["subject"] == "Key" && i["severity"] == "high")
        .unwrap_or_else(|| panic!("a known-leaked entry is a high issue: {flagged:?}"));
    assert!(
        leaked["message"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("leak")
            || leaked["message"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("compromis"),
        "{leaked}"
    );
    v.ok(&["entry", "rotate", "Key"]);
    let after = issues(&v);
    assert!(has(&after, "Key", "Never rotated").is_none(), "{after:?}");
    assert!(
        !after
            .iter()
            .any(|i| i["subject"] == "Key" && i["severity"] == "high"),
        "rotating is the answer to a leak and clears the flag: {after:?}"
    );
}

#[test]
fn status_says_where_it_points_what_it_holds_and_warns_about_the_salt() {
    let v = Vault::new("scan-text-status");
    v.ok(&["project", "add", "Web"]);
    v.ok(&["entry", "add", "A", "--key", STRONG]);
    v.ok(&["entry", "add", "B", "--key", STRONG, "--type", "password"]);
    let o = v.run_text(&["status"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let out = &o.stdout;
    assert!(out.contains("Mode        local"), "{out}");
    assert!(out.contains("Entries     2"), "{out}");
    assert!(out.contains("Chunks      0"), "{out}");
    assert!(out.contains("Salt") && out.contains("vault.salt"), "{out}");
    assert!(
        out.contains("A vault.db without its vault.salt cannot be opened"),
        "{out}"
    );
    assert!(out.contains("By type:"), "{out}");
    assert!(!out.contains(STRONG));
    let json = v.ok(&["status"]);
    assert_eq!(json["mode"], "local");
    assert_eq!(json["entries"], 2);
    assert_eq!(json["salt_present"], true);
    assert_eq!(json["redacting"], true);

    std::fs::remove_file(v.path("vault.salt")).unwrap();
    // Without its salt the database cannot be opened, and nothing may quietly make a new
    // salt (which would make the real one unrecoverable for good).
    let missing = v.run_text(&["status"]);
    assert_ne!(missing.code, 0, "{}", missing.all());
    assert!(
        missing.all().contains("vault.salt is missing")
            && missing.all().contains("restore-archive"),
        "{}",
        missing.all()
    );
    assert!(
        !v.path("vault.salt").exists(),
        "a missing salt must not be regenerated"
    );
}
