// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Checking the vault: `scan`, `rotate-check`, `diff`, `audit`, `status`
//! (README "Commands", "Tools", "The security model"):
//!
//! * `scan` reports weak, expired, duplicated and never-rotated secrets, ranked
//!   by severity, and never prints a value;
//! * `rotate-check` lists what expires inside the window, and only that;
//! * `diff` answers "same or different" with fingerprints, without reading either;
//! * the audit log gets a row for each add, change of a secret and delete, none
//!   for a read, and its hash chain notices an edited row.

mod common;
use common::Vault;
use serde_json::Value;

const STRONG: &str = "a-long-enough-secret-value-1";

fn issues(v: &Vault, args: &[&str]) -> Vec<Value> {
    let mut a = vec!["scan"];
    a.extend_from_slice(args);
    v.ok(&a)["issues"].as_array().cloned().unwrap_or_default()
}

fn find<'a>(issues: &'a [Value], subject: &str, needle: &str) -> Option<&'a Value> {
    issues.iter().find(|i| {
        i["subject"].as_str().unwrap_or("").contains(subject)
            && i["message"].as_str().unwrap_or("").contains(needle)
    })
}

/// `YYYY-MM-DD` for today plus `days`, from the system clock (no date crate here).
fn date_in(days: i64) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let z = (secs + days * 86_400).div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[test]
fn scan_ranks_weak_expired_duplicate_and_never_rotated_secrets() {
    let v = Vault::new("scan");
    v.ok(&["entry", "add", "Weak", "--key", "short"]);
    v.ok(&[
        "entry",
        "add",
        "Old",
        "--key",
        "old-secret-123456789",
        "--expires",
        "2020-01-01",
    ]);
    v.ok(&["entry", "add", "One", "--key", STRONG]);
    v.ok(&["entry", "add", "Two", "--key", STRONG]);
    let found = issues(&v, &[]);

    assert_eq!(find(&found, "Weak", "weak").unwrap()["severity"], "high");
    assert_eq!(find(&found, "Old", "Expired").unwrap()["severity"], "high");
    let dup = find(&found, "One", "Same secret value").expect("two entries sharing a value");
    assert_eq!(dup["severity"], "med");
    assert!(
        dup["subject"].as_str().unwrap().contains("Two"),
        "both names are reported: {dup}"
    );
    assert_eq!(
        find(&found, "One", "Never rotated").unwrap()["severity"],
        "low"
    );
}

#[test]
fn scan_severity_filters_and_a_clean_entry_is_not_reported_as_weak() {
    let v = Vault::new("scansev");
    v.ok(&["entry", "add", "Weak", "--key", "short"]);
    v.ok(&[
        "entry",
        "add",
        "Fine",
        "--key",
        STRONG,
        "--desc",
        "described",
        "--rotation-days",
        "90",
    ]);
    let high = issues(&v, &["--severity", "high"]);
    assert!(high.iter().all(|i| i["severity"] == "high"), "{high:?}");
    assert!(find(&high, "Weak", "weak").is_some());
    assert!(
        find(&issues(&v, &[]), "Fine", "weak").is_none(),
        "a long secret is not weak"
    );
}

