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
