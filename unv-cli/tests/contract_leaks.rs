// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! "Leak-scan import" (R17): `unv migrate leaks <dir>` runs the user's own gitleaks over a folder
//! and offers what it finds as migration candidates. A scan names files, lines and fingerprints and
//! never a value; nothing reaches the vault without `--apply`; `--template` keeps the file
//! rebuildable from the vault; a missing or too old gitleaks is a clear refusal.
//!
//! The report comes from a stub program with the real gitleaks JSON shape (checked against 8.30.1).
//! `real_gitleaks_finds_and_imports` runs the genuine binary and is skipped when none is installed.
#![cfg(unix)]

mod common;
use common::Vault;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Built at run time so the test source holds no credential-shaped literal.
fn secret() -> String {
    ["k3Jd9sLq2V", "x8mPz4Rt7W", "nYb1HcFa6U", "eG"].concat()
}

const CONFIG: &str = "client_secret";

fn project(v: &Vault) -> PathBuf {
    let dir = v.path("proj");
    std::fs::create_dir_all(dir.join("conf")).unwrap();
    std::fs::write(
        dir.join("conf/app.env"),
        format!("APP=demo\n# note\n{CONFIG} = \"{}\"\n", secret()),
    )
    .unwrap();
    std::fs::write(dir.join("clean.txt"), "nothing here\n").unwrap();
    dir
}

/// A gitleaks that answers `version` and, for `dir`, prints one finding for `conf/app.env`.
fn stub(v: &Vault, version: &str, finding_secret: &str) -> String {
    let path = v.path("gitleaks-stub");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" = version ]; then echo {version}; exit 0; fi
for last; do :; done
printf '[{{"RuleID":"generic-api-key","Description":"Detected a Generic API Key","StartLine":3,"EndLine":3,"StartColumn":2,"EndColumn":51,"Match":"x","Secret":"%s","File":"%s/conf/app.env","SymlinkFile":"","Commit":"","Fingerprint":"f"}}]' '{finding_secret}' "$last"
"#
    );
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().into_owned()
}

fn canonical(p: &Path) -> String {
    p.canonicalize().unwrap().to_string_lossy().into_owned()
}

fn entry_count(v: &Vault) -> usize {
    let o = v.run(&["list"]);
    if o.code != 0 {
        return 0;
    }
    o.json()["data"]["entries"].as_array().unwrap().len()
}

fn leaks(v: &Vault, tool: &str, dir: &Path, extra: &[&str]) -> common::Out {
    let d = dir.to_str().unwrap();
    let mut args = vec!["migrate", "leaks", d, "--gitleaks", tool];
    args.extend_from_slice(extra);
    v.run(&args)
}

