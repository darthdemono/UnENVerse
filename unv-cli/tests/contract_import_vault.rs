//! `unv import-vault` ("Importing existing secrets", README): the vendor
//! importers are preview-first, never show a value in the preview, write
//! atomically, treat a re-import of an unchanged source as a no-op, and turn a
//! changed source plus a separately changed vault entry into a named conflict
//! instead of picking a winner.

mod common;
use common::Vault;
use serde_json::Value;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("unv-app")
        .join("tests")
        .join("fixtures")
        .join(name)
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

#[test]
fn the_default_is_a_preview_that_writes_nothing_and_shows_no_value() {
    let v = Vault::new("preview");
    let file = fixture("bitwarden.json");
    let o = v.run(&["import-vault", "bitwarden", file.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(
        counts(&o)["imported"],
        5,
        "the preview reports what would be imported: {}",
        o.stdout
    );
    assert!(names(&v).is_empty(), "nothing is written without --apply");
    for secret in ["ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "s3cr3t"] {
        assert!(
            !o.all().contains(secret),
            "a preview must show fingerprints, not values: {}",
            o.all()
        );
    }
}

#[test]
fn apply_imports_the_entries_with_their_values_and_a_rerun_is_a_no_op() {
    let v = Vault::new("apply");
    let file = fixture("bitwarden.json");
    let path = file.to_str().unwrap();
    let applied = v.run(&["import-vault", "bitwarden", path, "--apply"]);
    assert_eq!(applied.code, 0, "{}", applied.all());
    assert_eq!(counts(&applied)["imported"], 5);
    assert_eq!(
        names(&v),
        vec![
            "Deploy key",
            "GitHub",
            "No password here",
            "Postgres prod",
            "Visa"
        ]
    );
    assert_eq!(
        v.reveal_field("GitHub", "api_key"),
        "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );

    let again = v.run(&["import-vault", "bitwarden", path, "--apply"]);
    assert_eq!(again.code, 0, "{}", again.all());
    let c = counts(&again);
    assert_eq!(
        c["imported"], 0,
        "re-importing an unchanged source is a no-op: {c}"
    );
    assert_eq!(c["skipped"], 5, "{c}");
    assert_eq!(names(&v).len(), 5, "no duplicates");
}

#[test]
fn a_changed_source_and_a_changed_vault_entry_is_a_conflict_and_nothing_is_written() {
    let v = Vault::new("conflict");
    let file = fixture("bitwarden.json");
    v.ok(&[
        "import-vault",
        "bitwarden",
        file.to_str().unwrap(),
        "--apply",
    ]);

    // The vault entry changes on its own...
    v.ok(&[
        "entry",
        "set",
        "GitHub",
        "--key",
        "ghp_changed_in_the_vault_0000000000000000",
    ]);
    // ...and so does the source.
    let mut doc: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    doc["items"][0]["login"]["password"] =
        Value::String("ghp_changed_at_the_source_111111111111111".into());
    doc["items"][1]["login"]["password"] =
        Value::String("a-source-change-that-must-not-land".into());
    let changed = v.path("bitwarden-changed.json");
    std::fs::write(&changed, doc.to_string()).unwrap();

    let o = v.run(&[
        "import-vault",
        "bitwarden",
        changed.to_str().unwrap(),
        "--apply",
    ]);
    assert_eq!(o.code, 6, "{}", o.all());
    assert_eq!(o.error_code(), "conflict");
    assert!(o.all().contains("nothing was written"), "{}", o.all());
    // Atomic: not even the item that did NOT conflict was applied.
    assert_eq!(
        v.reveal_field("GitHub", "api_key"),
        "ghp_changed_in_the_vault_0000000000000000"
    );
    assert_ne!(
        v.reveal_field("Postgres prod", "api_key"),
        "a-source-change-that-must-not-land",
        "a conflict must leave the vault exactly as it was"
    );
}

#[test]
fn an_encrypted_bitwarden_export_is_refused_by_name() {
    let v = Vault::new("encrypted");
    let f = v.path("enc.json");
    std::fs::write(
        &f,
        r#"{"encrypted":true,"encKeyValidation_DO_NOT_EDIT":"x","data":"y"}"#,
    )
    .unwrap();
    let o = v.run(&["import-vault", "bitwarden", f.to_str().unwrap(), "--apply"]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert_eq!(o.error_code(), "invalid");
    assert!(
        o.all().to_lowercase().contains("encrypted"),
        "refused BY NAME, not 'parse error': {}",
        o.all()
    );
    assert!(names(&v).is_empty());
}

#[test]
fn a_1password_export_is_a_durable_migration_with_the_same_rules() {
    let v = Vault::new("onepassword");
    let path = fixture("onepassword.json");
    let preview = v.run(&["import-vault", "onepassword", path.to_str().unwrap()]);
    assert_eq!(preview.code, 0, "{}", preview.all());
    let would = counts(&preview)["imported"].as_u64().unwrap();
    assert!(would > 0, "the fixture holds entries: {}", preview.stdout);
    assert!(names(&v).is_empty(), "a preview writes nothing");

    let applied = v.run(&[
        "import-vault",
        "onepassword",
        path.to_str().unwrap(),
        "--apply",
    ]);
    assert_eq!(applied.code, 0, "{}", applied.all());
    assert_eq!(
        names(&v).len() as u64,
        would,
        "what the preview promised is what lands"
    );
    let again = v.run(&[
        "import-vault",
        "onepassword",
        path.to_str().unwrap(),
        "--apply",
    ]);
    assert_eq!(
        counts(&again)["imported"],
        0,
        "an unchanged source is a no-op: {}",
        again.stdout
    );
}

#[test]
fn a_proton_export_previews_with_fingerprints_and_writes_only_with_apply() {
    let v = Vault::new("proton");
    let path = fixture("proton.json");
    let preview = v.run(&["import-vault", "proton", path.to_str().unwrap()]);
    assert_eq!(preview.code, 0, "{}", preview.all());
    let data = &preview.json()["data"];
    assert_eq!(data["applied"], false, "{data}");
    assert_eq!(data["created"], 2, "{data}");
    assert!(
        data["items"][0]["fingerprint"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"),
        "the preview shows fingerprints rather than values: {data}"
    );
    assert!(names(&v).is_empty(), "a preview writes nothing");

    let applied = v.run(&["import-vault", "proton", path.to_str().unwrap(), "--apply"]);
    assert_eq!(applied.code, 0, "{}", applied.all());
    assert_eq!(names(&v), vec!["DigitalOcean", "Root CA"]);
}

#[test]
fn project_and_category_flags_place_every_imported_entry() {
    let v = Vault::new("placed");
    v.ok(&["project", "add", "Web"]);
    let file = fixture("bitwarden.json");
    v.ok(&[
        "import-vault",
        "bitwarden",
        file.to_str().unwrap(),
        "--apply",
        "--project",
        "web",
        "--category",
        "imported",
    ]);
    assert_eq!(v.ok(&["list", "--project", "web"])["count"], 5);
    let e = &v.ok(&["get", "GitHub"])["entries"][0];
    assert!(e["categories"].to_string().contains("imported"), "{e}");
}

#[test]
fn an_unreadable_source_is_refused_without_touching_the_vault() {
    let v = Vault::new("garbage");
    v.ok(&["entry", "add", "Existing", "--key", "existing-secret-1234"]);
    let f = v.path("junk.json");
    std::fs::write(&f, "this is not json at all").unwrap();
    let o = v.run(&["import-vault", "bitwarden", f.to_str().unwrap(), "--apply"]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert_eq!(names(&v), vec!["Existing"]);
}
