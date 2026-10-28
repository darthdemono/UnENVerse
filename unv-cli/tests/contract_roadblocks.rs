// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Roadblocks reported while conforming a real vault with the CLI:
//!
//! * an entry id (or an unambiguous prefix) selects an entry; entries that read
//!   the same in an error message are told apart by their id;
//! * a bare provider name picks the one entry that has no key id;
//! * `--dry-run` says "Would update", not "Updated", and lists the changes;
//! * changing a role that renames the variable is shown, and refused without
//!   `--yes`;
//! * `entry names` lists variables without values, `entry lint` the gaps, and
//!   `entry set --where` fills a field on every entry that lacks it.

mod common;

use common::Vault;

fn id_of(v: &Vault, name: &str) -> String {
    let o = v.ok(&["entry", "names", name]);
    o["names"][0]["id"].as_str().unwrap().to_string()
}

#[test]
fn an_entry_id_and_its_prefix_select_the_entry() {
    let v = Vault::new("rb-id");
    v.ok(&["entry", "add", "Twin", "--key", "a", "--key-id", "one"]);
    v.ok(&["entry", "add", "Twin", "--key", "b", "--key-id", "two"]);
    let id = id_of(&v, "Twin:two");
    // Before the fix an id answered `No entry matching`.
    v.ok(&["entry", "set", &id, "--env", "production"]);
    v.ok(&["entry", "set", &id[..8], "--desc", "second"]);
    let o = v.ok(&["entry", "get", "Twin:two"]);
    assert!(o.to_string().contains("production"), "{o}");
    let o = v.ok(&["entry", "get", "Twin:one"]);
    assert!(!o.to_string().contains("production"), "{o}");
}

#[test]
fn identical_candidates_are_told_apart_by_id() {
    let v = Vault::new("rb-ambig");
    v.ok(&["entry", "add", "Same", "--key", "a", "--key-id", "k"]);
    v.ok(&[
        "entry", "add", "Same", "--key", "b", "--key-id", "k", "--label", "x",
    ]);
    let o = v.run(&["entry", "set", "Sam", "--env", "production"]);
    assert_eq!(o.code, 4, "{}", o.all());
    let c = o.json()["error"]["details"]["candidates"].to_string();
    assert!(c.contains("(id "), "{c}");
}

#[test]
fn a_bare_provider_name_picks_the_entry_without_a_key_id() {
    let v = Vault::new("rb-bare");
    v.ok(&["entry", "add", "Nextcloud", "--key", "a"]);
    v.ok(&["entry", "add", "Nextcloud", "--key", "b", "--key-id", "k"]);
    // Before the fix the entry itself was listed among the matches.
    v.ok(&["entry", "set", "Nextcloud", "--env", "production"]);
}

#[test]
fn dry_run_says_would_update_and_writes_nothing() {
    let v = Vault::new("rb-dry");
    v.ok(&["entry", "add", "Proton", "--key", "k"]);
    let o = v.run_with(
        &["--dry-run"],
        &["entry", "set", "Proton", "--env", "production"],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    let t = v.run_text(&["--dry-run", "entry", "set", "Proton", "--env", "production"]);
    assert!(t.stdout.contains("Would update 'Proton'"), "{}", t.stdout);
    assert!(
        t.stdout.contains("environment: (none) -> production"),
        "{}",
        t.stdout
    );
    assert!(!t.stdout.contains("Updated"), "{}", t.stdout);
    let g = v.ok(&["entry", "get", "Proton"]);
    assert!(!g.to_string().contains("production"), "{g}");
}

#[test]
fn a_role_change_shows_the_renamed_variable_and_needs_yes() {
    let v = Vault::new("rb-rename");
    v.ok(&["entry", "add", "Svc", "--key", "k", "--role", "token"]);
    let before = v.ok(&["entry", "names", "Svc"]);
    let old = before["names"][0]["name"].as_str().unwrap().to_string();
    // Before the fix the variable was renamed with no word of it.
    let o = v.run_no_yes(&["entry", "set", "Svc", "--role", "key"]);
    assert_eq!(o.code, 8, "{}", o.all());
    let still = v.ok(&["entry", "names", "Svc"]);
    assert_eq!(still["names"][0]["name"].as_str().unwrap(), old);
    let t = v.run_text(&["entry", "set", "Svc", "--role", "key"]);
    assert!(
        t.stdout.contains(&format!("variable {old} -> ")),
        "{}",
        t.stdout
    );
}

#[test]
fn names_never_print_a_value() {
    let v = Vault::new("rb-names");
    v.ok(&["entry", "add", "Svc", "--key", "sekret-value-123"]);
    let o = v.run(&["entry", "names"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains("sekret-value-123"));
    assert!(o.stdout.contains("SVC"), "{}", o.stdout);
}

#[test]
fn lint_lists_gaps_and_set_where_closes_them() {
    let v = Vault::new("rb-lint");
    v.ok(&["entry", "add", "A", "--key", "k", "--env", "production"]);
    v.ok(&["entry", "add", "B", "--key", "k"]);
    let o = v.run(&["entry", "lint", "--require", "environment"]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert_eq!(o.json()["error"]["details"]["environment"][0], "B");
    v.ok(&[
        "entry",
        "set",
        "--where",
        "environment=missing",
        "--env",
        "staging",
    ]);
    let o = v.run(&["entry", "lint", "--require", "environment"]);
    assert_eq!(o.code, 0, "{}", o.all());
    // A is untouched: the filter only matched the entry that lacked the field.
    let a = v.ok(&["entry", "get", "A"]);
    assert!(a.to_string().contains("production"), "{a}");
}
