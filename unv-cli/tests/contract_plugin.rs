// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv plugin`: discovery, install without enabling, enabling after a warning, and the
//! conformance suite on the example plugin. Needs `python3`.
#![cfg(unix)]

mod common;
use common::Vault;
use std::path::PathBuf;

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/plugins")
}

#[test]
fn the_example_plugin_is_found_installed_disabled_and_enabled_only_with_consent() {
    let v = Vault::new("plugin");
    let dir = example();
    let dir = dir.to_str().unwrap();

    let ls = v.run(&["plugin", "ls", "--dir", dir]);
    assert_eq!(ls.code, 0, "{}", ls.all());
    assert_eq!(ls.json()["data"]["available"][0]["name"], "ntfy-notifier");
    assert!(ls.json()["data"]["installed"]
        .as_array()
        .unwrap()
        .is_empty());

    let i = v.run(&["plugin", "install", "ntfy-notifier", "--dir", dir]);
    assert_eq!(i.code, 0, "{}", i.all());
    assert_eq!(i.json()["data"]["plugin"]["enabled"], false);

    // Without --yes and without a terminal, enabling is refused: nobody agreed.
    let refused = v.run_no_yes(&["plugin", "enable", "ntfy-notifier"]);
    assert_ne!(refused.code, 0);
    assert!(refused.all().contains("--yes"), "{}", refused.all());

    let on = v.run(&["plugin", "enable", "ntfy-notifier"]);
    assert_eq!(on.code, 0, "{}", on.all());
    let ls = v.run(&["plugin", "ls", "--dir", dir]);
    assert_eq!(ls.json()["data"]["installed"][0]["enabled"], true);

    assert_eq!(v.run(&["plugin", "disable", "ntfy-notifier"]).code, 0);
    assert_eq!(v.run(&["plugin", "rm", "ntfy-notifier"]).code, 0);
    assert_ne!(v.run(&["plugin", "enable", "ntfy-notifier"]).code, 0);
}

#[test]
fn the_conformance_suite_runs_without_a_vault_and_passes_the_example() {
    let v = Vault::new("plugin-test");
    let path = example().join("ntfy-notifier");
    let o = v.run(&["plugin", "test", path.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["passed"], true);
    assert_eq!(o.json()["data"]["checks"].as_array().unwrap().len(), 6);
}
