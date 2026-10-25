// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The rest of `unv doctor`: the checks about data that lives outside the vault
//! file or that an older build could have left behind, and the one repair it offers.
//!
//! * pool state (a machine-local file) is checked for readability and for members
//!   whose entry has since been deleted, and the remedy it prints actually clears it;
//! * an entry with no stable id is a warning, `--fix` backfills it, says what it did, and
//!   leaves a sound vault alone;
//! * the human report is one line per check with its remedy beneath, and a loose pool
//!   file is named with the chmod that fixes it.

mod common;

use common::Vault;
use serde_json::{json, Value};

fn finding(o: &common::Out, check: &str) -> Value {
    let j = o.json();
    j["data"]["findings"]
        .as_array()
        .or_else(|| j["error"]["details"]["findings"].as_array())
        .and_then(|f| f.iter().find(|f| f["check"] == check).cloned())
        .unwrap_or_else(|| panic!("no '{check}' finding in {}", o.stdout))
}

#[cfg(unix)]
fn pools_file(v: &Vault) -> std::path::PathBuf {
    v.path("state").join("unv").join("pools.json")
}

#[cfg(unix)]
fn pooled() -> (Vault, std::path::PathBuf) {
    let v = Vault::new("doctor-pools");
    for k in ["K1", "K2"] {
        v.ok(&[
            "entry",
            "add",
            k,
            "--key",
            &format!("secret-{k}-123456"),
            "--pool",
            "main",
        ]);
    }
    // Two turns, so both members have recorded state on this machine.
    for _ in 0..2 {
        let o = v.run_with(&["--reveal"], &["pool", "next", "main"]);
        assert_eq!(o.code, 0, "{}", o.all());
    }
    let file = pools_file(&v);
    assert!(file.exists(), "using a pool writes its machine-local state");
    (v, file)
}

#[cfg(unix)]
#[test]
fn a_pool_whose_member_was_deleted_is_reported_and_its_own_remedy_clears_it() {
    let (v, _) = pooled();
    assert_eq!(finding(&v.run(&["doctor"]), "pools")["level"], "ok");

    v.ok(&["entry", "rm", "K2"]);
    let o = v.run(&["doctor"]);
    assert_eq!(o.code, 0, "a warning is not a failure: {}", o.all());
    let f = finding(&o, "pools");
    assert_eq!(f["level"], "warn", "{f}");
    assert!(
        f["message"].as_str().unwrap().contains("no longer exist"),
        "{f}"
    );
    assert!(f["message"].as_str().unwrap().contains("main"), "{f}");
    assert!(
        f["remedy"].as_str().unwrap().contains("unv pool reset"),
        "{f}"
    );

    v.ok(&["pool", "reset", "main"]);
    assert_eq!(
        finding(&v.run(&["doctor"]), "pools")["level"],
        "ok",
        "following the remedy must make the warning go away"
    );
}

#[cfg(unix)]
#[test]
fn an_unreadable_pool_file_is_a_warning_that_says_nothing_in_it_is_secret() {
    let (v, file) = pooled();
    std::fs::write(&file, "{ torn json").unwrap();
    let f = finding(&v.run(&["doctor"]), "pools");
    assert_eq!(f["level"], "warn", "{f}");
    assert!(f["message"].as_str().unwrap().contains("not valid JSON"));
    assert!(f["remedy"]
        .as_str()
        .unwrap()
        .contains("nothing in it is a secret"));
    // And deleting it, as the remedy says, is enough.
    std::fs::remove_file(&file).unwrap();
    assert_eq!(finding(&v.run(&["doctor"]), "pools")["level"], "ok");
}

#[cfg(unix)]
#[test]
fn a_pool_file_readable_by_others_is_named_with_the_chmod_that_fixes_it() {
    use std::os::unix::fs::PermissionsExt;
    let (v, file) = pooled();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let f = finding(&v.run(&["doctor"]), "permissions");
    assert_eq!(f["level"], "warn", "{f}");
    assert!(f["message"].as_str().unwrap().contains("644"), "{f}");
    assert!(
        f["remedy"].as_str().unwrap().starts_with("chmod 600 "),
        "{f}"
    );
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(finding(&v.run(&["doctor"]), "permissions")["level"], "ok");
}

