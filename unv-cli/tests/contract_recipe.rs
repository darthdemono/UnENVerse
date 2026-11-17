// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv recipe` (R41): saving and listing recipes, samples redacted by default, a stored result that
//! is an ordinary password, rules and the default choosing a recipe, lists and facts feeding tokens,
//! and the floor refusing a weak result.

mod common;

use common::Vault;

fn reveal(v: &Vault, args: &[&str]) -> common::Out {
    v.run_with(&["--reveal"], args)
}

#[test]
fn recipes_are_saved_listed_replaced_and_removed() {
    let v = Vault::new("recipe-crud");
    // A recipe command needs a vault that exists; one entry makes it.
    v.ok(&["entry", "add", "Seed", "--key", "k"]);
    v.ok(&[
        "recipe",
        "add",
        "social",
        "{prefix}-{random:16:base62}",
        "--max",
        "40",
        "--require",
        "upper:2+digit",
    ]);
    assert_eq!(
        v.run(&["recipe", "add", "social", "{random:20}"]).code,
        6,
        "exists"
    );
    assert_eq!(v.run(&["recipe", "add", "bad", "{nope}"]).code, 10);
    assert_eq!(
        v.run(&["recipe", "add", "bad", "{random:9}", "--require", "colour"])
            .code,
        10
    );
    // Replacing without flags keeps the options.
    v.ok(&[
        "recipe",
        "add",
        "social",
        "{prefix}-{random:20:base62}",
        "--replace",
    ]);
    let shown = v.run(&["recipe", "show", "social"]).json();
    assert_eq!(shown["data"]["template"], "{prefix}-{random:20:base62}");
    assert_eq!(shown["data"]["options"]["max_len"], 40);
    assert_eq!(shown["data"]["options"]["require_upper"], 2);
    let ls = v.run(&["recipe", "ls"]).json();
    assert_eq!(ls["data"]["recipes"][0]["name"], "social");
    v.ok(&["recipe", "default", "social"]);
    assert_eq!(v.run(&["recipe", "default", "ghost"]).code, 3);
    assert_eq!(
        v.run(&["recipe", "ls"]).json()["data"]["recipes"][0]["default"],
        true
    );
    v.ok(&["recipe", "default", ""]);
    v.ok(&["recipe", "rm", "social"]);
    assert_eq!(v.run(&["recipe", "rm", "social"]).code, 3);
    assert_eq!(v.run(&["recipe", "show", "social"]).code, 3);
    let text = v.run_text(&["recipe", "ls"]).all();
    assert!(!text.contains("social"));
}

#[test]
fn samples_are_redacted_unless_revealed_and_say_how_strong_they_are() {
    let v = Vault::new("recipe-test");
    // A recipe command needs a vault that exists; one entry makes it.
    v.ok(&["entry", "add", "Seed", "--key", "k"]);
    let hidden = v
        .run(&["recipe", "test", "{random:20:base62}", "--count", "3"])
        .json();
    assert_eq!(hidden["data"]["samples"][0]["redacted"], true);
    assert_eq!(hidden["data"]["samples"].as_array().unwrap().len(), 3);
    assert!(hidden["data"]["bits"].as_f64().unwrap() > 100.0);
    assert_eq!(hidden["data"]["rebuildable_from_memory"], false);
    let shown = reveal(&v, &["recipe", "test", "{random:20:base62}"]).json();
    let sample = shown["data"]["samples"][0].as_str().unwrap();
    assert_eq!(sample.len(), 20);
    assert!(sample.chars().all(|c| c.is_ascii_alphanumeric()));
    // Too weak for the default floor, then allowed with a lower one.
    assert_eq!(v.run(&["recipe", "test", "{digits:4}"]).code, 10);
    v.ok(&["recipe", "test", "{digits:4}", "--floor", "8"]);
    assert_eq!(
        v.run(&[
            "recipe",
            "test",
            "{random:16}",
            "--min",
            "30",
            "--policy",
            "refuse"
        ])
        .code,
        10
    );
}

