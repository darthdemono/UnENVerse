//! Structure commands ("Projects, chunks and exports", README, and the cascade
//! invariants in AGENTS.md): categories, tags and chunks.
//!
//! * a category is unique (case-insensitively), a rename carries the entries
//!   that use it and a delete removes it from them without deleting an entry;
//! * a chunk name is unique within its project, a rename rewrites every
//!   `${chunk:Name/Field}` that pointed at it, and a whole `env_file` chunk is
//!   shown masked.

mod common;
use common::Vault;
use serde_json::Value;

fn categories_of(v: &Vault, provider: &str) -> Value {
    v.ok(&["get", provider])["entries"][0]["categories"].clone()
}

fn chunk_names(v: &Vault, project: &str) -> Vec<String> {
    let mut n: Vec<String> = v.ok(&["project", "chunk", "ls", project])["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    n.sort();
    n
}

// ---- categories

#[test]
fn a_category_is_unique_ignoring_case_and_unknown_ones_are_not_found() {
    let v = Vault::new("catunique");
    assert_eq!(v.ok(&["category", "add", "infra"])["created"], true);
    let dup = v.run(&["category", "add", "Infra"]);
    assert_eq!(dup.code, 6, "{}", dup.all());
    assert_eq!(dup.error_code(), "conflict");
    for args in [
        vec!["category", "rm", "nope"],
        vec!["category", "rename", "nope", "other"],
    ] {
        let o = v.run(&args);
        assert_eq!(o.code, 3, "{args:?}: {}", o.all());
    }
}

#[test]
fn renaming_a_category_carries_its_entries() {
    let v = Vault::new("catrename");
    v.ok(&["category", "add", "infra"]);
    v.ok(&[
        "entry",
        "add",
        "One",
        "--key",
        "one-secret-123456",
        "--categories",
        "infra",
    ]);
    v.ok(&["entry", "add", "Two", "--key", "two-secret-123456"]);
    let renamed = v.ok(&["category", "rename", "infra", "platform"]);
    assert_eq!(renamed["to"], "platform");
    assert_eq!(
        categories_of(&v, "One"),
        serde_json::json!(["platform"]),
        "the entry follows the rename"
    );
    let ls = v.ok(&["category", "ls"]);
    let rows = ls["categories"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{ls}");
    assert_eq!(rows[0]["name"], "platform");
    assert_eq!(rows[0]["entry_count"], 1);
}

#[test]
fn deleting_a_category_removes_it_from_entries_and_deletes_no_entry() {
    let v = Vault::new("catrm");
    v.ok(&["category", "add", "infra"]);
    v.ok(&[
        "entry",
        "add",
        "One",
        "--key",
        "one-secret-123456",
        "--categories",
        "infra",
    ]);
    let removed = v.ok(&["category", "rm", "infra"]);
    assert_eq!(removed["deleted"], true);
    assert_eq!(
        categories_of(&v, "One"),
        serde_json::json!([]),
        "no dangling category is left on the entry"
    );
    assert_eq!(
        v.ok(&["list"])["count"],
        1,
        "deleting a category never deletes an entry"
    );
    assert_eq!(v.ok(&["category", "ls"])["count"], 0);
}

#[test]
fn tags_lists_every_tag_with_its_entry_count() {
    let v = Vault::new("tags");
    v.ok(&[
        "entry",
        "add",
        "One",
        "--key",
        "one-secret-123456",
        "--tags",
        "ci,shared",
    ]);
    v.ok(&[
        "entry",
        "add",
        "Two",
        "--key",
        "two-secret-123456",
        "--tags",
        "shared",
    ]);
    let d = v.ok(&["tags"]);
    assert_eq!(d["count"], 2, "{d}");
    let shared = d["tags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["tag"] == "shared")
        .expect("the shared tag");
    assert_eq!(shared["entry_count"], 2, "{d}");
}

// ---- chunks

fn project_with_chunks(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["project", "add", "Web"]);
    v.ok(&[
        "project", "chunk", "add", "Web", "api", "--type", "env_file",
    ]);
    v.ok(&[
        "project",
        "chunk",
        "set",
        "Web",
        "api",
        "DB_PASSWORD=hunter2-real-pass",
        "PLAIN=hello",
    ]);
    v.ok(&[
        "project", "chunk", "add", "Web", "app", "--type", "env_file",
    ]);
    v.ok(&[
        "project",
        "chunk",
        "set",
        "Web",
        "app",
        "URL=${chunk:api/PLAIN}",
    ]);
    v
}

#[test]
fn chunk_ls_and_show_describe_a_chunk_and_mask_an_env_file_whole() {
    let v = project_with_chunks("chunkshow");
    let ls = v.ok(&["project", "chunk", "ls", "Web"]);
    assert_eq!(ls["count"], 2);
    let api = ls["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "api")
        .unwrap()
        .clone();
    assert_eq!(api["fields"], 2);
    assert_eq!(api["disabled"], false);

    let shown = v.run(&["project", "chunk", "show", "Web", "api", "--raw"]);
    assert_eq!(shown.code, 0, "{}", shown.all());
    assert!(
        !shown.all().contains("hunter2-real-pass") && !shown.all().contains("=hello"),
        "{}",
        shown.all()
    );
    assert!(
        shown.stdout.contains("sha256:"),
        "even an unflagged value is masked in an env_file: {}",
        shown.stdout
    );
    let revealed = v.run_with(&["--reveal"], &["project", "chunk", "show", "Web", "api"]);
    assert_eq!(revealed.code, 0, "{}", revealed.all());
    let data = &revealed.json()["data"];
    assert_eq!(data["redacted"], false, "{data}");
    assert!(
        data["text"]
            .as_str()
            .unwrap()
            .contains("DB_PASSWORD=hunter2-real-pass"),
        "--reveal opts back in: {data}"
    );
}

#[test]
fn renaming_a_chunk_rewrites_the_cross_chunk_references_that_pointed_at_it() {
    let v = project_with_chunks("chunkrename");
    let renamed = v.ok(&["project", "chunk", "rename", "Web", "api", "backend"]);
    assert_eq!(renamed["rewritten_refs"], 1, "{renamed}");
    assert_eq!(chunk_names(&v, "Web"), vec!["app", "backend"]);

    // The reference still resolves, now through the new name.
    let doc = v
        .run(&["project", "export", "Web", "--format", "json"])
        .json();
    let app = doc["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "app")
        .unwrap()
        .clone();
    assert_eq!(app["fields"][0]["value"], "${chunk:backend/PLAIN}", "{app}");
    let env = v.run_with(
        &["--reveal"],
        &["project", "export", "Web", "--format", "env"],
    );
    assert!(
        env.stdout.contains("URL=hello"),
        "the deploy output is unchanged by a rename: {}",
        env.stdout
    );
}

#[test]
fn a_chunk_cannot_be_renamed_onto_a_name_already_used_in_its_project() {
    let v = project_with_chunks("chunkdup");
    let o = v.run(&["project", "chunk", "rename", "Web", "app", "api"]);
    assert_eq!(o.code, 6, "the rule chunk add applies: {}", o.all());
    assert_eq!(o.error_code(), "conflict");
    assert_eq!(
        chunk_names(&v, "Web"),
        vec!["api", "app"],
        "a refused rename changes nothing"
    );
}

#[test]
fn unset_removes_named_fields_and_a_missing_key_is_not_an_error() {
    let v = project_with_chunks("chunkunset");
    assert_eq!(
        v.ok(&["project", "chunk", "unset", "Web", "api", "PLAIN"])["removed"],
        1
    );
    assert_eq!(
        v.ok(&["project", "chunk", "unset", "Web", "api", "NOT_THERE"])["removed"],
        0
    );
    let ls = v.ok(&["project", "chunk", "ls", "Web"]);
    let api = ls["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "api")
        .unwrap()
        .clone();
    assert_eq!(api["fields"], 1, "only DB_PASSWORD is left");
}

#[test]
fn a_chunk_can_be_disabled_and_enabled_again() {
    let v = project_with_chunks("chunktoggle");
    assert_eq!(
        v.ok(&["project", "chunk", "disable", "Web", "api"])["disabled"],
        true
    );
    let off = v.ok(&["project", "chunk", "ls", "Web"]);
    assert!(
        off["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "api" && c["disabled"] == true),
        "{off}"
    );
    assert_eq!(
        v.ok(&["project", "chunk", "enable", "Web", "api"])["disabled"],
        false
    );
}

#[test]
fn an_unknown_project_or_chunk_is_not_found() {
    let v = project_with_chunks("chunkmissing");
    for args in [
        vec!["project", "chunk", "ls", "Nope"],
        vec!["project", "chunk", "show", "Web", "nope"],
        vec!["project", "chunk", "rename", "Web", "nope", "x"],
        vec!["project", "chunk", "enable", "Web", "nope"],
    ] {
        let o = v.run(&args);
        assert_eq!(o.code, 3, "{args:?}: {}", o.all());
    }
}