#[test]
fn scan_never_prints_a_value() {
    let v = Vault::new("scansecret");
    v.ok(&["entry", "add", "Weak", "--key", "short-value-xyz"]);
    v.ok(&[
        "entry",
        "add",
        "Old",
        "--key",
        STRONG,
        "--expires",
        "2020-01-01",
    ]);
    let o = v.run(&["scan"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        !o.all().contains("short-value-xyz") && !o.all().contains(STRONG),
        "{}",
        o.all()
    );
}

#[test]
fn rotate_check_lists_what_expires_inside_the_window_and_only_that() {
    let v = Vault::new("rotatecheck");
    v.ok(&[
        "entry",
        "add",
        "Soon",
        "--key",
        STRONG,
        "--expires",
        &date_in(10),
    ]);
    v.ok(&[
        "entry",
        "add",
        "Later",
        "--key",
        STRONG,
        "--expires",
        &date_in(200),
    ]);
    v.ok(&[
        "entry",
        "add",
        "Gone",
        "--key",
        STRONG,
        "--expires",
        &date_in(-10),
    ]);
    v.ok(&["entry", "add", "Never", "--key", STRONG]);

    let names = |days: &str| -> Vec<String> {
        let d = v.ok(&["rotate-check", "--days", days]);
        let mut n: Vec<String> = d["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["provider"].as_str().unwrap().to_string())
            .collect();
        n.sort();
        n
    };
    assert_eq!(
        names("30"),
        vec!["Soon"],
        "an expired or distant or never-expiring entry is outside the window"
    );
    assert_eq!(names("5"), Vec::<String>::new());
    assert_eq!(names("365"), vec!["Later", "Soon"]);
    // Expired entries are not 'expiring': they are a scan finding instead.
    assert!(find(&issues(&v, &[]), "Gone", "Expired").is_some());
}

#[test]
fn diff_answers_same_or_different_with_fingerprints_and_no_values() {
    let v = Vault::new("diff");
    v.ok(&[
        "entry",
        "add",
        "A",
        "--key",
        STRONG,
        "--url",
        "https://one.example",
    ]);
    v.ok(&[
        "entry",
        "add",
        "B",
        "--key",
        STRONG,
        "--url",
        "https://two.example",
    ]);
    v.ok(&[
        "entry",
        "add",
        "C",
        "--key",
        "a-different-secret-value-2",
        "--url",
        "https://one.example",
    ]);

    let row = |d: &Value, field: &str| -> Value {
        d["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["field"] == field)
            .cloned()
            .unwrap_or(Value::Null)
    };
    let ab = v.ok(&["diff", "A", "B"]);
    assert_eq!(row(&ab, "api_url")["changed"], true, "{ab}");
    assert!(
        row(&ab, "api_key").is_null() || row(&ab, "api_key")["changed"] == false,
        "equal secrets must read as equal: {ab}"
    );
    let ac = v.run(&["diff", "A", "C"]);
    assert_eq!(ac.code, 0, "{}", ac.all());
    assert!(
        !ac.all().contains(STRONG) && !ac.all().contains("a-different-secret-value-2"),
        "{}",
        ac.all()
    );
    assert_eq!(
        row(&ac.json()["data"], "api_key")["changed"],
        true,
        "different secrets must read as different"
    );
}

#[test]
fn the_audit_log_has_a_row_for_a_write_and_none_for_a_read() {
    let v = Vault::new("audit");
    v.ok(&["entry", "add", "GitHub", "--key", STRONG]);
    let rows = |v: &Vault| {
        v.ok(&["audit", "--limit", "100"])["rows"]
            .as_array()
            .unwrap()
            .clone()
    };
    assert_eq!(rows(&v).len(), 1);

    // Reads change nothing: a chain that grows with reads cannot be pruned.
    v.ok(&["list"]);
    v.ok(&["get", "GitHub"]);
    v.run_with(&["--reveal"], &["get", "GitHub", "--field", "api_key"]);
    assert_eq!(
        rows(&v).len(),
        1,
        "a read must not append to the audit chain"
    );

    v.ok(&[
        "entry",
        "rotate",
        "GitHub",
        "--key",
        "a-second-secret-value-9",
    ]);
    v.ok(&["entry", "add", "Other", "--key", "another-secret-value-3"]);
    v.ok(&["entry", "rm", "Other"]);
    let actions: Vec<String> = rows(&v)
        .iter()
        .map(|r| r["action"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        actions,
        vec!["delete", "add", "update", "add"],
        "newest first: {actions:?}"
    );
    assert!(
        !serde_json::to_string(&rows(&v)).unwrap().contains(STRONG),
        "the audit log names entries, never values"
    );
    assert_eq!(
        v.ok(&["audit", "--limit", "2"])["rows"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "--limit"
    );
}

#[test]
fn the_audit_chain_verifies_and_notices_an_edited_row() {
    let v = Vault::new("auditverify");
    v.ok(&["entry", "add", "One", "--key", STRONG]);
    v.ok(&["entry", "add", "Two", "--key", "another-secret-value-3"]);
    v.ok(&["entry", "add", "Three", "--key", "third-secret-value-4"]);
    let ok = v.ok(&["audit", "--verify"]);
    assert_eq!(ok["ok"], true, "{ok}");
    assert_eq!(ok["checked"], 3);

    // Someone with the key edits history after the fact.
    let salt = vault_core::read_or_create_salt(&v.dir.join("vault.salt")).unwrap();
    let key = vault_core::derive_key(common::PASSWORD, &salt).unwrap();
    {
        let conn = vault_core::open_db(&v.db(), &key).unwrap();
        conn.execute(
            "UPDATE vault_audit SET entry_provider = 'Forged' WHERE id = 2",
            [],
        )
        .unwrap();
    }
    let bad = v.run(&["audit", "--verify"]);
    assert_ne!(
        bad.code,
        0,
        "an edited row must break the chain: {}",
        bad.all()
    );
    let doctor = v.run(&["doctor"]);
    assert_eq!(doctor.code, 10, "doctor reports it too: {}", doctor.all());
}

#[test]
fn status_says_where_the_cli_is_pointed_and_what_the_vault_holds() {
    let v = Vault::new("status");
    v.ok(&["project", "add", "Web"]);
    v.ok(&["entry", "add", "One", "--key", STRONG]);
    v.ok(&[
        "entry",
        "add",
        "Two",
        "--key",
        "p-secret-value-12345",
        "--type",
        "password",
        "--username",
        "u",
    ]);
    let s = v.ok(&["status"]);
    assert_eq!(s["mode"], "local");
    assert_eq!(s["entries"], 2);
    assert_eq!(s["by_type"]["api_key"], 1);
    assert_eq!(s["by_type"]["password"], 1);
    assert_eq!(s["salt_present"], true);
    assert_eq!(
        s["redacting"], true,
        "redaction is the default and status says so"
    );
    assert!(s["location"].as_str().unwrap().ends_with("vault.db"));
}