#[test]
fn an_applied_recipe_is_stored_as_an_ordinary_password_with_a_public_record() {
    let v = Vault::new("recipe-apply");
    v.ok(&[
        "entry",
        "add",
        "Instagram",
        "--type",
        "password",
        "--key",
        "old-password-1",
        "--username",
        "joy",
    ]);
    v.ok(&[
        "recipe",
        "add",
        "social",
        "{provider|lower}.{created:YYYY}.{random:20:base62}",
        "--require",
        "upper+digit",
    ]);
    assert_eq!(
        v.run(&["recipe", "apply", "Instagram"]).code,
        3,
        "no recipe applies yet"
    );
    v.ok(&["recipe", "rule", "add", "provider", "instagram", "social"]);
    let done = v.run(&["recipe", "apply", "Instagram"]).json();
    assert_eq!(done["data"]["recipe"], "social");
    assert_eq!(done["data"]["why"], "rule: provider instagram");
    assert_eq!(done["data"]["value"]["redacted"], true);
    // The whole password is stored: no fragment, no marker.
    let pw = v.reveal_field("Instagram", "api_key");
    assert!(pw.starts_with("instagram."), "a password from the recipe");
    assert!(pw.len() > 30);
    assert_ne!(pw, "old-password-1");
    // The record of how it was made holds no secret.
    let got = v.run(&["get", "Instagram"]).all();
    assert!(!got.contains(&pw));
    assert!(got.contains("generated_by"), "{got}");
    // A second run makes a different password and counts up.
    v.ok(&["recipe", "apply", "Instagram"]);
    let again = v.reveal_field("Instagram", "api_key");
    assert_ne!(again, pw);
    let test = v.run(&["recipe", "rule", "test", "Instagram"]).json();
    assert_eq!(test["data"]["recipe"], "social");
}

#[test]
fn rules_and_the_default_choose_the_recipe_and_none_switches_it_off() {
    let v = Vault::new("recipe-rules");
    v.ok(&["entry", "add", "Sonarr", "--key", "k"]);
    v.ok(&["recipe", "add", "a", "{random:20}"]);
    v.ok(&["recipe", "add", "b", "{random:24}"]);
    let none = v.run(&["recipe", "rule", "test", "Sonarr"]).json();
    assert!(none["data"]["recipe"].is_null());
    v.ok(&["recipe", "default", "a"]);
    assert_eq!(
        v.run(&["recipe", "rule", "test", "Sonarr"]).json()["data"]["why"],
        "vault default"
    );
    v.ok(&["recipe", "rule", "add", "type", "api_key", "b"]);
    assert_eq!(
        v.run(&["recipe", "rule", "test", "Sonarr"]).json()["data"]["recipe"],
        "b"
    );
    v.ok(&["recipe", "rule", "add", "provider", "sonarr", "none"]);
    let rules = v.run(&["recipe", "rule", "ls"]).json();
    assert_eq!(rules["data"]["rules"].as_array().unwrap().len(), 2);
    v.ok(&["recipe", "rule", "rm", "1"]);
    assert!(v.run(&["recipe", "rule", "test", "Sonarr"]).json()["data"]["recipe"].is_null());
    assert_eq!(v.run(&["recipe", "rule", "rm", "9"]).code, 3);
    assert_eq!(
        v.run(&["recipe", "rule", "add", "provider", "x", "ghost"])
            .code,
        10
    );
    // Removing a recipe removes the rules that name it.
    v.ok(&["recipe", "rm", "b"]);
    v.ok(&["recipe", "rm", "a"]);
    assert!(
        v.run(&["recipe", "rule", "ls"]).json()["data"]["rules"]
            .as_array()
            .unwrap()
            .len()
            <= 1
    );
}

