// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Recovery packages ("Backups", README): a streamed, age-encrypted snapshot to a
//! destination, restored with a recovery identity into an EMPTY directory after
//! every member has been validated, and verifiable afterwards.
//!
//! * the package holds no plaintext, and `recover inspect` reads only
//!   non-secret envelope metadata;
//! * a restored directory verifies, and the original master password opens it;
//! * a wrong identity, a damaged package or a directory that is not empty
//!   restores nothing and disturbs nothing.

mod common;
use common::Vault;
use std::path::{Path, PathBuf};

const SECRET: &str = "package-secret-value-1234";

struct Packaged {
    vault: Vault,
    package: PathBuf,
    identity_file: PathBuf,
}

fn packaged(tag: &str) -> Packaged {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "GitHub", "--key", SECRET]);
    let key = v.ok(&["backup", "recovery-key"]);
    let identity = key["identity"].as_str().unwrap().to_string();
    assert!(identity.starts_with("AGE-SECRET-KEY-"), "{key}");
    let identity_file = v.path("identity.txt");
    std::fs::write(&identity_file, format!("{identity}\n")).unwrap();

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
    let made = v.ok(&["backup", "package", "--dest", "main"]);
    assert!(made["receipt"]["manifest"]["entry_count"] == 1, "{made}");

    let package = std::fs::read_dir(&dest)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|x| x == "unvpkg"))
        .expect("a .unvpkg must be written to the destination");
    Packaged {
        vault: v,
        package,
        identity_file,
    }
}

fn contains_bytes(path: &Path, needle: &str) -> bool {
    let bytes = std::fs::read(path).unwrap();
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

fn restore(p: &Packaged, to: &Path, identity: &Path) -> common::Out {
    p.vault.run(&[
        "recover",
        "restore",
        p.package.to_str().unwrap(),
        "--to",
        to.to_str().unwrap(),
        "--identity",
        identity.to_str().unwrap(),
    ])
}

fn dir_is_empty(path: &Path) -> bool {
    std::fs::read_dir(path).unwrap().next().is_none()
}

#[test]
fn a_package_is_written_with_a_completion_receipt_and_holds_no_plaintext() {
    let p = packaged("pkg-plain");
    let dest = p.vault.path("dest");
    let names: Vec<String> = std::fs::read_dir(&dest)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().any(|n| n.ends_with(".complete.json")),
        "the completion receipt: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == ".unenverse-destination.json"),
        "the destination marker: {names:?}"
    );
    assert!(
        !contains_bytes(&p.package, SECRET),
        "the package must be encrypted"
    );
    assert!(
        !contains_bytes(&p.package, "GitHub"),
        "names stay inside the encryption"
    );
    assert!(
        !contains_bytes(&p.package, "SQLite format"),
        "the database header must not be visible"
    );
}

#[test]
fn inspect_reads_only_non_secret_metadata_without_any_key() {
    let p = packaged("pkg-inspect");
    let o = p
        .vault
        .run(&["recover", "inspect", p.package.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    let m = &o.json()["data"]["manifest"];
    assert_eq!(m["entry_count"], 1, "{m}");
    let files: Vec<&str> = m["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(
        files.contains(&"vault.db") && files.contains(&"vault.salt"),
        "{files:?}"
    );
    assert!(
        !o.all().contains(SECRET) && !o.all().contains("GitHub"),
        "{}",
        o.all()
    );
}

#[test]
fn restore_installs_a_verifiable_vault_the_original_password_opens() {
    let p = packaged("pkg-restore");
    let to = p.vault.path("restored");
    std::fs::create_dir_all(&to).unwrap();
    let o = restore(&p, &to, &p.identity_file);
    assert_eq!(o.code, 0, "{}", o.all());
    for f in ["vault.db", "vault.salt", "manifest.json"] {
        assert!(to.join(f).exists(), "{f} must be restored");
    }

    let verify = p.vault.run(&["recover", "verify", to.to_str().unwrap()]);
    assert_eq!(
        verify.code,
        0,
        "a restored directory must verify: {}",
        verify.all()
    );

    // The restored vault is a normal vault: same master password, same entry.
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
    let shown = c
        .env_remove("UNV_SERVER_URL")
        .env("UNV_PASSWORD", common::PASSWORD)
        .env("HOME", p.vault.path("home"))
        .env("XDG_STATE_HOME", p.vault.path("state"))
        .args([
            "--db-path",
            to.join("vault.db").to_str().unwrap(),
            "--yes",
            "--json",
            "--reveal",
        ])
        .args(["get", "GitHub", "--field", "api_key"])
        .output()
        .unwrap();
    assert_eq!(
        shown.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&shown.stdout)
    );
    assert!(String::from_utf8_lossy(&shown.stdout).contains(SECRET));
}

#[test]
fn a_wrong_identity_restores_nothing() {
    let p = packaged("pkg-wrongid");
    let other = p.vault.ok(&["backup", "recovery-key"]);
    let wrong = p.vault.path("wrong.txt");
    std::fs::write(&wrong, format!("{}\n", other["identity"].as_str().unwrap())).unwrap();
    let to = p.vault.path("restored");
    std::fs::create_dir_all(&to).unwrap();
    let o = restore(&p, &to, &wrong);
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(dir_is_empty(&to), "a failed restore must install nothing");
}

#[test]
fn a_directory_that_is_not_empty_is_refused_and_left_untouched() {
    let p = packaged("pkg-notempty");
    let to = p.vault.path("restored");
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(to.join("precious.txt"), "keep me").unwrap();
    let o = restore(&p, &to, &p.identity_file);
    assert_ne!(o.code, 0, "{}", o.all());
    assert_eq!(
        std::fs::read_to_string(to.join("precious.txt")).unwrap(),
        "keep me"
    );
    assert!(
        !to.join("vault.db").exists(),
        "nothing may be installed beside it"
    );
}

#[test]
fn a_damaged_package_is_rejected_before_anything_is_installed() {
    let p = packaged("pkg-damaged");
    let mut bytes = std::fs::read(&p.package).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    std::fs::write(&p.package, bytes).unwrap();
    let to = p.vault.path("restored");
    std::fs::create_dir_all(&to).unwrap();
    let o = restore(&p, &to, &p.identity_file);
    assert_ne!(o.code, 0, "tampering must be detected: {}", o.all());
    assert!(
        dir_is_empty(&to),
        "every member is validated before any is installed"
    );
}

#[test]
fn a_directory_with_a_changed_database_fails_verification() {
    let p = packaged("pkg-verify");
    let to = p.vault.path("restored");
    std::fs::create_dir_all(&to).unwrap();
    assert_eq!(restore(&p, &to, &p.identity_file).code, 0);
    let mut db = std::fs::read(to.join("vault.db")).unwrap();
    let at = db.len() / 2;
    db[at] ^= 0xff;
    std::fs::write(to.join("vault.db"), db).unwrap();
    let verify = p.vault.run(&["recover", "verify", to.to_str().unwrap()]);
    assert_ne!(verify.code, 0, "{}", verify.all());
}
