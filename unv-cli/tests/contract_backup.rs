// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! "Backups, and the one thing you cannot recover" (README): two formats that do
//! different jobs, and the rules that keep a restore from becoming the way a
//! vault is lost.
//!
//! * `backup export` writes a `.vaultbak`: the contents re-encrypted under a
//!   fresh password. Restoring it needs only that password, never the original
//!   salt, so it moves a vault between machines;
//! * `backup archive` writes a `.vaultarc` holding the database AND its salt,
//!   the answer to losing `vault.salt`;
//! * restoring an archive refuses to overwrite an existing vault without
//!   `--force`, and verifies the checksums before writing anything.

mod common;
use common::Vault;
use std::path::Path;

const SECRET: &str = "backup-secret-value-1234";
const BACKUP_PW: &str = "backup-password-123";

fn seeded(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["project", "add", "Web"]);
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        SECRET,
        "--projects",
        "web",
        "--tags",
        "ci",
    ]);
    v.ok(&[
        "entry",
        "rotate",
        "GitHub",
        "--key",
        "second-secret-value-5678",
    ]);
    v
}

fn with_password(v: &Vault, password: &str, args: &[&str]) -> common::Out {
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
    for var in [
        "UNV_SERVER_URL",
        "UNV_ENV_FILE",
        "UNV_PROJECT",
        "UNV_TOKEN",
        "UNV_USER",
        "UNV_SESSION_FILE",
    ] {
        c.env_remove(var);
    }
    c.env("UNV_PASSWORD", password)
        .env("HOME", v.path("home"))
        .env("XDG_DATA_HOME", v.path("data"))
        .env("XDG_CONFIG_HOME", v.path("config"))
        .env("XDG_STATE_HOME", v.path("state"))
        .stdin(std::process::Stdio::null())
        .args([
            "--db-path",
            v.db().to_str().unwrap(),
            "--init",
            "--yes",
            "--json",
        ])
        .args(args);
    let o = c.output().unwrap();
    common::Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn contains_bytes(path: &Path, needle: &str) -> bool {
    let bytes = std::fs::read(path).unwrap();
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

// ---- .vaultbak

#[test]
fn a_vaultbak_holds_no_plaintext_and_restores_under_a_different_master_password() {
    let src = seeded("bak-src");
    let file = src.path("vault.vaultbak");
    let o = src.run(&[
        "backup",
        "export",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["entries"], 1);
    assert!(
        !contains_bytes(&file, SECRET),
        "the backup must be encrypted"
    );
    assert!(
        !contains_bytes(&file, "GitHub"),
        "even names stay inside the encryption"
    );

    // A different machine, a different master password and a different salt.
    let dst = Vault::new("bak-dst");
    let restored = with_password(
        &dst,
        "an-entirely-different-master-pw",
        &[
            "backup",
            "import",
            file.to_str().unwrap(),
            "--backup-password",
            BACKUP_PW,
        ],
    );
    assert_eq!(restored.code, 0, "{}", restored.all());

    let shown = with_password(
        &dst,
        "an-entirely-different-master-pw",
        &["--reveal", "get", "GitHub", "--field", "api_key"],
    );
    assert_eq!(shown.code, 0, "{}", shown.all());
    assert_eq!(shown.json()["data"]["value"], "second-secret-value-5678");
    // History and project membership came across too.
    let hist = with_password(
        &dst,
        "an-entirely-different-master-pw",
        &["entry", "history", "GitHub"],
    );
    assert_eq!(hist.json()["data"]["count"], 1, "{}", hist.stdout);
    let list = with_password(
        &dst,
        "an-entirely-different-master-pw",
        &["list", "--project", "web"],
    );
    assert_eq!(list.json()["data"]["count"], 1, "{}", list.stdout);
}

#[test]
fn a_backup_password_that_is_too_short_is_refused() {
    let v = seeded("bak-short");
    let file = v.path("short.vaultbak");
    let o = v.run(&[
        "backup",
        "export",
        file.to_str().unwrap(),
        "--backup-password",
        "short",
    ]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(
        !file.exists(),
        "a refused export must not leave a weakly protected file behind"
    );
}

#[test]
fn a_wrong_backup_password_restores_nothing_and_leaves_the_vault_alone() {
    let src = seeded("bak-wrong");
    let file = src.path("vault.vaultbak");
    src.ok(&[
        "backup",
        "export",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);

    let dst = Vault::new("bak-wrong-dst");
    dst.ok(&["entry", "add", "Existing", "--key", "existing-secret-1234"]);
    let o = dst.run(&[
        "backup",
        "import",
        file.to_str().unwrap(),
        "--backup-password",
        "not-the-backup-password",
    ]);
    assert_ne!(o.code, 0, "{}", o.all());
    let names = dst.ok(&["list"]);
    assert_eq!(
        names["count"], 1,
        "a failed restore must not replace the vault: {names}"
    );
    assert_eq!(
        dst.reveal_field("Existing", "api_key"),
        "existing-secret-1234"
    );
}

#[test]
fn a_damaged_backup_is_refused_not_half_restored() {
    let src = seeded("bak-damaged");
    let file = src.path("vault.vaultbak");
    src.ok(&[
        "backup",
        "export",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);
    let mut bytes = std::fs::read(&file).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    std::fs::write(&file, bytes).unwrap();

    let dst = Vault::new("bak-damaged-dst");
    dst.ok(&["entry", "add", "Existing", "--key", "existing-secret-1234"]);
    let o = dst.run(&[
        "backup",
        "import",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);
    assert_ne!(o.code, 0, "tampering must be detected: {}", o.all());
    assert_eq!(dst.ok(&["list"])["count"], 1);
}

// ---- .vaultarc

#[test]
fn an_archive_brings_a_vault_back_even_after_the_salt_is_lost() {
    let v = seeded("arc-lost-salt");
    let file = v.path("vault.vaultarc");
    let o = v.run(&[
        "backup",
        "archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    for part in ["db_sha256", "salt_sha256"] {
        assert!(
            o.json()["data"]["manifest"][part].is_string(),
            "the manifest records {part}"
        );
    }
    assert!(
        !contains_bytes(&file, SECRET),
        "the archive must be encrypted"
    );

    // The unrecoverable accident: the salt is gone, so the vault cannot be opened.
    std::fs::remove_file(v.dir.join("vault.salt")).unwrap();

    let restored = v.run(&[
        "backup",
        "restore-archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
        "--force",
    ]);
    assert_eq!(restored.code, 0, "{}", restored.all());
    assert!(
        v.dir.join("vault.salt").exists(),
        "the salt comes back with the database"
    );
    assert_eq!(
        v.reveal_field("GitHub", "api_key"),
        "second-secret-value-5678",
        "the original master password opens the restored vault"
    );
}

#[test]
fn restoring_an_archive_onto_an_existing_vault_needs_force_and_changes_nothing_without_it() {
    let v = seeded("arc-force");
    let file = v.path("vault.vaultarc");
    v.ok(&[
        "backup",
        "archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);
    v.ok(&["entry", "add", "Later", "--key", "added-after-backup-1"]);

    let refused = v.run(&[
        "backup",
        "restore-archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);
    assert_ne!(refused.code, 0, "{}", refused.all());
    assert_eq!(
        v.ok(&["list"])["count"],
        2,
        "without --force nothing is overwritten"
    );

    v.ok(&[
        "backup",
        "restore-archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
        "--force",
    ]);
    assert_eq!(
        v.ok(&["list"])["count"],
        1,
        "--force restores the state at backup time"
    );
}

#[test]
fn a_tampered_archive_is_rejected_before_anything_is_written() {
    let v = seeded("arc-tamper");
    let file = v.path("vault.vaultarc");
    v.ok(&[
        "backup",
        "archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);
    let db_before = std::fs::read(v.db()).unwrap();
    let salt_before = std::fs::read(v.dir.join("vault.salt")).unwrap();

    let mut bytes = std::fs::read(&file).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    std::fs::write(&file, bytes).unwrap();

    let o = v.run(&[
        "backup",
        "restore-archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
        "--force",
    ]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert_eq!(
        std::fs::read(v.db()).unwrap(),
        db_before,
        "a rejected archive must not touch the database"
    );
    assert_eq!(
        std::fs::read(v.dir.join("vault.salt")).unwrap(),
        salt_before,
        "nor the salt"
    );
}

#[test]
fn restoring_an_archive_works_where_there_is_no_vault_at_all() {
    let src = seeded("arc-src");
    let file = src.path("vault.vaultarc");
    src.ok(&[
        "backup",
        "archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);

    // "There is no vault here" is the state restore-archive exists for.
    let dst = Vault::new("arc-empty");
    assert!(!dst.db().exists());
    let o = dst.run(&[
        "backup",
        "restore-archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(
        dst.reveal_field("GitHub", "api_key"),
        "second-secret-value-5678"
    );
}

#[test]
fn a_wrong_archive_password_is_refused_and_writes_nothing() {
    let src = seeded("arc-wrongpw");
    let file = src.path("vault.vaultarc");
    src.ok(&[
        "backup",
        "archive",
        file.to_str().unwrap(),
        "--backup-password",
        BACKUP_PW,
    ]);
    let dst = Vault::new("arc-wrongpw-dst");
    let o = dst.run(&[
        "backup",
        "restore-archive",
        file.to_str().unwrap(),
        "--backup-password",
        "wrong-archive-password",
    ]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(
        !dst.db().exists(),
        "nothing may be written when the password is wrong"
    );
    assert!(!dst.dir.join("vault.salt").exists());
}
