// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! "Lossless config migration" (README): `unv migrate` reads a `.env`, Compose,
//! WireGuard or nginx file locally, plans first, keeps the source read-only,
//! stores a byte-preserving template when asked, treats a repeat of an
//! unchanged source as a no-op, and turns a changed source plus a separately
//! changed vault entry into a named conflict instead of choosing a side.

mod common;
use common::Vault;
use serde_json::Value;

const SOURCE: &str =
    "# app config\nDB_PASSWORD=hunter2-real-pass\nAPI_URL=https://api.example.com\nLITERAL=pre$${fix}\n";

fn source(v: &Vault, body: &str) -> String {
    let p = v.path("app.env");
    std::fs::write(&p, body).unwrap();
    p.to_str().unwrap().to_string()
}

fn counts(o: &common::Out) -> Value {
    o.json()["data"]["counts"].clone()
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

fn migration_id(v: &Vault) -> String {
    v.ok(&["migrate", "report"])["migrations"][0]["id"]
        .as_str()
        .expect("a migration")
        .to_string()
}

#[test]
fn plan_inspects_and_changes_nothing() {
    let v = Vault::new("plan");
    let f = source(&v, SOURCE);
    let o = v.run(&["migrate", "plan", "env", &f, "--template"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(counts(&o)["imported"], 3, "{}", o.stdout);
    assert!(names(&v).is_empty(), "a plan writes no entry");
    assert_eq!(
        v.ok(&["migrate", "report"])["migrations"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "and no migration record"
    );
    assert!(
        !o.all().contains("hunter2-real-pass"),
        "a plan names things, not values: {}",
        o.all()
    );
}

#[test]
fn apply_imports_the_entries_keeps_the_source_untouched_and_verifies() {
    let v = Vault::new("apply");
    let f = source(&v, SOURCE);
    let applied = v.run(&["migrate", "apply", "env", &f, "--template"]);
    assert_eq!(applied.code, 0, "{}", applied.all());
    assert_eq!(names(&v), vec!["API_URL", "DB_PASSWORD", "LITERAL"]);
    assert_eq!(
        v.reveal_field("DB_PASSWORD", "api_key"),
        "hunter2-real-pass"
    );
    assert_eq!(
        std::fs::read_to_string(&f).unwrap(),
        SOURCE,
        "the source is read-only to the migration"
    );

    let id = migration_id(&v);
    let verified = v.ok(&["migrate", "verify", &id]);
    assert_eq!(
        verified["verified"], true,
        "re-rendering the template reproduces the source bytes: {verified}"
    );
    assert_eq!(verified["sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn a_repeat_of_an_unchanged_source_is_a_no_op() {
    let v = Vault::new("repeat");
    let f = source(&v, SOURCE);
    v.ok(&["migrate", "apply", "env", &f, "--template"]);
    let again = v.run(&["migrate", "apply", "env", &f, "--template"]);
    assert_eq!(again.code, 0, "{}", again.all());
    let c = counts(&again);
    assert_eq!(c["imported"], 0, "{c}");
    assert_eq!(c["skipped"], 3, "{c}");
    assert_eq!(names(&v).len(), 3, "no duplicates");
}

#[test]
fn a_changed_source_and_a_changed_vault_entry_is_a_named_conflict_and_nothing_is_written() {
    let v = Vault::new("conflict");
    let f = source(&v, SOURCE);
    v.ok(&["migrate", "apply", "env", &f, "--template"]);
    v.ok(&[
        "entry",
        "set",
        "DB_PASSWORD",
        "--key",
        "changed-in-the-vault-1234",
    ]);
    std::fs::write(
        &f,
        SOURCE
            .replace("hunter2-real-pass", "changed-at-the-source-9999")
            .replace("api.example.com", "api.other.example"),
    )
    .unwrap();

    let o = v.run(&["migrate", "apply", "env", &f, "--template"]);
    assert_eq!(o.code, 6, "{}", o.all());
    assert_eq!(o.error_code(), "conflict");
    assert!(o.all().contains("nothing was written"), "{}", o.all());
    assert_eq!(
        v.reveal_field("DB_PASSWORD", "api_key"),
        "changed-in-the-vault-1234",
        "the vault side is untouched"
    );
    assert_eq!(
        v.reveal_field("API_URL", "api_key"),
        "https://api.example.com",
        "an item that did not conflict is not applied either: the apply is atomic"
    );
}

#[test]
fn the_archive_keeps_the_template_byte_for_byte_with_references_instead_of_values() {
    let v = Vault::new("archive");
    let f = source(&v, SOURCE);
    v.ok(&["migrate", "apply", "env", &f, "--template"]);
    let ls = v.ok(&["migrate", "archive", "ls"]);
    let id = ls["archives"][0]["id"]
        .as_str()
        .expect("an archive record")
        .to_string();
    assert_eq!(ls["archives"][0]["reason"], "lossless migration template");

    let out = v.path("template.txt");
    let o = v.run(&[
        "migrate",
        "archive",
        "export",
        &id,
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    let template = std::fs::read_to_string(&out).unwrap();
    assert!(
        template.starts_with("# app config\n"),
        "comments survive: {template}"
    );
    assert!(
        template.contains("DB_PASSWORD=${UNV_MIG_"),
        "imported values become references: {template}"
    );
    assert!(
        !template.contains("hunter2-real-pass"),
        "the template holds no value: {template}"
    );
    assert_eq!(
        template.matches("${UNV_MIG_").count(),
        3,
        "one reference per variable, comments kept: {template}"
    );
    // The source's own `$${` is a value like any other: it lives in the vault entry,
    // so rendering the template gives the source back (checked by `migrate verify`).
    assert_eq!(v.reveal_field("LITERAL", "api_key"), "pre$${fix}");
    #[cfg(unix)]
    assert_eq!(common::mode_of(&out), 0o600, "written to a private file");
}

#[test]
fn report_lists_migrations_without_values_or_file_contents() {
    let v = Vault::new("report");
    let f = source(&v, SOURCE);
    v.ok(&["migrate", "apply", "env", &f, "--template"]);
    let o = v.run(&["migrate", "report"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        !o.all().contains("hunter2-real-pass") && !o.all().contains("# app config"),
        "{}",
        o.all()
    );
    assert_eq!(o.json()["data"]["migrations"][0]["source_kind"], "env");
}

#[test]
fn a_migration_needs_the_local_vault_and_a_readable_source() {
    let v = Vault::new("needs");
    let missing = v.run(&["migrate", "plan", "env", "/nonexistent/path/app.env"]);
    assert_ne!(missing.code, 0, "{}", missing.all());
    assert!(names(&v).is_empty());
}

#[test]
fn the_other_formats_are_read_too() {
    let v = Vault::new("formats");
    let compose = v.path("docker-compose.yml");
    std::fs::write(&compose, "services:\n  web:\n    image: nginx\n    environment:\n      SECRET_KEY: compose-secret-value-123\n").unwrap();
    let o = v.run(&["migrate", "plan", "compose", compose.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(counts(&o)["read"].as_u64().unwrap() >= 1, "{}", o.stdout);
    assert!(!o.all().contains("compose-secret-value-123"), "{}", o.all());
}
