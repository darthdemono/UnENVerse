// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv env --refs` (R30): the project's `.env` with every value left as its `${Entry}` pointer.
//! A stolen copy holds no secret, `unv render` turns it back into the real file, and a field that
//! holds a literal is refused by name instead of being copied into the file.

mod common;
use common::Vault;

const TOKEN: &str = "tok-github-plain-value-7731";

fn project(v: &Vault) {
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        TOKEN,
        "--username",
        "octocat",
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
        "TOKEN=${GitHub}",
        "USER=${GitHub/username}",
    ]);
}

#[test]
fn the_ref_only_file_holds_pointers_and_no_value() {
    let v = Vault::new("env-refs");
    project(&v);
    let o = v.run(&["env", "Web", "--refs"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(o.stdout.contains("TOKEN=${GitHub}"), "{}", o.stdout);
    assert!(o.stdout.contains("USER=${GitHub/username}"), "{}", o.stdout);
    assert!(!o.all().contains(TOKEN), "a stolen copy must hold no value");
    assert!(!o.all().contains("octocat"), "{}", o.all());
}

#[test]
fn rendering_the_ref_only_file_gives_back_the_real_values() {
    let v = Vault::new("env-refs-roundtrip");
    project(&v);
    let path = v.db().with_file_name("refs.env");
    let o = v.run(&["env", "Web", "--refs", "--out", path.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(!written.contains(TOKEN), "{written}");
    let r = v.run(&["render", path.to_str().unwrap(), "--reveal"]);
    assert_eq!(r.code, 0, "{}", r.all());
    assert!(r.stdout.contains(&format!("TOKEN={TOKEN}")), "{}", r.stdout);
    assert!(r.stdout.contains("USER=octocat"), "{}", r.stdout);
}

#[test]
fn a_literal_value_is_refused_by_name_and_never_written() {
    let v = Vault::new("env-refs-literal");
    project(&v);
    v.ok(&[
        "project",
        "chunk",
        "set",
        "Web",
        "app",
        "PLAIN=hunter2-literal-in-the-chunk",
    ]);
    let path = v.db().with_file_name("refs-literal.env");
    let o = v.run(&["env", "Web", "--refs", "--out", path.to_str().unwrap()]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert!(o.all().contains("app/PLAIN"), "{}", o.all());
    assert!(!o.all().contains("hunter2-literal"), "{}", o.all());
    assert!(
        !path.exists(),
        "nothing may be written when a literal is found"
    );
}

const API: &str = "plain-api-value-for-the-env-file-5519";

fn env_file(v: &Vault, name: &str) -> std::path::PathBuf {
    let path = v.db().with_file_name(name);
    std::fs::write(
        &path,
        format!("# service settings\nAPI_KEY={API}\nEMPTY=\nexport REGION=eu-west-1\nALREADY=${{GitHub}}\n"),
    )
    .unwrap();
    path
}

#[test]
fn migrating_moves_values_into_the_vault_and_leaves_pointers_in_the_file() {
    let v = Vault::new("env-migrate");
    project(&v);
    let path = env_file(&v, "migrate.env");
    let o = v.run(&["env", "Web", "--migrate", path.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains(API), "{}", o.all());
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("# service settings"), "{after}");
    assert!(after.contains("API_KEY=${API_KEY}"), "{after}");
    assert!(after.contains("REGION=${REGION}"), "{after}");
    assert!(after.contains("ALREADY=${GitHub}"), "{after}");
    assert!(after.contains("EMPTY="), "{after}");
    assert!(
        !after.contains(API) && !after.contains("eu-west-1"),
        "{after}"
    );
    // The vault now holds what the file held.
    assert_eq!(v.reveal_field("API_KEY", "api_key"), API);
    assert_eq!(v.reveal_field("REGION", "api_key"), "eu-west-1");
    // And the rewritten file renders back to the original values.
    let r = v.run(&["render", path.to_str().unwrap(), "--reveal"]);
    assert_eq!(r.code, 0, "{}", r.all());
    assert!(r.stdout.contains(&format!("API_KEY={API}")), "{}", r.stdout);
}

#[test]
fn a_dry_run_names_the_keys_and_changes_neither_the_file_nor_the_vault() {
    let v = Vault::new("env-migrate-dry");
    project(&v);
    let path = env_file(&v, "dry.env");
    let before = std::fs::read_to_string(&path).unwrap();
    let o = v.run_with(
        &["--dry-run"],
        &["env", "Web", "--migrate", path.to_str().unwrap()],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        o.all().contains("API_KEY") && o.all().contains("REGION"),
        "{}",
        o.all()
    );
    assert!(!o.all().contains(API), "{}", o.all());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    let list = v.run(&["list"]);
    assert!(!list.all().contains("API_KEY"), "{}", list.all());
}

#[test]
fn a_file_that_cannot_be_rewritten_line_by_line_is_refused_untouched() {
    let v = Vault::new("env-migrate-multiline");
    project(&v);
    let path = v.db().with_file_name("multi.env");
    let original = format!("KEY={API}\\\ncontinued\n");
    std::fs::write(&path, &original).unwrap();
    let o = v.run(&["env", "Web", "--migrate", path.to_str().unwrap()]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert!(
        !v.run(&["list"]).all().contains("KEY"),
        "nothing may be imported"
    );
}
