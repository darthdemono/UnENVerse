// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Restore drills ("Backups", README): a drill restores the latest completed
//! package into a scratch location and verifies it, without activating it.
//!
//! * with nothing to restore a drill fails and says so;
//! * after a package a drill succeeds and the live vault is exactly as it was;
//! * an unknown destination is not found.

mod common;
use common::Vault;

fn with_package(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "Seed", "--key", "seed-secret-123456"]);
    let dest = v.path("dest");
    std::fs::create_dir_all(&dest).unwrap();
    v.ok(&[
        "backup",
        "dest",
        "add",
        "local",
        "main",
        dest.to_str().unwrap(),
    ]);
    v.ok(&["backup", "recovery-key"]);
    v
}

#[test]
fn a_drill_with_no_package_fails_instead_of_passing_vacuously() {
    let v = with_package("drillnone");
    let o = v.run(&["backup", "drill", "--dest", "main"]);
    // Bug it pins: a drill that verified nothing printed `ok: true` and exited 0.
    assert_eq!(
        o.code,
        7,
        "nothing was restored, so nothing was verified: {}",
        o.all()
    );
    let doc = o.json();
    assert_eq!(doc["ok"], false, "{doc}");
    assert_eq!(
        doc["error"]["details"]["run"]["state"], "failed",
        "one document: the run rides in the error: {doc}"
    );
    assert!(
        doc["error"]["message"]
            .as_str()
            .unwrap()
            .contains("No verified recovery package"),
        "{doc}"
    );
    let ran = v.run(&["ops", "run", "--dest", "main"]);
    assert_eq!(
        ran.code,
        0,
        "a package run with a key and data succeeds: {}",
        ran.all()
    );
}

#[test]
fn a_drill_after_a_package_succeeds_and_leaves_the_live_vault_alone() {
    let v = with_package("drillok");
    v.ok(&["backup", "package", "--dest", "main"]);

    let o = v.run(&["backup", "drill", "--dest", "main"]);
    assert_eq!(o.code, 0, "{}", o.all());

    assert_eq!(v.ok(&["list"])["count"], 1);
}

#[test]
fn a_drill_of_an_unknown_destination_is_not_found() {
    let v = with_package("drillmissing");
    let o = v.run(&["backup", "drill", "--dest", "nope"]);
    assert_eq!(o.code, 3, "{}", o.all());
    assert_eq!(v.run(&["ops", "run", "--dest", "nope"]).code, 3);
}