#[test]
fn a_scan_lists_findings_without_values_and_writes_nothing() {
    let v = Vault::new("leaks-scan");
    let dir = project(&v);
    let tool = stub(&v, "v8.30.1", &secret());
    let o = leaks(&v, &tool, &dir, &[]);
    assert_eq!(o.code, 0, "{}", o.all());
    let j = o.json();
    let f = &j["data"]["findings"][0];
    assert_eq!(f["file"], "conf/app.env");
    assert_eq!(f["line"], 3);
    assert_eq!(f["rule"], "generic-api-key");
    assert_eq!(f["fingerprint"].as_str().unwrap().len(), 12);
    assert_eq!(j["data"]["applied"], false);
    assert!(
        !o.all().contains(&secret()),
        "a scan prints fingerprints, never the value"
    );
    assert_eq!(entry_count(&v), 0, "no entry without --apply");
    assert_eq!(
        v.ok(&["migrate", "report"])["migrations"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "and no migration record"
    );
}

#[test]
fn apply_saves_the_finding_and_a_repeat_changes_nothing() {
    let v = Vault::new("leaks-apply");
    let dir = project(&v);
    let tool = stub(&v, "v8.30.1", &secret());
    let o = leaks(&v, &tool, &dir, &["--apply"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(entry_count(&v), 1);
    assert!(!o.all().contains(&secret()));
    let provider = "generic-api-key conf/app.env line 3";
    assert_eq!(v.reveal_field(provider, "api_key"), secret());

    let again = leaks(&v, &tool, &dir, &["--apply"]);
    assert_eq!(again.code, 0, "{}", again.all());
    assert_eq!(again.json()["data"]["files"][0]["counts"]["skipped"], 1);
    assert_eq!(entry_count(&v), 1);
    assert_eq!(
        v.ok(&["migrate", "report"])["migrations"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "an unchanged source records nothing new"
    );
}

#[test]
fn template_mode_keeps_the_file_rebuildable_from_the_vault() {
    let v = Vault::new("leaks-template");
    let dir = project(&v);
    let tool = stub(&v, "v8.30.1", &secret());
    let o = leaks(&v, &tool, &dir, &["--apply", "--template"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let id = v.ok(&["migrate", "report"])["migrations"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let verified = v.ok(&["migrate", "verify", &id]);
    assert_eq!(verified["verified"], true, "{verified}");
    let on_disk = std::fs::read_to_string(dir.join("conf/app.env")).unwrap();
    assert!(
        on_disk.contains(&secret()),
        "the scanner's source file is only read, never rewritten"
    );
}

#[test]
fn a_secret_that_is_not_in_the_file_as_written_is_reported_not_guessed() {
    let v = Vault::new("leaks-decoded");
    let dir = project(&v);
    let tool = stub(&v, "v8.30.1", "decoded-form-of-the-value-x");
    let o = leaks(&v, &tool, &dir, &["--apply"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let file = &o.json()["data"]["files"][0];
    assert_eq!(file["counts"]["imported"], 0);
    assert!(
        file["unsupported"][0]
            .as_str()
            .unwrap()
            .contains("not in the file as written"),
        "{file}"
    );
    assert_eq!(entry_count(&v), 0);
}

#[test]
fn a_missing_gitleaks_is_a_plain_refusal() {
    let v = Vault::new("leaks-missing");
    let dir = project(&v);
    let o = leaks(&v, "/nonexistent/gitleaks", &dir, &[]);
    assert_eq!(o.error_code(), "not_found", "{}", o.all());
    assert_eq!(entry_count(&v), 0);
}

#[test]
fn a_gitleaks_older_than_the_checked_format_is_refused() {
    let v = Vault::new("leaks-old");
    let dir = project(&v);
    for old in ["v8.18.4", "v7.6.1"] {
        let tool = stub(&v, old, &secret());
        let o = leaks(&v, &tool, &dir, &["--apply"]);
        assert_eq!(o.error_code(), "invalid", "{old}: {}", o.all());
        assert!(o.all().contains("8.19"), "{}", o.all());
    }
    assert_eq!(entry_count(&v), 0);
}

#[test]
fn a_report_that_is_not_json_is_refused() {
    let v = Vault::new("leaks-garbage");
    let dir = project(&v);
    let path = v.path("gitleaks-garbage");
    std::fs::write(
        &path,
        "#!/bin/sh\nif [ \"$1\" = version ]; then echo v8.30.1; exit 0; fi\necho not-json\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let o = leaks(&v, path.to_str().unwrap(), &dir, &["--apply"]);
    assert_eq!(o.error_code(), "invalid", "{}", o.all());
    assert_eq!(entry_count(&v), 0);
}

#[test]
fn a_finding_reached_through_a_link_out_of_the_folder_is_refused() {
    let v = Vault::new("leaks-link");
    let dir = project(&v);
    let outside = v.path("outside.env");
    std::fs::write(&outside, format!("{CONFIG} = \"{}\"\n", secret())).unwrap();
    std::fs::remove_file(dir.join("conf/app.env")).unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("conf/app.env")).unwrap();
    let tool = stub(&v, "v8.30.1", &secret());
    let o = leaks(&v, &tool, &dir, &["--apply"]);
    assert_eq!(o.error_code(), "invalid", "{}", o.all());
    assert!(o.all().contains("outside the folder"), "{}", o.all());
    assert_eq!(entry_count(&v), 0);
}

#[test]
fn a_clean_folder_reports_nothing_found() {
    let v = Vault::new("leaks-clean");
    let dir = project(&v);
    let path = v.path("gitleaks-clean");
    std::fs::write(
        &path,
        "#!/bin/sh\nif [ \"$1\" = version ]; then echo v8.30.1; exit 0; fi\necho '[]'\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let o = leaks(&v, path.to_str().unwrap(), &dir, &["--apply"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["findings"], Value::Array(vec![]));
    assert_eq!(entry_count(&v), 0);
}

#[test]
fn real_gitleaks_finds_and_imports() {
    let Some(tool) = std::env::var("GITLEAKS_BIN")
        .ok()
        .or_else(|| which("gitleaks"))
    else {
        eprintln!("gitleaks is not installed; the stub tests above cover the contract");
        return;
    };
    let v = Vault::new("leaks-real");
    let dir = project(&v);
    let o = leaks(&v, &tool, &dir, &["--apply", "--template"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let f = &o.json()["data"]["findings"];
    assert_eq!(f[0]["file"], "conf/app.env", "{f}");
    assert_eq!(f[0]["line"], 3);
    assert_eq!(entry_count(&v), 1);
    assert_eq!(
        v.reveal_field("generic-api-key conf/app.env line 3", "api_key"),
        secret()
    );
    assert!(!o.all().contains(&canonical(&dir)), "paths stay relative");
}

fn which(name: &str) -> Option<String> {
    std::env::var("PATH")
        .ok()?
        .split(':')
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}
