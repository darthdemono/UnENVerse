// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! A new user's first `unv --init`: what may create a vault and what may not.
use std::process::{Command, Stdio};

fn unv(dir: &std::path::Path, password: Option<&str>, args: &[&str]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_unv"));
    c.env_remove("UNV_SERVER_URL")
        .env_remove("UNV_PASSWORD")
        .env_remove("UNV_PASSWORD_FILE")
        .env_remove("UNV_ENV_FILE")
        .stdin(Stdio::null())
        .arg("--db-path")
        .arg(dir.join("vault.db"))
        .arg("--salt-path")
        .arg(dir.join("vault.salt"));
    if let Some(p) = password {
        c.env("UNV_PASSWORD", p);
    }
    c.args(args).output().unwrap()
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("unv-first-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn long_password() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("first-run-{}-{nanos}", std::process::id())
}

/// The bug: with no terminal and no password source, the prompt failed, the empty answer was
/// taken as the password, and `--init` created a vault that anyone could open with nothing.
#[test]
fn init_without_any_password_source_creates_no_vault() {
    let dir = scratch("none");
    let o = unv(&dir, None, &["--init", "status"]);
    assert_eq!(
        o.status.code(),
        Some(10),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("at least 12 characters"));
    assert!(!dir.join("vault.db").exists());
    assert!(!dir.join("vault.salt").exists());
}

/// The bug: a one-character password was accepted for a new vault while the desktop and
/// `unv vault rekey` refuse anything under twelve.
#[test]
fn init_refuses_a_short_password_and_creates_nothing() {
    let dir = scratch("short");
    let o = unv(&dir, Some("short"), &["--init", "status"]);
    assert_eq!(o.status.code(), Some(10));
    assert!(!dir.join("vault.db").exists());
}

/// The floor applies to creation only: an existing vault still opens, and a wrong password
/// is still a wrong password, not a length complaint.
#[test]
fn a_long_password_creates_the_vault_and_reopens_it() {
    let dir = scratch("long");
    let pw = long_password();
    assert!(unv(&dir, Some(&pw), &["--init", "status"]).status.success());
    assert!(dir.join("vault.db").exists());
    assert!(unv(&dir, Some(&pw), &["status"]).status.success());
    let wrong = unv(&dir, Some("short"), &["status"]);
    assert_eq!(wrong.status.code(), Some(5));
}

/// The message must say what to type, not only what to pass.
#[test]
fn no_vault_message_names_the_command_that_creates_one() {
    let dir = scratch("msg");
    let o = unv(&dir, Some(&long_password()), &["status"]);
    assert_eq!(o.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&o.stderr).contains("unv --init status"));
}

/// The bug: on a machine with no vault, `unv doctor` reported "the salt is missing, this vault
/// cannot be opened" with "there is no repair", which reads as a destroyed vault.
#[test]
fn doctor_on_a_machine_with_no_vault_says_there_is_none_yet() {
    let dir = scratch("doctor");
    let o = unv(&dir, Some(&long_password()), &["--json", "doctor"]);
    let text = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(text.contains("No vault here yet"), "{text}");
    assert!(!text.contains("cannot be opened"), "{text}");
    assert!(
        !dir.join("vault.salt").exists(),
        "doctor must not create anything"
    );
}

/// The bug: only `unv-server` demanded twelve characters for a named user's password. The same
/// vault, opened locally by the CLI or the desktop, accepted any password, so a weak account
/// could be made by choosing the other front end.
#[test]
fn a_local_user_needs_a_long_password_too() {
    let dir = scratch("user");
    let pw = long_password();
    assert!(unv(&dir, Some(&pw), &["--init", "status"]).status.success());
    let short = format!("s{}", std::process::id() % 100);
    let refused = unv(
        &dir,
        Some(&pw),
        &["user", "add", "bob", "--user-password", &short],
    );
    assert_eq!(
        refused.status.code(),
        Some(10),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    let listed = unv(&dir, Some(&pw), &["user", "ls"]);
    assert!(!String::from_utf8_lossy(&listed.stdout).contains("bob"));
    let ok = unv(
        &dir,
        Some(&pw),
        &["user", "add", "bob", "--user-password", &long_password()],
    );
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

/// The security phrase is the third credential of a login, after the password and the 2FA code.
/// It is a secret like the password: listed in the type list, stored, and never printed.
#[test]
fn a_security_phrase_is_accepted_and_never_printed() {
    let dir = scratch("phrase");
    let pw = long_password();
    let phrase = format!("my-first-pet-{}", std::process::id());
    let added = unv(
        &dir,
        Some(&pw),
        &[
            "--init",
            "entry",
            "add",
            "Bank phrase",
            "--type",
            "security_phrase",
            "--key",
            &phrase,
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let shown = unv(&dir, Some(&pw), &["get", "Bank phrase"]);
    let text = String::from_utf8_lossy(&shown.stdout).to_string();
    assert!(text.contains("security_phrase"), "{text}");
    assert!(
        !text.contains(&phrase),
        "the phrase must not be printed: {text}"
    );
}

/// A secret phrase is a secret type of its own: accepted by `--type`, stored, and never printed
/// (the words are the primary value and the whole value is masked).
#[test]
fn a_secret_phrase_is_accepted_and_never_printed() {
    let dir = scratch("secretphrase");
    let pw = long_password();
    let words = "legal winner thank year wave sausage worth useful legal winner thank yellow";
    let added = unv(
        &dir,
        Some(&pw),
        &[
            "--init",
            "entry",
            "add",
            "Hardware wallet",
            "--type",
            "secret_phrase",
            "--key",
            words,
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let shown = unv(&dir, Some(&pw), &["get", "Hardware wallet"]);
    let text = String::from_utf8_lossy(&shown.stdout).to_string();
    assert!(text.contains("secret_phrase"), "{text}");
    assert!(
        !text.contains("legal winner"),
        "the words must not be printed: {text}"
    );
}
