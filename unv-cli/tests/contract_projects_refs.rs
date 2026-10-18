// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Projects, chunks and `${refs}` as the README describes them ("Projects,
//! chunks and exports"), plus the cascade rules the project holds itself to:
//! a rename must never quietly break what pointed at the old name.
//!
//! * a `${ref}` is resolved by everything that exports or copies;
//! * renaming an entry rewrites every `${ref}` that points at it;
//! * a disabled chunk is excluded from every export;
//! * nested names create their parents, and a project rename carries its
//!   sub-projects and the entries linked to it.

mod common;
use common::Vault;

fn export_env(v: &Vault, project: &str) -> String {
    let o = v.run_with(
        &["--reveal"],
        &["project", "export", project, "--format", "env"],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    o.stdout
}

fn project_with_refs(v: &Vault) {
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "tok-github-123",
        "--username",
        "octocat",
    ]);
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
        "DATABASE_URL=postgres://db.internal/app",
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
        "TOKEN=${GitHub}",
        "USER=${GitHub/username}",
        "DB=${chunk:api/DATABASE_URL}",
    ]);
}

#[test]
fn every_kind_of_ref_is_resolved_when_a_project_is_exported() {
    let v = Vault::new("refs");
    project_with_refs(&v);
    let env = export_env(&v, "Web");
    assert!(
        env.contains("TOKEN=tok-github-123"),
        "${{GitHub}} is the main secret: {env}"
    );
    assert!(
        env.contains("USER=octocat"),
        "${{GitHub/username}} is a named field: {env}"
    );
    assert!(
        env.contains("DB=postgres://db.internal/app"),
        "${{chunk:api/DATABASE_URL}} is a field on another chunk: {env}"
    );
    assert!(
        !env.contains("${"),
        "a ${{...}} reaching a deploy file is a broken deploy: {env}"
    );
}

#[test]
fn a_ref_is_disambiguated_by_key_id() {
    let v = Vault::new("keyid");
    v.ok(&["entry", "add", "GitHub", "--key", "main-token-111"]);
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "deploy-token-222",
        "--key-id",
        "deploy",
    ]);
    v.ok(&["project", "add", "Web"]);
    v.ok(&[
        "project", "chunk", "add", "Web", "app", "--type", "env_file",
    ]);
    v.ok(&[
        "project",
        "chunk",
        "set",
        "Web",
        "app",
        "DEPLOY=${GitHub_deploy}",
    ]);
    let env = export_env(&v, "Web");
    assert!(env.contains("DEPLOY=deploy-token-222"), "{env}");
    assert!(
        !env.contains("main-token-111"),
        "the wrong entry was resolved: {env}"
    );
}

#[test]
fn renaming_an_entry_rewrites_every_ref_and_the_export_is_unchanged() {
    let v = Vault::new("rename");
    project_with_refs(&v);
    let before = export_env(&v, "Web");

    let renamed = v.ok(&["entry", "rename", "GitHub", "GH2"]);
    assert_eq!(
        renamed["rewritten_refs"], 2,
        "the two refs that pointed at it: {renamed}"
    );

    // The structured view shows the new name, the old one is gone.
    let doc = v
        .run(&["project", "export", "Web", "--format", "json"])
        .stdout;
    assert!(
        doc.contains("${GH2}") && doc.contains("${GH2/username}"),
        "{doc}"
    );
    assert!(
        !doc.contains("${GitHub"),
        "a ref still points at the old name: {doc}"
    );

    // And the deploy output is exactly what it was.
    assert_eq!(
        export_env(&v, "Web"),
        before,
        "a rename must not change what is deployed"
    );
}

#[test]
fn a_disabled_chunk_is_excluded_from_every_export_until_it_is_enabled() {
    let v = Vault::new("disable");
    project_with_refs(&v);
    assert!(export_env(&v, "Web").contains("DATABASE_URL="));

    v.ok(&["project", "chunk", "disable", "Web", "api"]);
    let env = export_env(&v, "Web");
    assert!(
        !env.contains("DATABASE_URL="),
        "a disabled chunk must not be exported: {env}"
    );
    // The structured JSON view is a dump of the project, not a deploy file: it
    // keeps the chunk but says it is disabled, so nothing reading it is misled.
    let doc = v
        .run(&["project", "export", "Web", "--format", "json"])
        .json();
    let api = doc["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "api")
        .expect("the chunk is still part of the project");
    assert_eq!(api["disabled"], true, "{api}");

    v.ok(&["project", "chunk", "enable", "Web", "api"]);
    assert!(export_env(&v, "Web").contains("DATABASE_URL="));
}

#[test]
fn a_nested_project_name_creates_its_parents() {
    let v = Vault::new("nested");
    let created = v.ok(&["project", "add", "Acme/Web"]);
    assert_eq!(created["id"], "acme/web");
    let list = v.ok(&["project", "ls"]).to_string();
    assert!(
        list.contains("\"Acme\""),
        "\"Acme/Web\" creates \"Acme\" if absent: {list}"
    );
}

#[test]
fn renaming_a_project_carries_its_sub_projects_and_its_entries() {
    let v = Vault::new("projrename");
    v.ok(&["project", "add", "Acme"]);
    v.ok(&["project", "add", "Acme/Web"]);
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "tok-1234567",
        "--projects",
        "acme/web",
    ]);
    assert_eq!(v.ok(&["list", "--project", "Acme/Web"])["count"], 1);

    v.ok(&["project", "rename", "Acme", "Globex"]);

    let list = v.ok(&["project", "ls"]).to_string();
    assert!(list.contains("\"Globex\""), "{list}");
    assert!(
        list.contains("Globex/Web"),
        "sub-projects follow their parent: {list}"
    );
    assert!(
        !list.contains("\"Acme"),
        "nothing may keep the old name: {list}"
    );
    assert_eq!(
        v.ok(&["list", "--project", "Globex/Web"])["count"],
        1,
        "the entry's link must follow the rename, not dangle"
    );
}

#[test]
fn deleting_a_project_promotes_its_sub_projects_and_keeps_the_entries() {
    let v = Vault::new("projrm");
    v.ok(&["project", "add", "Acme"]);
    v.ok(&["project", "add", "Acme/Web"]);
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "tok-1234567",
        "--projects",
        "acme/web",
    ]);

    v.ok(&["project", "rm", "Acme"]);

    let list = v.ok(&["project", "ls"]).to_string();
    assert!(!list.contains("\"Acme\""), "{list}");
    assert!(
        list.contains("Web"),
        "sub-projects are promoted to top level, not lost: {list}"
    );
    assert_eq!(
        v.ok(&["list"])["count"],
        1,
        "deleting a project must never delete an entry"
    );
}