#[test]
fn your_lists_and_facts_feed_the_tokens_and_private_facts_are_redacted() {
    let v = Vault::new("recipe-lists");
    // A recipe command needs a vault that exists; one entry makes it.
    v.ok(&["entry", "add", "Seed", "--key", "k"]);
    let f = v.dir.join("words.txt");
    std::fs::write(&f, "kimchi\nnaan\ntacos\n\nramen\n").unwrap();
    v.ok(&[
        "recipe",
        "list",
        "add",
        "meals",
        "--file",
        f.to_str().unwrap(),
        "--private",
    ]);
    v.ok(&["recipe", "list", "add", "pair", "a", "b"]);
    assert_eq!(v.run(&["recipe", "list", "add", "x", "only"]).code, 10);
    v.ok(&["recipe", "fact", "set", "pet", "rex"]);
    v.ok(&["recipe", "fact", "set", "street", "elm", "--public"]);
    assert_eq!(v.run(&["recipe", "fact", "set", "Bad Name", "x"]).code, 10);
    let facts = v.run(&["recipe", "fact", "ls"]).json();
    let rows = facts["data"]["facts"].as_array().unwrap();
    assert_eq!(
        rows[0]["value"]["redacted"], true,
        "a private fact is redacted"
    );
    assert_eq!(rows[1]["value"], "elm");
    let out = reveal(
        &v,
        &[
            "recipe",
            "test",
            "{list:meals:2:/}-{var:pet}-{var:street}-{random:12:base62}",
        ],
    )
    .json();
    let sample = out["data"]["samples"][0].as_str().unwrap();
    assert!(sample.contains("-rex-elm-"), "{sample}");
    let first = sample.split('-').next().unwrap();
    assert!(first
        .split('/')
        .all(|w| ["kimchi", "naan", "tacos", "ramen"].contains(&w)));
    assert!(out["data"]["private_bits"].as_f64().unwrap() >= 4.0);
    let lists = v.run(&["recipe", "list", "ls"]).json();
    let names: Vec<&str> = lists["data"]["lists"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"meals") && names.contains(&"prefix"));
    v.ok(&["recipe", "list", "rm", "pair"]);
    assert_eq!(v.run(&["recipe", "list", "rm", "pair"]).code, 3);
    v.ok(&["recipe", "fact", "rm", "pet"]);
    assert_eq!(v.run(&["recipe", "fact", "rm", "pet"]).code, 3);
    assert_eq!(
        v.run(&["recipe", "test", "{var:pet}", "--floor", "0"]).code,
        10
    );
    assert_eq!(v.run(&["recipe", "test", "{list:nope}"]).code, 10);
}

#[test]
fn the_token_list_comes_from_the_registry_and_can_be_narrowed_by_group() {
    let v = Vault::new("recipe-tokens");
    let all = v.run(&["recipe", "tokens"]).json();
    let n = all["data"]["tokens"].as_array().unwrap().len();
    assert!(n > 80, "{n}");
    let lengtheners = v
        .run(&["recipe", "tokens", "--group", "lengtheners"])
        .json();
    let ids: Vec<&str> = lengtheners["data"]["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"expand") && ids.contains(&"family_bound"));
    let text = v.run_text(&["recipe", "tokens", "--group", "random"]).all();
    assert!(text.contains("random") && !text.contains("expand"));
}

/// `entry add --recipe` makes the password at creation: stored whole as an ordinary password with
/// the public record, and refused when another source already gave the value.
#[test]
fn entry_add_with_a_recipe_creates_the_entry_with_a_generated_password() {
    let v = Vault::new("recipe-entry-add");
    v.ok(&["entry", "add", "Seed", "--key", "k"]);
    v.ok(&["recipe", "add", "plain", "{random:24:base62}"]);
    v.ok(&[
        "entry", "add", "Mastodon", "--type", "password", "--recipe", "plain",
    ]);
    let shown = reveal(&v, &["entry", "get", "Mastodon", "--field", "api_key"]).json();
    let text = shown["data"]["value"].as_str().unwrap_or_default();
    assert_eq!(text.len(), 24, "{shown}");
    let got = v.run(&["entry", "get", "Mastodon"]).json();
    assert_eq!(got["data"]["generated_by"]["name"], "plain", "{got}");
    // An unknown recipe is not found, and no entry is left behind.
    assert_eq!(
        v.run(&["entry", "add", "Ghost", "--recipe", "nope"]).code,
        3
    );
    assert_eq!(v.run(&["entry", "get", "Ghost"]).code, 3);
    // A value from another source is refused, not silently replaced.
    assert_eq!(
        v.run(&["entry", "add", "Both", "--key", "x", "--recipe", "plain"])
            .code,
        10
    );
}