/// A vault document as an older build could have left it: one entry with no id. (A bundle
/// member whose bundle is gone is also repairable, but saving a vault already drops such a
/// member's bundle fields, so the CLI cannot be made to hold one.)
fn damaged_vault(tag: &str) -> Vault {
    let source = Vault::new(&format!("{tag}-src"));
    let doc = json!({
        "api_keys": [
            { "provider": "NoId", "api_key": "secret-noid-123456", "secretType": "api_key",
              "price_type": "free", "categories": [], "projectIds": ["Universal"], "scopes": [] },
            { "id": "e2", "provider": "Orphan", "api_key": "secret-orphan-123456", "secretType": "api_key",
              "price_type": "free", "categories": [], "projectIds": ["Universal"], "scopes": [] },
            { "id": "e3", "provider": "Fine", "api_key": "secret-fine-123456", "secretType": "api_key",
              "price_type": "free", "categories": [], "projectIds": ["Universal"], "scopes": [] }
        ],
        "user_categories": [],
        "projects": [{ "id": "Universal", "name": "Universal" }]
    });
    let file = source.path("damaged.json");
    std::fs::write(&file, doc.to_string()).unwrap();
    let v = Vault::new(tag);
    let o = v.run_text(&["import", file.to_str().unwrap(), "--json"]);
    assert_eq!(o.code, 0, "{}", o.all());
    v
}

#[test]
fn an_entry_without_an_id_is_warned_about_with_the_repair_named() {
    let v = damaged_vault("doctor-damaged");
    let o = v.run(&["doctor"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let ids = finding(&o, "entry-ids");
    assert_eq!(ids["level"], "warn", "{ids}");
    assert!(
        ids["message"]
            .as_str()
            .unwrap()
            .contains("1 entry has no stable id"),
        "{ids}"
    );
    assert!(ids["remedy"].as_str().unwrap().contains("doctor --fix"));
    assert_eq!(finding(&o, "bundle-membership")["level"], "ok");
}

#[test]
fn doctor_does_not_repair_unless_asked_and_fix_backfills_the_missing_id_and_nothing_else() {
    let v = damaged_vault("doctor-fix");
    v.run(&["doctor"]);
    assert_eq!(
        finding(&v.run(&["doctor"]), "entry-ids")["level"],
        "warn",
        "plain doctor is read-only"
    );

    let o = v.run(&["doctor", "--fix"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let fix = finding(&o, "fix");
    assert_eq!(fix["level"], "ok");
    assert!(
        fix["message"]
            .as_str()
            .unwrap()
            .contains("backfilled 1 id(s), cleared 0 dangling bundle membership(s)"),
        "{fix}"
    );
    assert_eq!(
        finding(&o, "entry-ids")["level"],
        "ok",
        "the report describes the repaired vault"
    );
    assert_eq!(finding(&o, "bundle-membership")["level"], "ok");

    let orphan = v.run_with(&["--reveal"], &["get", "Orphan"]).json()["data"]["entries"][0].clone();
    assert_eq!(
        orphan["api_key"], "secret-orphan-123456",
        "other entries are untouched"
    );
    let noid = v.run_with(&["--reveal"], &["get", "NoId"]).json()["data"]["entries"][0].clone();
    assert!(!noid["id"].as_str().unwrap_or("").is_empty(), "{noid}");
    assert_eq!(
        v.run_with(&["--reveal"], &["get", "Fine"]).json()["data"]["entries"][0]["id"],
        "e3"
    );

    let again = v.run(&["doctor", "--fix"]);
    assert!(
        finding(&again, "fix")["message"]
            .as_str()
            .unwrap()
            .contains("nothing to repair"),
        "a second run has nothing to do"
    );
}

#[test]
fn the_human_report_is_one_line_per_check_with_the_remedy_beneath_a_warning() {
    let v = damaged_vault("doctor-text");
    let o = v.run_text(&["doctor"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let line = o
        .stdout
        .lines()
        .find(|l| l.contains("entry-ids"))
        .expect("an entry-ids line");
    assert!(line.starts_with("[ warn ]"), "{line}");
    let at = o
        .stdout
        .lines()
        .position(|l| l.contains("entry-ids"))
        .unwrap();
    let next = o.stdout.lines().nth(at + 1).unwrap();
    assert!(
        next.trim_start().starts_with("→"),
        "the remedy sits under its warning: {next}"
    );
    assert!(next.contains("doctor --fix"));
    assert!(
        o.stdout
            .lines()
            .any(|l| l.starts_with("[  ok  ] integrity")),
        "{}",
        o.stdout
    );
}
