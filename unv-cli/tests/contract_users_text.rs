// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The human (non-JSON) side of user, class and token administration against a
//! local vault, as a person at a terminal sees it:
//!
//! * lists say so when empty and otherwise print a header and one row per subject,
//!   without ever showing a password hash;
//! * every mutation confirms in words what it did;
//! * a token is never printed unless asked for, and nothing is minted when it is
//!   refused;
//! * a name that matches nothing is "not found" (3), and strict write scoping can be
//!   switched on and off for a user.

mod common;

use common::Vault;

fn text_ok(v: &Vault, args: &[&str]) -> String {
    let o = v.run_text(args);
    assert_eq!(o.code, 0, "{args:?}: {}", o.all());
    o.stdout
}

#[test]
fn an_empty_user_list_says_so_and_a_filled_one_has_a_header_and_a_row_each() {
    let v = Vault::new("utext-ls");
    assert!(text_ok(&v, &["user", "ls"]).contains("No users."));

    text_ok(
        &v,
        &[
            "user",
            "add",
            "alice",
            "--user-password",
            "alice-pass-12345",
        ],
    );
    text_ok(&v, &["user", "add", "bob", "--no-password"]);
    let list = text_ok(&v, &["user", "ls"]);
    assert!(list.contains("Username"), "{list}");
    let alice = list.lines().find(|l| l.contains("alice")).unwrap();
    let bob = list.lines().find(|l| l.contains("bob")).unwrap();
    assert!(alice.contains("yes"), "alice has a password: {alice}");
    assert!(
        !bob.contains("yes"),
        "bob is token-only and not an owner: {bob}"
    );
    assert!(!list.contains("$argon2") && !list.contains("alice-pass-12345"));
}

#[test]
fn user_mutations_confirm_in_words_and_unknown_names_are_not_found() {
    let v = Vault::new("utext-mut");
    assert!(
        text_ok(&v, &["user", "add", "alice", "--no-password"]).contains("Created user 'alice'")
    );
    assert!(text_ok(&v, &["user", "rename", "alice", "al"]).contains("Renamed user 'alice'"));
    assert!(text_ok(&v, &["user", "passwd", "al", "--clear"])
        .contains("Password cleared (token-only auth)"));
    assert!(text_ok(&v, &["user", "rm", "al"]).contains("Deleted user 'al'"));
    let gone = v.run_text(&["user", "rm", "al"]);
    assert_eq!(gone.code, 3, "{}", gone.all());
    assert!(gone.all().contains("No user 'al'"));
}

#[test]
fn a_user_can_be_found_by_id_and_by_name_in_any_case() {
    let v = Vault::new("utext-find");
    let made = v.ok(&["user", "add", "Alice", "--no-password"]);
    let id = made["id"].as_str().unwrap().to_string();
    assert!(text_ok(&v, &["user", "rename", "ALICE", "alice2"]).contains("Renamed"));
    assert!(text_ok(&v, &["user", "rename", &id, "alice3"]).contains("Renamed"));
    let list = v.ok(&["user", "ls"]);
    assert_eq!(list["users"][0]["username"], "alice3");
}

#[test]
fn a_class_is_listed_assigned_and_removed_in_words() {
    let v = Vault::new("utext-class");
    assert!(text_ok(
        &v,
        &[
            "class",
            "add",
            "Deployers",
            "--desc",
            "CI",
            "--manage-users"
        ]
    )
    .contains("Created class 'Deployers'"));
    let list = text_ok(&v, &["class", "ls"]);
    let row = list.lines().find(|l| l.contains("Deployers")).unwrap();
    assert!(row.contains("yes") && row.contains("CI"), "{row}");
    let viewer = list.lines().find(|l| l.contains("Viewer")).unwrap();
    assert!(
        !viewer.contains("yes"),
        "Viewer has no management caps: {viewer}"
    );

    text_ok(&v, &["user", "add", "alice", "--no-password"]);
    assert!(text_ok(&v, &["user", "class", "alice", "Deployers"])
        .contains("Assigned 'alice' to class 'Deployers'"));
    assert!(text_ok(&v, &["user", "class", "alice", "--none"])
        .contains("Removed 'alice' from its class"));

    assert!(
        text_ok(&v, &["class", "set", "Deployers", "--name", "Releasers"])
            .contains("Updated class 'Releasers'")
    );
    let again = text_ok(&v, &["class", "ls"]);
    assert!(again.contains("Releasers") && !again.contains("Deployers"));
    assert!(text_ok(&v, &["class", "rm", "Releasers"]).contains("Deleted class 'Releasers'"));
}

#[test]
fn deleting_without_yes_on_a_script_is_refused_not_assumed() {
    let v = Vault::new("utext-noyes");
    v.ok(&["user", "add", "alice", "--no-password"]);
    let o = v.run_no_yes(&["user", "rm", "alice"]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert_eq!(v.ok(&["user", "ls"])["count"], 1, "alice must still exist");
}

#[test]
fn strict_write_is_switched_on_and_off_for_a_user_and_a_class() {
    let v = Vault::new("utext-strict");
    v.ok(&["user", "add", "alice", "--no-password"]);
    assert!(text_ok(&v, &["user", "strict-write", "alice"])
        .contains("writes must now match EVERY scope"));
    assert!(text_ok(&v, &["user", "strict-write", "alice", "--off"])
        .contains("writes match any scope again"));
    let unknown = v.run_text(&["user", "strict-write", "ghost"]);
    assert_eq!(unknown.code, 3, "{}", unknown.all());
}

#[test]
fn a_token_is_listed_without_its_secret_and_refused_before_minting_unless_revealed() {
    let v = Vault::new("utext-token");
    v.ok(&["user", "add", "alice", "--no-password"]);
    assert!(text_ok(&v, &["user", "token", "ls", "alice"]).contains("No tokens for 'alice'."));

    let refused = v.run_text(&["user", "token", "new", "alice"]);
    assert_ne!(refused.code, 0);
    assert!(
        refused.all().contains("Nothing was created."),
        "{}",
        refused.all()
    );
    assert!(text_ok(&v, &["user", "token", "ls", "alice"]).contains("No tokens"));

    let shown = v.run_text(&["--reveal", "user", "token", "new", "alice", "--desc", "ci"]);
    assert_eq!(shown.code, 0, "{}", shown.all());
    let secret = shown.stdout.trim().to_string();
    assert_eq!(secret.len(), 64, "the token alone on stdout: {secret}");
    assert!(shown.stderr.contains("will not be shown again"));

    let list = text_ok(&v, &["user", "token", "ls", "alice"]);
    assert!(list.contains("ci") && list.contains("never"), "{list}");
    assert!(
        !list.contains(&secret),
        "a listing must never print the token"
    );

    let id = v.ok(&["user", "token", "ls", "alice"])["tokens"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text_ok(&v, &["user", "token", "revoke", "alice", &id]).contains("Revoked"));
}

#[test]
fn a_token_written_to_a_file_is_owner_only_and_reported_with_its_id() {
    let v = Vault::new("utext-tokenfile");
    v.ok(&["user", "add", "alice", "--no-password"]);
    let path = v.path("alice.token");
    let out = text_ok(
        &v,
        &[
            "user",
            "token",
            "new",
            "alice",
            "--out",
            path.to_str().unwrap(),
        ],
    );
    assert!(out.contains("(0600)") && out.contains("Token id:"), "{out}");
    #[cfg(unix)]
    assert_eq!(common::mode_of(&path), 0o600);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        !out.contains(written.trim()),
        "the secret stays out of stdout"
    );
}
