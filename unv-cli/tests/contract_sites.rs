// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv site`: several addresses per entry, loopback spellings that match each other, a port that
//! stays significant, and families of hosts that share one entry (R40).

mod common;

use common::Vault;

fn names(v: &Vault, origin: &str) -> Vec<String> {
    let out = v.run(&["site", "match", origin]).json();
    out["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn an_entry_answers_on_every_loopback_spelling_but_only_on_its_port() {
    let v = Vault::new("sites-loopback");
    v.ok(&["entry", "add", "Sonarr", "--key", "k"]);
    v.ok(&["entry", "add", "Radarr", "--key", "k"]);
    v.ok(&["site", "add", "Sonarr", "localhost:8989"]);
    v.ok(&[
        "site",
        "add",
        "Radarr",
        "localhost:7878",
        "--label",
        "movies",
    ]);
    assert_eq!(names(&v, "http://127.0.0.1:8989"), ["Sonarr"]);
    assert_eq!(names(&v, "http://[::1]:7878"), ["Radarr"]);
    assert!(names(&v, "http://127.0.0.1:9999").is_empty());
    // A public host is never matched over plain http by a stored bare address.
    v.ok(&["site", "add", "Sonarr", "tv.example.org"]);
    assert!(names(&v, "http://tv.example.org").is_empty());
    assert_eq!(names(&v, "https://tv.example.org"), ["Sonarr"]);
}

#[test]
fn sites_are_listed_added_once_and_removed_and_a_credential_in_an_address_is_refused() {
    let v = Vault::new("sites-list");
    v.ok(&[
        "entry",
        "add",
        "Acme",
        "--key",
        "k",
        "--url",
        "https://login.acme.test",
    ]);
    v.ok(&["site", "add", "Acme", "localhost:8080", "--kind", "local"]);
    let again = v.run(&["site", "add", "Acme", "LOCALHOST:8080"]).json();
    assert_eq!(again["data"]["added"], false);
    let listed = v.run(&["site", "ls", "Acme"]).json();
    let urls: Vec<&str> = listed["data"]["sites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["url"].as_str().unwrap())
        .collect();
    assert!(urls.contains(&"localhost:8080"));
    assert!(
        urls.contains(&"https://login.acme.test"),
        "api_url counts as a site"
    );
    assert_eq!(
        v.run(&["site", "add", "Acme", "https://u:p@x.test"]).code,
        10
    );
    assert_eq!(
        v.run(&["site", "add", "Acme", "x.test", "--kind", "blog"])
            .code,
        10
    );
    v.ok(&["site", "rm", "Acme", "localhost:8080"]);
    assert_eq!(v.run(&["site", "rm", "Acme", "localhost:8080"]).code, 3);
}

#[test]
fn a_family_offers_one_entry_on_every_host_and_not_on_a_lookalike() {
    let v = Vault::new("sites-family");
    v.ok(&["entry", "add", "MetaBrainz", "--key", "k"]);
    v.ok(&[
        "site",
        "family",
        "add",
        "metabrainz",
        "musicbrainz.org",
        "listenbrainz.org",
    ]);
    assert_eq!(
        v.run(&["site", "family", "use", "MetaBrainz", "nope"]).code,
        3
    );
    v.ok(&["site", "family", "use", "MetaBrainz", "metabrainz"]);
    assert_eq!(names(&v, "https://listenbrainz.org"), ["MetaBrainz"]);
    assert!(names(&v, "https://evilmusicbrainz.org").is_empty());
    v.ok(&["site", "family", "rm", "metabrainz", "listenbrainz.org"]);
    assert!(names(&v, "https://listenbrainz.org").is_empty());
    let fams = v.run(&["site", "family", "ls"]).json();
    assert_eq!(fams["data"]["families"][0]["hosts"][0], "musicbrainz.org");
    v.ok(&["site", "family", "use", "MetaBrainz", ""]);
    v.ok(&["site", "family", "rm", "metabrainz"]);
    assert_eq!(v.run(&["site", "family", "rm", "metabrainz"]).code, 3);
}

#[test]
fn the_addresses_are_public_metadata_in_a_redacted_listing() {
    let v = Vault::new("sites-redact");
    v.ok(&["entry", "add", "Acme", "--key", "k"]);
    v.ok(&["site", "add", "Acme", "localhost:8080"]);
    v.ok(&["site", "family", "add", "fam", "acme.test"]);
    v.ok(&["site", "family", "use", "Acme", "fam"]);
    let got = v.run(&["get", "Acme"]).all();
    assert!(got.contains("localhost:8080"), "{got}");
    assert!(
        got.contains("\"site_family\":\"fam\"") || got.contains("\"site_family\": \"fam\""),
        "{got}"
    );
}

/// `--site` on `entry add` and `entry set` stores the same addresses `unv site add` does: repeatable,
/// the same spelling twice stays one, and a bad address is refused before anything is written.
#[test]
fn the_site_flag_adds_addresses_on_add_and_set() {
    let v = Vault::new("sites-flag");
    v.ok(&[
        "entry",
        "add",
        "Sonarr",
        "--key",
        "k",
        "--site",
        "localhost:8989",
        "--site",
        "tv.example.org",
    ]);
    assert_eq!(names(&v, "http://127.0.0.1:8989"), ["Sonarr"]);
    assert_eq!(names(&v, "https://tv.example.org"), ["Sonarr"]);
    // The same address again is not stored twice. (Another spelling of it is a separate row until
    // the "same address, other spelling" hint exists.)
    v.ok(&["entry", "set", "Sonarr", "--site", "localhost:8989"]);
    let ls = v.run(&["site", "ls", "Sonarr"]).json();
    assert_eq!(ls["data"]["sites"].as_array().unwrap().len(), 2, "{ls}");
    v.ok(&["entry", "set", "Sonarr", "--site", "other.example.org"]);
    assert_eq!(names(&v, "https://other.example.org"), ["Sonarr"]);
    // A bad address is invalid input and changes nothing.
    assert_eq!(
        v.run(&["entry", "add", "Bad", "--key", "k", "--site", ""])
            .code,
        10
    );
    assert_eq!(v.run(&["entry", "get", "Bad"]).code, 3);
}

/// Families that ship with the program: an entry may name one without making it, it is offered on
/// every host of the family and not on a look-alike, `ls` shows only the vault's own unless
/// `--builtin`, and a family of the same name in the vault replaces the shipped one.
#[test]
fn a_shipped_family_can_be_named_without_being_created() {
    let v = Vault::new("sites-builtin-family");
    v.ok(&["entry", "add", "Brainz", "--key", "k"]);
    assert_eq!(
        v.run(&["site", "family", "use", "Brainz", "nope"]).code,
        3,
        "an unknown family is still refused"
    );
    v.ok(&["site", "family", "use", "Brainz", "metabrainz"]);
    assert_eq!(names(&v, "https://listenbrainz.org"), ["Brainz"]);
    assert_eq!(names(&v, "https://beta.musicbrainz.org"), ["Brainz"]);
    assert!(names(&v, "https://notmusicbrainz.org").is_empty());

    let own = v.run(&["site", "family", "ls"]).json();
    assert!(own["data"]["families"].as_array().unwrap().is_empty());
    let shipped = v.run(&["site", "family", "ls", "--builtin"]).json();
    let list = shipped["data"]["families"].as_array().unwrap();
    assert!(list.iter().any(|f| f["name"] == "metabrainz"), "{shipped}");

    // Your own family of that name wins: now only its hosts count.
    v.ok(&["site", "family", "add", "metabrainz", "only.example.org"]);
    assert!(names(&v, "https://listenbrainz.org").is_empty());
    assert_eq!(names(&v, "https://only.example.org"), ["Brainz"]);
}
