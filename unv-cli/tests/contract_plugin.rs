// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
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

fn plugin_dir_with(v: &Vault, name: &str, manifest: &str) -> PathBuf {
    let d = v.path("plugins");
    std::fs::create_dir_all(d.join(name)).unwrap();
    std::fs::write(d.join(name).join("plugin.json"), manifest).unwrap();
    std::fs::write(d.join(name).join("run.sh"), "#!/bin/sh\n").unwrap();
    d
}

const GOOD: &str = r#"{"name":"demo","version":"2.0.0","protocol":"unv-plugin-rpc/1","exec":"run.sh","capabilities":["notifier"]}"#;

#[test]
fn the_default_folder_sits_beside_the_vault_and_the_listing_reads_in_words() {
    let v = Vault::new("plugin-default");
    // No --dir: `plugins` beside the database.
    let empty = v.run_text(&["plugin", "ls"]);
    assert_eq!(empty.code, 0, "{}", empty.all());
    assert!(empty.stdout.contains("No plugins in"), "{}", empty.stdout);
    plugin_dir_with(&v, "demo", GOOD);
    plugin_dir_with(&v, "broken", "{}");
    let text = v.run_text(&["plugin", "ls"]);
    assert!(
        text.stdout.contains("demo 2.0.0 (not installed)"),
        "{}",
        text.stdout
    );
    assert!(
        text.stdout.contains("problem:") && text.stdout.contains("broken"),
        "{}",
        text.stdout
    );
    assert_eq!(v.run(&["plugin", "install", "demo"]).code, 0);
    let after = v.run_text(&["plugin", "ls"]);
    assert!(
        after
            .stdout
            .contains("demo 2.0.0 [notifier] disabled  sha256:"),
        "{}",
        after.stdout
    );
    let json = v.run(&["plugin", "ls"]).json();
    assert_eq!(json["data"]["problems"].as_array().unwrap().len(), 1);
}

#[test]
fn install_says_what_it_did_and_what_it_could_not_find() {
    let v = Vault::new("plugin-install");
    plugin_dir_with(&v, "demo", GOOD);
    plugin_dir_with(&v, "broken", "{}");
    let dry = v.run_with(&["--dry-run"], &["plugin", "install", "demo"]);
    assert_eq!(dry.code, 0, "{}", dry.all());
    assert_eq!(dry.json()["data"]["dry_run"], true);
    assert!(v.run(&["plugin", "ls"]).json()["data"]["installed"]
        .as_array()
        .unwrap()
        .is_empty());
    let text = v.run_text(&["plugin", "install", "demo"]);
    assert!(
        text.stdout.contains("Installed demo 2.0.0 (disabled)")
            && text.stdout.contains("unv plugin enable demo"),
        "{}",
        text.stdout
    );
    let missing = v.run(&["plugin", "install", "nope"]);
    assert_eq!(missing.code, 3, "{}", missing.all());
    assert!(
        missing.all().contains("broken"),
        "the broken manifest is named as a hint: {}",
        missing.all()
    );
    let plain = Vault::new("plugin-install2");
    let none = plain.run(&["plugin", "install", "nope"]);
    assert_eq!(none.code, 3);
    assert!(
        !none.all().contains("("),
        "no hint when nothing is broken: {}",
        none.all()
    );
}

#[test]
fn enable_disable_and_remove_report_in_words_and_dry_run_changes_nothing() {
    let v = Vault::new("plugin-lifecycle");
    plugin_dir_with(&v, "demo", GOOD);
    v.ok(&["plugin", "install", "demo"]);
    let on = v.run_text(&["plugin", "enable", "demo"]);
    assert!(on.stdout.contains("Enabled demo"), "{}", on.stdout);
    let dry_off = v.run_with(&["--dry-run"], &["plugin", "disable", "demo"]);
    assert_eq!(dry_off.code, 0, "{}", dry_off.all());
    assert_eq!(
        v.run(&["plugin", "ls"]).json()["data"]["installed"][0]["enabled"],
        true
    );
    let off = v.run_text(&["plugin", "disable", "demo"]);
    assert!(off.stdout.contains("Disabled demo"), "{}", off.stdout);
    let dry_rm = v.run_with(&["--dry-run"], &["plugin", "rm", "demo"]);
    assert_eq!(dry_rm.json()["data"]["removed"], false);
    assert_eq!(
        v.run(&["plugin", "ls"]).json()["data"]["installed"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let rm = v.run_text(&["plugin", "rm", "demo"]);
    assert!(
        rm.stdout.contains("Removed (the files stay on disk)"),
        "{}",
        rm.stdout
    );
    let again = v.run_text(&["plugin", "rm", "demo"]);
    assert!(again.stdout.contains("Nothing removed"), "{}", again.stdout);
    assert!(plugin_dir_with(&v, "demo", GOOD)
        .join("demo/run.sh")
        .exists());
    assert_ne!(v.run(&["plugin", "disable", "ghost"]).code, 0);
}

#[test]
fn the_conformance_suite_fails_loudly_for_a_bad_plugin_and_a_bad_directory() {
    let v = Vault::new("plugin-conform");
    // A plugin that answers nothing sensible: every check fails and the exit code says so.
    let dir = v.path("silent");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plugin.json"), GOOD).unwrap();
    std::fs::write(dir.join("run.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let o = v.run_text(&["plugin", "test", dir.to_str().unwrap()]);
    assert_eq!(o.code, 6, "{}", o.all());
    assert!(
        o.all().contains("FAIL  handshake") && o.all().contains("check(s) failed"),
        "{}",
        o.all()
    );
    // No manifest at all, and an invalid one.
    let none = v.run(&["plugin", "test", v.path("absent").to_str().unwrap()]);
    assert_eq!(none.code, 3, "{}", none.all());
    let bad = v.path("bad");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("plugin.json"), r#"{"name":"Bad Name"}"#).unwrap();
    assert_eq!(v.run(&["plugin", "test", bad.to_str().unwrap()]).code, 10);
}

#[test]
fn the_conformance_report_in_words_lists_a_pass_per_line() {
    let v = Vault::new("plugin-words");
    let path = example().join("ntfy-notifier");
    let o = v.run_text(&["plugin", "test", path.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        o.stdout.contains("ok    handshake") && o.stdout.contains("ok    shutdown_within_deadline"),
        "{}",
        o.stdout
    );
}
