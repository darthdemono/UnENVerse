// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv support bundle` and `unv demo-vault` (R25): the bundle may be sent to a stranger, so it
//! must hold no name, value, path or host; the demo vault must be usable and honest.

mod common;
use common::Vault;

const NAMES: [&str; 4] = [
    "Zq9DistinctProject",
    "Qx7DistinctEntry",
    "distinct-secret-value-4411",
    "node-host.distinct.example",
];

#[test]
fn the_bundle_holds_counts_and_versions_and_none_of_the_names_values_or_paths() {
    let v = Vault::new("support-bundle");
    v.ok(&["project", "add", NAMES[0], "--type", "nginx"]);
    v.ok(&[
        "entry",
        "add",
        NAMES[1],
        "--key",
        NAMES[2],
        "--url",
        &format!("https://{}", NAMES[3]),
    ]);
    let o = v.run(&["support", "bundle"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let text = o.all();
    for n in NAMES {
        assert!(!text.contains(n), "the bundle leaked {n}");
    }
    let dir = v.dir.display().to_string();
    assert!(!text.contains(&dir), "the bundle leaked a path");
    let b = &o.json()["data"]["bundle"];
    assert_eq!(b["entries"], 1);
    assert_eq!(b["project_types"]["nginx"], 1);
    assert_eq!(b["bundle_version"], 1);
    assert!(b["sqlcipher_version"].is_string());
    assert!(b["doctor"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["check"].is_string() && c["level"].is_string()));
}

#[test]
fn the_bundle_can_be_written_to_a_file_and_read_back() {
    let v = Vault::new("support-file");
    v.ok(&[
        "entry",
        "add",
        "Anything",
        "--key",
        "some-value-for-the-test",
    ]);
    let file = v.path("bundle.json");
    let o = v.run(&["support", "bundle", "--out", file.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(on_disk["entries"], 1);
}

#[test]
fn a_demo_vault_opens_with_its_printed_password_passes_doctor_and_refuses_to_overwrite() {
    let v = Vault::new("demo-vault");
    let dir = v.path("demo");
    let script = format!(
        "\"$UNV\" demo-vault --out '{d}' && UNV_PASSWORD=demo-vault-password-0000 \"$UNV\" --db-path '{d}/vault.db' --json list",
        d = dir.display()
    );
    let o = v.sh(&script);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(o.all().contains("Demo Service A"), "{}", o.all());

    let doctor = v.sh(&format!(
        "UNV_PASSWORD=demo-vault-password-0000 \"$UNV\" --db-path '{}/vault.db' --json doctor",
        dir.display()
    ));
    assert_eq!(doctor.code, 0, "{}", doctor.all());

    let again = v.sh(&format!("\"$UNV\" demo-vault --out '{}'", dir.display()));
    assert_ne!(again.code, 0, "an existing vault must not be overwritten");
}
