// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Whole-vault `unv export` and `unv import --json`, as README describes them:
//!
//! * the flat formats (dotenv, yaml, k8s Secret, tfvars) carry real values, so they
//!   go to `--out` or need `--reveal`, never to a terminal by default;
//! * `--project` narrows an export to that project's entries, `--name` names the
//!   Kubernetes Secret;
//! * `--profile full` writes every entry's metadata, so vault-wide it is refused;
//! * an unknown format is invalid input;
//! * the JSON export is the vault: importing it into another vault with
//!   `import --json` replaces what was there with the same entries, values,
//!   projects and categories, and a document that is not a vault export is
//!   refused without touching anything.

mod common;

use common::Vault;

fn seeded(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["project", "add", "Web"]);
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "ghp_secret123",
        "--projects",
        "web",
    ]);
    v.ok(&["entry", "add", "Stripe", "--key", "sk_live_abc"]);
    v
}

fn revealed(v: &Vault, args: &[&str]) -> String {
    let o = v.run_with(&["--reveal"], args);
    assert_eq!(o.code, 0, "{args:?}: {}", o.all());
    o.stdout
}

#[test]
fn each_flat_format_carries_the_real_values_in_its_own_syntax() {
    let v = seeded("export-formats");
    let dotenv = revealed(&v, &["export", "--format", "dotenv"]);
    assert!(dotenv.contains("GITHUB=ghp_secret123"), "{dotenv}");
    assert!(dotenv.contains("STRIPE=sk_live_abc"), "{dotenv}");
    let yaml = revealed(&v, &["export", "--format", "yaml"]);
    assert!(yaml.contains("STRIPE: \"sk_live_abc\""), "{yaml}");
    let tfvars = revealed(&v, &["export", "--format", "tfvars"]);
    assert!(tfvars.contains("stripe = \"sk_live_abc\""), "{tfvars}");
    let k8s = revealed(&v, &["export", "--format", "k8s", "--name", "payments"]);
    assert!(k8s.contains("kind: Secret"), "{k8s}");
    assert!(
        k8s.contains("name: payments"),
        "--name names the Secret: {k8s}"
    );
    assert!(k8s.contains("STRIPE: \"sk_live_abc\""), "{k8s}");
}

#[test]
fn a_project_narrows_the_export_to_its_own_entries() {
    let v = seeded("export-project");
    let dotenv = revealed(&v, &["export", "--format", "dotenv", "--project", "web"]);
    assert!(dotenv.contains("GITHUB=ghp_secret123"), "{dotenv}");
    assert!(
        !dotenv.contains("STRIPE") && !dotenv.contains("sk_live_abc"),
        "an entry outside the project must not be exported: {dotenv}"
    );
}

#[test]
fn values_do_not_reach_a_terminal_unless_revealed_and_a_file_is_the_way_out() {
    let v = seeded("export-redacted");
    for fmt in ["dotenv", "yaml", "k8s", "tfvars"] {
        let o = v.run(&["export", "--format", fmt]);
        assert_eq!(o.code, 9, "{fmt}: {}", o.all());
        assert!(!o.all().contains("sk_live_abc"), "{fmt}: {}", o.all());
    }
    let file = v.path("out.env");
    let o = v.run(&[
        "export",
        "--format",
        "dotenv",
        "--out",
        file.to_str().unwrap(),
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(std::fs::read_to_string(&file)
        .unwrap()
        .contains("STRIPE=sk_live_abc"));
}

#[test]
fn the_json_export_to_the_terminal_is_the_vault_with_its_secrets_masked() {
    let v = seeded("export-json-masked");
    let o = v.run(&["export", "--format", "json"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains("sk_live_abc") && !o.all().contains("ghp_secret123"));
    assert_eq!(o.json()["data"]["api_keys"].as_array().unwrap().len(), 2);
}

#[test]
fn full_metadata_is_per_project_never_vault_wide_and_an_unknown_format_is_invalid() {
    let v = seeded("export-profile");
    let wide = v.run_with(
        &["--reveal"],
        &["export", "--format", "dotenv", "--profile", "full"],
    );
    assert_eq!(wide.code, 10, "{}", wide.all());
    assert!(wide.all().contains("--project"), "{}", wide.all());
    let scoped = v.run_with(
        &["--reveal"],
        &[
            "export",
            "--format",
            "dotenv",
            "--profile",
            "full",
            "--project",
            "web",
        ],
    );
    assert_eq!(scoped.code, 0, "{}", scoped.all());

    let unknown = v.run_with(&["--reveal"], &["export", "--format", "toml"]);
    assert_eq!(unknown.code, 10, "{}", unknown.all());
    assert!(unknown.all().contains("Unknown format 'toml'"));
}

#[test]
fn a_json_export_imports_back_as_the_same_vault_and_replaces_what_was_there() {
    let source = seeded("export-roundtrip-src");
    let file = source.path("vault.json");
    let o = source.run(&[
        "export",
        "--format",
        "json",
        "--out",
        file.to_str().unwrap(),
    ]);
    assert_eq!(o.code, 0, "{}", o.all());

    let target = Vault::new("export-roundtrip-dst");
    target.ok(&["entry", "add", "Leftover", "--key", "will-be-replaced"]);
    let done = target.run_text(&["import", file.to_str().unwrap(), "--json"]);
    assert_eq!(done.code, 0, "{}", done.all());
    // `import --json` names the input format and also puts the answer in an envelope.
    assert_eq!(done.json()["data"]["entries"], 2, "{}", done.stdout);

    let names: Vec<String> = target.ok(&["list"])["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["provider"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"GitHub".into()) && names.contains(&"Stripe".into()));
    assert!(
        !names.contains(&"Leftover".into()),
        "the import replaces: {names:?}"
    );
    assert_eq!(target.reveal_field("GitHub", "api_key"), "ghp_secret123");
    let projects = target.ok(&["project", "ls"]).to_string();
    assert!(
        projects.contains("Web"),
        "projects come back too: {projects}"
    );
}

#[test]
fn a_document_that_is_not_a_vault_export_is_refused_and_nothing_changes() {
    let v = seeded("export-import-bad");
    for (name, body) in [("array.json", "[1,2]"), ("garbage.json", "not json at all")] {
        let f = v.path(name);
        std::fs::write(&f, body).unwrap();
        let o = v.run_text(&["import", f.to_str().unwrap(), "--json"]);
        assert_ne!(o.code, 0, "{name}: {}", o.all());
    }
    assert_eq!(v.ok(&["list"])["count"], 2, "the vault is untouched");
}
