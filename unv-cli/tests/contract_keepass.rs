// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! KeePass migration ("Importing existing secrets", README): `unv import-vault
//! keepass` reads a KDBX with a key file (the password prompt needs a terminal),
//! previews first without showing a value, keeps attachments instead of dropping
//! them, places folders as categories, and refuses a wrong key without touching
//! the vault.

mod common;
use common::Vault;
use keepass::db::{fields, Value};
use keepass::{Database, DatabaseKey};
use std::path::PathBuf;

const PASS: &str = "kp-secret-value-9876543210";

struct Kdbx {
    file: PathBuf,
    key: PathBuf,
}

fn kdbx(v: &Vault) -> Kdbx {
    let key = v.path("db.key");
    std::fs::write(&key, "keyfile-contents-for-the-test-database\n").unwrap();
    let mut db = Database::new();
    let mut root = db.root_mut();
    root.add_entry().edit(|e| {
        e.set_unprotected(fields::TITLE, "Router");
        e.set_unprotected(fields::USERNAME, "admin");
        e.set_unprotected(fields::URL, "https://router.example.org");
        e.set_unprotected(fields::NOTES, "the grey box");
        e.set_protected(fields::PASSWORD, PASS);
        e.add_attachment(
            "config.txt",
            Value::unprotected(b"attached-file-bytes".to_vec()),
        );
    });
    root.add_group()
        .edit(|g| g.name = "Servers".into())
        .add_entry()
        .edit(|e| {
            e.set_unprotected(fields::TITLE, "NAS");
            e.set_protected(fields::PASSWORD, "another-kp-secret-0123456789");
        });
    let mut bytes = Vec::new();
    let mut k = std::fs::File::open(&key).unwrap();
    db.save(&mut bytes, DatabaseKey::new().with_keyfile(&mut k).unwrap())
        .unwrap();
    let file = v.path("db.kdbx");
    std::fs::write(&file, bytes).unwrap();
    Kdbx { file, key }
}

fn import(v: &Vault, k: &Kdbx, extra: &[&str]) -> common::Out {
    let mut a = vec![
        "import-vault",
        "keepass",
        k.file.to_str().unwrap(),
        "--key-file",
        k.key.to_str().unwrap(),
        "--no-password",
    ];
    a.extend_from_slice(extra);
    v.run(&a)
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

#[test]
fn a_keepass_preview_writes_nothing_and_shows_no_value() {
    let v = Vault::new("kppreview");
    let k = kdbx(&v);
    let o = import(&v, &k, &[]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["counts"]["imported"], 2, "{}", o.stdout);
    assert!(names(&v).is_empty(), "nothing is written without --apply");
    assert!(
        !o.all().contains(PASS) && !o.all().contains("attached-file-bytes"),
        "{}",
        o.all()
    );
}

#[test]
fn apply_imports_fields_keeps_the_attachment_and_a_rerun_is_a_no_op() {
    let v = Vault::new("kpapply");
    let k = kdbx(&v);
    let o = import(&v, &k, &["--apply"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(names(&v), vec!["NAS", "Router"]);
    assert_eq!(v.reveal_field("Router", "api_key"), PASS);
    let shown = v.ok(&["get", "Router"])["entries"][0].clone();
    assert_eq!(shown["account_name"], "admin", "{shown}");
    assert_eq!(shown["api_url"], "https://router.example.org", "{shown}");
    assert_eq!(
        shown["notes"]["length"], 12,
        "notes are a field too, so they are redacted by default: {shown}"
    );
    let ls = v.ok(&["migrate", "archive", "ls"]);
    let rec = ls["archives"].as_array().unwrap().iter().find(|a| {
        a["reason"]
            .as_str()
            .unwrap()
            .contains("attachment config.txt")
    });
    let id = rec.unwrap_or_else(|| panic!("the attachment is kept, not dropped: {ls}"))["id"]
        .as_str()
        .unwrap()
        .to_string();
    let out = v.path("att.txt");
    let ex = v.run(&[
        "migrate",
        "archive",
        "export",
        &id,
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(ex.code, 0, "{}", ex.all());
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "attached-file-bytes",
        "byte for byte"
    );

    let again = import(&v, &k, &["--apply"]);
    assert_eq!(again.code, 0, "{}", again.all());
    assert_eq!(
        again.json()["data"]["counts"]["imported"],
        0,
        "{}",
        again.stdout
    );
    assert_eq!(names(&v).len(), 2, "no duplicates");
}

#[test]
fn a_wrong_key_file_is_a_named_error_and_leaves_the_vault_alone() {
    let v = Vault::new("kpwrong");
    let k = kdbx(&v);
    let other = v.path("other.key");
    std::fs::write(&other, "a different key file\n").unwrap();
    let o = v.run(&[
        "import-vault",
        "keepass",
        k.file.to_str().unwrap(),
        "--key-file",
        other.to_str().unwrap(),
        "--no-password",
        "--apply",
    ]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(
        o.all().contains("incorrect password or key file"),
        "{}",
        o.all()
    );
    assert!(names(&v).is_empty());
}

#[test]
fn keepass_needs_a_key_of_some_kind_and_the_key_flags_are_keepass_only() {
    let v = Vault::new("kpflags");
    let k = kdbx(&v);
    let none = v.run(&[
        "import-vault",
        "keepass",
        k.file.to_str().unwrap(),
        "--no-password",
    ]);
    assert_eq!(
        none.code,
        2,
        "--no-password without --key-file is a usage error: {}",
        none.all()
    );
    let bw = v.run(&[
        "import-vault",
        "bitwarden",
        k.file.to_str().unwrap(),
        "--key-file",
        k.key.to_str().unwrap(),
    ]);
    assert_eq!(bw.code, 10, "{}", bw.all());
    let force = import(&v, &k, &["--force"]);
    assert_eq!(
        force.code,
        10,
        "durable migrations name the conflict instead: {}",
        force.all()
    );
}
