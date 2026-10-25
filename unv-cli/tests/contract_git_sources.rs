// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Git-store migration ("SOPS, pass and gopass"): `unv import-vault sops|pass|gopass` reads the
//! user's own tools. The tools here are stub scripts on a private PATH, so no GPG key, no age key
//! and no network is needed, but the whole path is real: the `unv` binary, its tool adapter, the
//! planner, the durable migration record and the encrypted source archive.
#![cfg(unix)]

mod common;
use common::Vault;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SECRET: &str = "pw-from-the-store-0123456789";

fn stub(dir: &Path, name: &str, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A PATH holding only the stubs plus the system directories the shell scripts need.
fn path_with(dir: &Path) -> String {
    format!("{}:/usr/bin:/bin", dir.display())
}

fn names(v: &Vault) -> Vec<String> {
    let o = v.run(&["list"]);
    if o.code != 0 {
        return Vec::new();
    }
    let mut n: Vec<String> = o.json()["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["provider"].as_str().unwrap().to_string())
        .collect();
    n.sort();
    n
}

fn pass_store(v: &Vault) -> (PathBuf, PathBuf) {
    let store = v.path("store");
    std::fs::create_dir_all(store.join("email")).unwrap();
    std::fs::write(store.join(".gpg-id"), "me@example.test\n").unwrap();
    std::fs::write(store.join("email/gmail.gpg"), "ciphertext").unwrap();
    let bin = v.path("bin");
    stub(
        &bin,
        "gpg",
        &format!("if [ \"$1\" = \"--version\" ]; then echo 'gpg (GnuPG) 2.4.5'; exit 0; fi\nprintf '{SECRET}\\nuser: me\\n'"),
    );
    (store, bin)
}

#[test]
fn a_pass_preview_writes_nothing_and_shows_no_value_and_apply_imports_through_gpg() {
    let v = Vault::new("passapply");
    let (store, bin) = pass_store(&v);
    let path = path_with(&bin);
    let env = [("PATH", path.as_str())];
    let o = v.run_env(&env, &["import-vault", "pass", store.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["counts"]["imported"], 1, "{}", o.stdout);
    assert!(names(&v).is_empty(), "nothing is written without --apply");
    assert!(!o.all().contains(SECRET), "{}", o.all());

    let o = v.run_env(
        &env,
        &["import-vault", "pass", store.to_str().unwrap(), "--apply"],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(names(&v), vec!["email/gmail"]);
    assert_eq!(v.reveal_field("email/gmail", "api_key"), SECRET);
    // A rerun with an unchanged store changes nothing.
    let again = v.run_env(
        &env,
        &["import-vault", "pass", store.to_str().unwrap(), "--apply"],
    );
    assert_eq!(again.code, 0, "{}", again.all());
    assert_eq!(
        again.json()["data"]["counts"]["imported"],
        0,
        "{}",
        again.stdout
    );
    // The store itself is untouched.
    assert_eq!(
        std::fs::read_to_string(store.join("email/gmail.gpg")).unwrap(),
        "ciphertext"
    );
}

#[test]
fn a_missing_tool_is_a_clear_error_and_leaves_the_vault_empty() {
    let v = Vault::new("passmissing");
    let (store, _bin) = pass_store(&v);
    let empty = v.path("emptybin");
    std::fs::create_dir_all(&empty).unwrap();
    let path = format!("{}", empty.display());
    let o = v.run_env(
        &[("PATH", path.as_str())],
        &["import-vault", "pass", store.to_str().unwrap(), "--apply"],
    );
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(o.all().contains("not installed"), "{}", o.all());
    assert!(names(&v).is_empty());
}

#[test]
fn sops_decrypts_a_file_through_the_users_sops_and_keeps_the_ciphertext_as_provenance() {
    let v = Vault::new("sops");
    let dir = v.path("repo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("app.yaml"),
        "db:\n  password: ENC[AES256_GCM,data:x]\nsops:\n  version: 3.9.0\n",
    )
    .unwrap();
    let bin = v.path("bin");
    stub(
        &bin,
        "sops",
        &format!("if [ \"$1\" = \"--version\" ]; then echo 'sops 3.9.0'; exit 0; fi\nprintf '{{\"db\":{{\"password\":\"{SECRET}\"}}}}'"),
    );
    let path = path_with(&bin);
    let env = [("PATH", path.as_str())];
    let o = v.run_env(
        &env,
        &["import-vault", "sops", dir.to_str().unwrap(), "--apply"],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(names(&v), vec!["app.yaml"]);
    assert_eq!(v.reveal_field("app.yaml", "db.password"), SECRET);
    let ls = v.ok(&["migrate", "archive", "ls"]);
    assert!(
        ls["archives"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["reason"].as_str().unwrap().contains("non-operational")),
        "{ls}"
    );
}

#[test]
fn gopass_lists_and_reads_through_the_users_gopass() {
    let v = Vault::new("gopass");
    let bin = v.path("bin");
    stub(
        &bin,
        "gopass",
        &format!(
            "case \"$1\" in --version) echo 'gopass 1.15.15';; ls) echo web/github;; mounts) echo 'gopass (/s)';; show) printf '{SECRET}\\nuser: octo\\n';; esac"
        ),
    );
    let path = path_with(&bin);
    let env = [("PATH", path.as_str())];
    let o = v.run_env(&env, &["import-vault", "gopass", ".", "--apply"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(names(&v), vec!["web/github"]);
    assert_eq!(v.reveal_field("web/github", "api_key"), SECRET);
}

/// Imports the pass store, returning the migration id.
fn imported_pass(v: &Vault, store: &Path, path: &str) -> String {
    let env = [("PATH", path)];
    let o = v.run_env(
        &env,
        &["import-vault", "pass", store.to_str().unwrap(), "--apply"],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    let report = v.ok(&["migrate", "report"]);
    report["migrations"][0]["id"].as_str().unwrap().to_string()
}

#[test]
fn a_source_owned_secret_cannot_be_edited_in_the_vault_but_a_note_can() {
    let v = Vault::new("srcown");
    let (store, bin) = pass_store(&v);
    let path = path_with(&bin);
    imported_pass(&v, &store, &path);

    let refused = v.run(&[
        "entry",
        "set",
        "email/gmail",
        "--key",
        "changed-secret-value",
    ]);
    assert_eq!(refused.code, 6, "{}", refused.all());
    assert!(
        refused.all().contains("Managed by its source"),
        "{}",
        refused.all()
    );
    assert!(refused.all().contains("pass"), "{}", refused.all());
    assert_eq!(
        v.reveal_field("email/gmail", "api_key"),
        SECRET,
        "nothing was written"
    );

    let ok = v.run(&[
        "entry",
        "set",
        "email/gmail",
        "--url",
        "https://mail.example.test",
    ]);
    assert_eq!(ok.code, 0, "{}", ok.all());
}

#[test]
fn drift_reports_what_the_source_changed_without_importing_it() {
    let v = Vault::new("drift");
    let (store, bin) = pass_store(&v);
    let path = path_with(&bin);
    let id = imported_pass(&v, &store, &path);
    let env = [("PATH", path.as_str())];

    let same = v.run_env(&env, &["migrate", "drift", &id]);
    assert_eq!(same.code, 0, "{}", same.all());
    assert_eq!(same.json()["data"]["unchanged"], 1, "{}", same.stdout);

    // The store changes: a new password for the same file, and a new file.
    stub(
        &bin,
        "gpg",
        "if [ \"$1\" = \"--version\" ]; then echo 'gpg (GnuPG) 2.4.5'; exit 0; fi\nprintf 'a-different-password-9876\\nuser: me\\n'",
    );
    std::fs::write(store.join("new.gpg"), "ct").unwrap();
    let moved = v.run_env(&env, &["migrate", "drift", &id]);
    assert_eq!(moved.code, 0, "{}", moved.all());
    let data = moved.json()["data"].clone();
    assert_eq!(
        data["source_drift"],
        serde_json::json!(["email/gmail.gpg"]),
        "{data}"
    );
    assert_eq!(
        data["new_at_source"],
        serde_json::json!(["new.gpg"]),
        "{data}"
    );
    assert_eq!(
        v.reveal_field("email/gmail", "api_key"),
        SECRET,
        "drift imports nothing"
    );
    assert!(
        !moved.all().contains("a-different-password"),
        "{}",
        moved.all()
    );
}

#[test]
fn sync_refreshes_the_vault_one_way_and_a_removed_file_is_reported_missing() {
    let v = Vault::new("sync");
    let (store, bin) = pass_store(&v);
    let path = path_with(&bin);
    let id = imported_pass(&v, &store, &path);
    let env = [("PATH", path.as_str())];
    stub(
        &bin,
        "gpg",
        "if [ \"$1\" = \"--version\" ]; then echo 'gpg (GnuPG) 2.4.5'; exit 0; fi\nprintf 'a-different-password-9876\\n'",
    );
    let preview = v.run_env(&env, &["migrate", "sync", &id]);
    assert_eq!(preview.code, 0, "{}", preview.all());
    assert_eq!(
        v.reveal_field("email/gmail", "api_key"),
        SECRET,
        "preview writes nothing"
    );
    let applied = v.run_env(&env, &["migrate", "sync", &id, "--apply"]);
    assert_eq!(applied.code, 0, "{}", applied.all());
    assert_eq!(
        v.reveal_field("email/gmail", "api_key"),
        "a-different-password-9876"
    );

    std::fs::remove_file(store.join("email/gmail.gpg")).unwrap();
    std::fs::write(store.join("other.gpg"), "ct").unwrap();
    let gone = v.run_env(&env, &["migrate", "drift", &id]);
    assert_eq!(
        gone.json()["data"]["missing_at_source"],
        serde_json::json!(["email/gmail.gpg"]),
        "{}",
        gone.stdout
    );
}

#[test]
fn cutover_makes_the_vault_authoritative_and_uncutover_is_refused_after_a_credential_changes() {
    let v = Vault::new("cutover");
    let (store, bin) = pass_store(&v);
    let path = path_with(&bin);
    let id = imported_pass(&v, &store, &path);

    let cut = v.run(&["migrate", "cutover", &id]);
    assert_eq!(cut.code, 0, "{}", cut.all());
    assert_eq!(
        cut.json()["data"]["entries"],
        serde_json::json!(["email/gmail"])
    );
    // The vault may now change the secret.
    let edit = v.run(&[
        "entry",
        "set",
        "email/gmail",
        "--key",
        "rotated-after-cutover-1",
    ]);
    assert_eq!(edit.code, 0, "{}", edit.all());

    let back = v.run(&["migrate", "uncutover", &id]);
    assert_eq!(back.code, 6, "{}", back.all());
    assert!(back.all().contains("Not reversible"), "{}", back.all());
    assert!(back.all().contains("email/gmail"), "{}", back.all());
    assert_eq!(
        v.reveal_field("email/gmail", "api_key"),
        "rotated-after-cutover-1"
    );
    // A second cutover has nothing left to take over.
    assert_ne!(v.run(&["migrate", "cutover", &id]).code, 0);
}

#[test]
fn uncutover_hands_authority_back_while_nothing_changed() {
    let v = Vault::new("uncutover");
    let (store, bin) = pass_store(&v);
    let path = path_with(&bin);
    let id = imported_pass(&v, &store, &path);
    assert_eq!(v.run(&["migrate", "cutover", &id]).code, 0);
    let back = v.run(&["migrate", "uncutover", &id]);
    assert_eq!(back.code, 0, "{}", back.all());
    // Owned by the source again: a secret edit is refused.
    let refused = v.run(&[
        "entry",
        "set",
        "email/gmail",
        "--key",
        "changed-secret-value",
    ]);
    assert_eq!(refused.code, 6, "{}", refused.all());
}

fn have(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The same import with a real `gpg`: a throwaway key in a temporary GNUPGHOME (no passphrase, so
/// no pinentry), a real encrypted `.gpg` file in a pass-shaped store, and the real `unv` reading it.
/// The stub tests above prove the plumbing; this proves the plumbing meets a real tool.
#[test]
fn a_real_gpg_decrypts_a_pass_store_through_unv() {
    if !have("gpg") {
        eprintln!("gpg is not installed; skipping the real-tool test");
        return;
    }
    let v = Vault::new("realgpg");
    // GnuPG sockets live under GNUPGHOME and the path is limited to about 100 bytes.
    let home = std::path::PathBuf::from(format!("/tmp/unvg-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    let gpg = |args: &[&str]| {
        let o = std::process::Command::new("gpg")
            .env("GNUPGHOME", &home)
            .args(["--batch", "--yes", "--pinentry-mode", "loopback"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "gpg {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        o
    };
    gpg(&[
        "--passphrase",
        "",
        "--quick-generate-key",
        "unv test <unv-test@example.test>",
        "ed25519",
        "sign",
        "never",
    ]);
    let listing = gpg(&["--list-keys", "--with-colons", "unv-test@example.test"]);
    let fpr = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .find(|l| l.starts_with("fpr:"))
        .and_then(|l| l.split(':').nth(9))
        .unwrap()
        .to_string();
    gpg(&[
        "--passphrase",
        "",
        "--quick-add-key",
        &fpr,
        "cv25519",
        "encr",
        "never",
    ]);

    let store = v.path("store");
    std::fs::create_dir_all(store.join("work")).unwrap();
    std::fs::write(store.join(".gpg-id"), "unv-test@example.test\n").unwrap();
    let plain = v.path("plain.txt");
    std::fs::write(
        &plain,
        format!("{SECRET}\nuser: alice\nurl: https://work.example.test\n"),
    )
    .unwrap();
    gpg(&[
        "--trust-model",
        "always",
        "-r",
        "unv-test@example.test",
        "-e",
        "-o",
        store.join("work/vpn.gpg").to_str().unwrap(),
        plain.to_str().unwrap(),
    ]);

    let home_str = home.to_string_lossy().into_owned();
    let env = [("GNUPGHOME", home_str.as_str())];
    let o = v.run_env(
        &env,
        &["import-vault", "pass", store.to_str().unwrap(), "--apply"],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(names(&v), vec!["work/vpn"]);
    assert_eq!(v.reveal_field("work/vpn", "api_key"), SECRET);
    let shown = v.ok(&["get", "work/vpn"])["entries"][0].clone();
    assert_eq!(shown["username"], "alice", "{shown}");
    let _ = std::fs::remove_dir_all(&home);
}
