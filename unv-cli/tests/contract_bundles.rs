// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Bundles ("Web sessions, composites and bundles", README): one card holding
//! several whole entries. Members stay ordinary entries, so unbundling is
//! lossless; `dissolve` and `remove` delete nothing, `delete` is the one verb
//! that does; and a `${bundle:Name/slot/field}` reference resolves to the
//! member's field.

mod common;
use common::Vault;
use serde_json::Value;

fn seeded(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&[
        "entry",
        "add",
        "Spotify",
        "--key",
        "spotify-password-123",
        "--type",
        "password",
        "--username",
        "me",
    ]);
    v.ok(&["entry", "add", "SpotifyApp", "--key", "spotify-app-key-123"]);
    v
}

fn bundle_names(v: &Vault) -> Vec<String> {
    let o = v.ok(&["bundle", "ls"]);
    o["bundles"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|b| b["name"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn entry_names(v: &Vault) -> Vec<String> {
    let mut n: Vec<String> = v.ok(&["list"])["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["secretType"] != "bundle")
        .map(|e| e["provider"].as_str().unwrap().to_string())
        .collect();
    n.sort();
    n
}

#[test]
fn a_bundle_holds_existing_entries_in_named_slots_and_leaves_them_ordinary() {
    let v = seeded("bundle");
    let made = v.run(&[
        "bundle",
        "new",
        "Spotify account",
        "--member",
        "login=Spotify",
        "--member",
        "app=SpotifyApp",
    ]);
    assert_eq!(made.code, 0, "{}", made.all());
    assert_eq!(bundle_names(&v), vec!["Spotify account"]);
    let ls = v.ok(&["bundle", "ls"]).to_string();
    assert!(
        ls.contains("login") && ls.contains("app"),
        "the slots are listed: {ls}"
    );

    // Members are still entries in their own right: readable, with their values.
    assert_eq!(v.reveal_field("Spotify", "api_key"), "spotify-password-123");
    assert_eq!(
        v.reveal_field("SpotifyApp", "api_key"),
        "spotify-app-key-123"
    );
}

#[test]
fn a_bundle_cannot_take_an_entry_that_does_not_exist() {
    let v = seeded("bundlemissing");
    let o = v.run(&["bundle", "new", "Broken", "--member", "login=NoSuchEntry"]);
    assert_eq!(o.code, 3, "{}", o.all());
    assert!(
        bundle_names(&v).is_empty(),
        "a refused bundle must not be half-created"
    );
}

#[test]
fn add_and_remove_attach_and_detach_without_touching_the_entry() {
    let v = seeded("bundleaddremove");
    v.ok(&["bundle", "new", "Acct", "--member", "login=Spotify"]);
    v.ok(&["bundle", "add", "Acct", "SpotifyApp", "--slot", "app"]);
    assert!(v.ok(&["bundle", "ls"]).to_string().contains("app"));

    v.ok(&["bundle", "remove", "Acct", "app"]);
    assert!(
        !v.ok(&["bundle", "ls"]).to_string().contains("\"app\""),
        "the slot is gone"
    );
    assert_eq!(
        entry_names(&v),
        vec!["Spotify", "SpotifyApp"],
        "detaching deletes no entry"
    );
    assert_eq!(
        v.reveal_field("SpotifyApp", "api_key"),
        "spotify-app-key-123"
    );
}

#[test]
fn dissolve_returns_every_member_to_the_grid_and_deletes_nothing() {
    let v = seeded("bundledissolve");
    v.ok(&[
        "bundle",
        "new",
        "Acct",
        "--member",
        "login=Spotify",
        "--member",
        "app=SpotifyApp",
    ]);
    v.ok(&["bundle", "dissolve", "Acct"]);
    assert!(bundle_names(&v).is_empty(), "it is no longer a bundle");
    // The desktop does the same: the card itself stays as an ordinary entry (it may
    // hold the bundle's local variables), and both members come through intact.
    let all = v.ok(&["list"]);
    let kinds: Vec<(String, String)> = all["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["provider"].as_str().unwrap().to_string(),
                e["secretType"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        kinds.contains(&("Acct".into(), "env_var".into())),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&("Spotify".into(), "password".into())),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&("SpotifyApp".into(), "api_key".into())),
        "{kinds:?}"
    );
    assert_eq!(
        v.reveal_field("Spotify", "api_key"),
        "spotify-password-123",
        "unbundling is lossless"
    );
    assert_eq!(
        v.reveal_field("SpotifyApp", "api_key"),
        "spotify-app-key-123"
    );
}

#[test]
fn delete_removes_the_bundle_and_every_member_and_asks_first() {
    let v = seeded("bundledelete");
    v.ok(&[
        "bundle",
        "new",
        "Acct",
        "--member",
        "login=Spotify",
        "--member",
        "app=SpotifyApp",
    ]);
    // Destructive: refused without --yes and without a terminal, exactly like entry rm.
    let refused = v.run_no_yes(&["bundle", "delete", "Acct"]);
    assert_eq!(refused.code, 8, "{}", refused.all());
    assert_eq!(
        entry_names(&v),
        vec!["Spotify", "SpotifyApp"],
        "a refused delete changes nothing"
    );

    v.ok(&["bundle", "delete", "Acct"]);
    assert!(bundle_names(&v).is_empty());
    let left = v.run(&["list"]);
    let remaining = left.json()["data"]["entries"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(
        remaining, 0,
        "delete is the one verb that removes the members too: {}",
        left.stdout
    );
}

#[test]
fn a_bundle_reference_resolves_to_the_members_field() {
    let v = seeded("bundleref");
    v.ok(&["bundle", "new", "Acct", "--member", "login=Spotify"]);
    let tpl = v.path("t.tpl");
    std::fs::write(&tpl, "user=${bundle:Acct/login/username}\n").unwrap();
    let o = v.run_with(&["--reveal"], &["render", tpl.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        o.stdout.contains("user=me"),
        "${{bundle:Name/slot/field}} resolves to the member's field: {}",
        o.stdout
    );
}

#[test]
fn a_name_already_in_use_is_a_conflict() {
    let v = seeded("bundledup");
    v.ok(&["bundle", "new", "Acct", "--member", "login=Spotify"]);
    let o = v.run(&["bundle", "new", "Acct", "--member", "app=SpotifyApp"]);
    assert_eq!(o.code, 6, "{}", o.all());
    let _: Value = v.ok(&["bundle", "ls"]);
}
