//! Entries: importing, enriching, presets and the lifecycle commands, as the
//! README describes them ("Secrets", "Importing existing secrets", "Templates",
//! "Secrets UnENVerse recognises on sight").
//!
//! * `import` upserts by provider, so re-running it never duplicates;
//! * `enrich` is a preview by default and only writes with `--apply`, only fills
//!   gaps unless `--force`, and recognises an issuer from its public prefix;
//! * a preset pre-fills an entry and explicit flags override it;
//! * tags, pins and the compromised flag do what they say, a compromised entry
//!   is a high-severity health finding, and nothing a rotation or restore
//!   replaces is ever lost.

mod common;
use common::Vault;
use serde_json::Value;

const GITHUB_PAT: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

fn names(v: &Vault, extra: &[&str]) -> Vec<String> {
    let mut args = vec!["list"];
    args.extend_from_slice(extra);
    let o = v.run(&args);
    let mut n: Vec<String> = o.json()["data"]["entries"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| e["provider"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();
    n.sort();
    n
}

fn entry(v: &Vault, provider: &str) -> Value {
    v.ok(&["get", provider])["entries"][0].clone()
}

fn write_env(v: &Vault, name: &str, body: &str) -> String {
    let p = v.path(name);
    std::fs::write(&p, body).unwrap();
    p.to_str().unwrap().to_string()
}

// ---- import

#[test]
fn importing_an_env_file_creates_one_entry_per_variable_and_never_duplicates_on_a_rerun() {
    let v = Vault::new("import");
    let file = write_env(
        &v,
        "in.env",
        &format!("GITHUB_TOKEN={GITHUB_PAT}\nDB_URL=postgres://u:p@h/db\nPLAIN=hello\n"),
    );
    // `--json` is deliberately not used: on `import` it means "the file is a JSON export".
    let first = v.run_text(&["import", &file]);
    assert_eq!(first.code, 0, "{}", first.all());
    assert!(
        first.all().contains("3 added, 0 updated"),
        "{}",
        first.all()
    );
    assert_eq!(names(&v, &[]), vec!["DB_URL", "GITHUB_TOKEN", "PLAIN"]);

    let again = v.run_text(&["import", &file]);
    assert_eq!(again.code, 0, "{}", again.all());
    assert!(
        again.all().contains("Nothing changed"),
        "a rerun of an unchanged file changes nothing: {}",
        again.all()
    );
    assert_eq!(names(&v, &[]).len(), 3);
}

#[test]
fn an_import_stamps_the_project_category_and_environment_it_was_given() {
    let v = Vault::new("importmeta");
    v.ok(&["project", "add", "Web"]);
    let file = write_env(&v, "in.env", "API_KEY=abcdef123456\n");
    let o = v.run_text(&[
        "import",
        &file,
        "--project",
        "web",
        "--env",
        "staging",
        "--category",
        "infra",
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    let e = entry(&v, "API_KEY");
    assert_eq!(e["environment"], "staging", "{e}");
    assert!(e["projectIds"].to_string().contains("web"), "{e}");
    assert!(e["categories"].to_string().contains("infra"), "{e}");
}

// ---- enrich

#[test]
fn enrich_previews_by_default_and_writes_only_with_apply() {
    let v = Vault::new("enrich");
    let file = write_env(&v, "in.env", &format!("GITHUB_TOKEN={GITHUB_PAT}\n"));
    v.run_text(&["import", &file]);
    let before = entry(&v, "GITHUB_TOKEN");
    assert!(before.get("api_url").is_none(), "{before}");

    let preview = v.run_text(&["enrich"]);
    assert_eq!(preview.code, 0, "{}", preview.all());
    assert!(
        preview.all().contains("Nothing written"),
        "{}",
        preview.all()
    );
    assert!(
        preview.all().contains("api.github.com"),
        "{}",
        preview.all()
    );
    assert_eq!(
        entry(&v, "GITHUB_TOKEN"),
        before,
        "a preview must not change anything"
    );

    let applied = v.run_text(&["enrich", "--apply"]);
    assert_eq!(applied.code, 0, "{}", applied.all());
    let after = entry(&v, "GITHUB_TOKEN");
    assert_eq!(after["api_url"], "https://api.github.com", "{after}");
    assert!(
        after["tags"].to_string().contains("github"),
        "the issuer is recognised from ghp_: {after}"
    );
}

#[test]
fn enrich_fills_gaps_but_does_not_overwrite_what_is_there_unless_forced() {
    let v = Vault::new("enrichgap");
    let file = write_env(&v, "in.env", &format!("GITHUB_TOKEN={GITHUB_PAT}\n"));
    v.run_text(&["import", &file]);
    v.ok(&[
        "entry",
        "set",
        "GITHUB_TOKEN",
        "--url",
        "https://ghe.internal.example",
    ]);
    v.ok(&["entry", "set", "GITHUB_TOKEN", "--desc", "my own words"]);

    v.run_text(&["enrich", "--apply"]);
    let kept = entry(&v, "GITHUB_TOKEN");
    assert_eq!(
        kept["api_description"], "my own words",
        "enrich must only fill gaps: {kept}"
    );
    assert_eq!(kept["api_url"], "https://ghe.internal.example", "{kept}");

    v.run_text(&["enrich", "--apply", "--force"]);
    let forced = entry(&v, "GITHUB_TOKEN");
    assert_ne!(
        forced["api_description"], "my own words",
        "--force is the way to overwrite: {forced}"
    );
    assert_eq!(forced["api_url"], "https://api.github.com", "{forced}");
}

// ---- presets

#[test]
fn a_preset_prefills_an_entry_and_explicit_flags_override_it() {
    let v = Vault::new("preset");
    let ls = v.ok(&["template", "ls"]);
    let ids: Vec<&str> = ls["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(ids.len() >= 10, "ten starter templates: {ids:?}");
    assert!(ids.contains(&"github-pat"), "{ids:?}");

    v.ok(&[
        "entry",
        "add",
        "Mine",
        "--preset",
        "github-pat",
        "--key",
        "k-1234567890",
    ]);
    let preset = entry(&v, "Mine");
    assert!(
        preset["api_url"].as_str().unwrap_or("").contains("github"),
        "the preset pre-fills the URL: {preset}"
    );

    v.ok(&[
        "entry",
        "add",
        "Other",
        "--preset",
        "github-pat",
        "--key",
        "k-1234567890",
        "--url",
        "https://ghe.example/api",
    ]);
    let other = entry(&v, "Other");
    assert!(
        other.to_string().contains("https://ghe.example/api"),
        "explicit flags override the preset: {other}"
    );
}

// ---- lifecycle

#[test]
fn tags_are_added_and_removed_and_filter_the_list() {
    let v = Vault::new("tags");
    v.ok(&["entry", "add", "One", "--key", "k-1111111", "--tags", "a,b"]);
    v.ok(&["entry", "add", "Two", "--key", "k-2222222", "--tags", "b"]);
    let after = v.ok(&["entry", "tag", "One", "--add", "shared", "--remove", "a"]);
    assert_eq!(after["tags"], serde_json::json!(["b", "shared"]));
    assert_eq!(names(&v, &["--tag", "b"]), vec!["One", "Two"]);
    assert_eq!(names(&v, &["--tag", "shared"]), vec!["One"]);
    assert_eq!(v.run(&["list", "--tag", "nothing"]).code, 0);
}

#[test]
fn list_filters_by_environment_type_and_project() {
    let v = Vault::new("filters");
    v.ok(&["project", "add", "Web"]);
    v.ok(&[
        "entry",
        "add",
        "StagingKey",
        "--key",
        "k-1111111",
        "--env",
        "staging",
        "--projects",
        "web",
    ]);
    v.ok(&[
        "entry",
        "add",
        "ProdKey",
        "--key",
        "k-2222222",
        "--env",
        "production",
    ]);
    v.ok(&[
        "entry",
        "add",
        "Pw",
        "--key",
        "p-3333333",
        "--type",
        "password",
        "--username",
        "u",
    ]);
    assert_eq!(names(&v, &["--env", "staging"]), vec!["StagingKey"]);
    assert_eq!(names(&v, &["--project", "web"]), vec!["StagingKey"]);
    assert_eq!(names(&v, &["--type", "password"]), vec!["Pw"]);
}

#[test]
fn a_compromised_entry_is_a_high_severity_finding_until_it_is_cleared() {
    let v = Vault::new("compromise");
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "a-long-enough-secret-value",
    ]);
    let quiet = v.ok(&["scan"]);
    assert_eq!(quiet["high"], 0, "{quiet}");

    v.ok(&["entry", "compromise", "GitHub"]);
    let flagged = v.ok(&["scan"]);
    assert_eq!(flagged["high"], 1, "{flagged}");
    let issue = flagged["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["field"] == "compromised")
        .expect("the compromised flag must be reported");
    assert_eq!(issue["severity"], "high");
    assert_eq!(issue["subject"], "GitHub");

    v.ok(&["entry", "compromise", "GitHub", "--off"]);
    assert_eq!(v.ok(&["scan"])["high"], 0);
}

#[test]
fn pinning_is_a_flag_that_can_be_turned_off() {
    let v = Vault::new("pin");
    v.ok(&["entry", "add", "GitHub", "--key", "k-1234567"]);
    let on = v.ok(&["entry", "pin", "GitHub"]);
    assert_eq!(on["value"], true);
    assert_eq!(entry(&v, "GitHub")["pinned"], true);
    let off = v.ok(&["entry", "pin", "GitHub", "--off"]);
    assert_eq!(off["value"], false);
}

#[test]
fn a_rotation_and_a_restore_never_lose_a_value() {
    let v = Vault::new("restore");
    v.ok(&["entry", "add", "GitHub", "--key", "value-one-111111"]);
    v.ok(&["entry", "rotate", "GitHub", "--key", "value-two-222222"]);
    assert_eq!(v.reveal_field("GitHub", "api_key"), "value-two-222222");
    let hist = v.ok(&["entry", "history", "GitHub"]);
    assert_eq!(hist["count"], 1, "the replaced value is kept: {hist}");

    let restored = v.ok(&["entry", "restore", "GitHub", "1"]);
    assert_eq!(restored["version"], 1);
    assert_eq!(v.reveal_field("GitHub", "api_key"), "value-one-111111");

    // The value the restore replaced is itself kept: nothing is destroyed.
    let after = v.ok(&["entry", "history", "GitHub"]);
    let revealed = v
        .run_with(&["--reveal"], &["entry", "history", "GitHub"])
        .stdout;
    assert!(
        revealed.contains("value-two-222222"),
        "a restore must not destroy the value it replaces: {after}"
    );
}

#[test]
fn renaming_to_a_name_in_use_is_refused_and_changes_nothing() {
    let v = Vault::new("renameconflict");
    v.ok(&["entry", "add", "One", "--key", "k-1111111"]);
    v.ok(&["entry", "add", "Two", "--key", "k-2222222"]);
    let o = v.run(&["entry", "rename", "One", "Two"]);
    assert_eq!(o.code, 6, "{}", o.all());
    assert_eq!(names(&v, &[]), vec!["One", "Two"]);
}

#[test]
fn a_value_from_a_template_is_a_new_entry_not_an_overwrite() {
    let v = Vault::new("presetdup");
    v.ok(&[
        "entry",
        "add",
        "Gh",
        "--preset",
        "github-pat",
        "--key",
        "k-1234567890",
    ]);
    let again = v.run(&[
        "entry",
        "add",
        "Gh",
        "--preset",
        "github-pat",
        "--key",
        "k-other-99999",
    ]);
    assert_eq!(again.code, 6, "{}", again.all());
    assert_eq!(v.reveal_field("Gh", "api_key"), "k-1234567890");
}
