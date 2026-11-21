// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! R39 slice 1, `vault-core/src/sites.rs`: the edges of address matching, kinds and families.
//! What a stored address means is what decides where a credential is offered, so each rule is
//! pinned by what a person would expect, not by how the code is arranged.

use serde_json::json;
use vault_core::sites::*;

fn site(s: &str) -> Site {
    parse_site(s).unwrap()
}

/// An address written without its port means the port its scheme uses, for every scheme the
/// program knows, and a different port is a different service.
#[test]
fn a_service_scheme_without_a_port_means_its_usual_port() {
    for (scheme, port) in [
        ("http", 80),
        ("https", 443),
        ("ssh", 22),
        ("sftp", 22),
        ("ftp", 21),
        ("postgres", 5432),
        ("postgresql", 5432),
        ("mysql", 3306),
        ("mariadb", 3306),
        ("redis", 6379),
        ("rediss", 6379),
        ("mongodb", 27017),
    ] {
        let stored = site(&format!("{scheme}://db.example.com"));
        let at = |p: u16| site(&format!("{scheme}://db.example.com:{p}"));
        assert!(
            site_matches(&stored, Mode::Exact, &at(port)),
            "{scheme} without a port is {port}"
        );
        assert!(
            !site_matches(&stored, Mode::Exact, &at(port + 1)),
            "{scheme}:{} is another service",
            port + 1
        );
    }
    // A scheme with no usual port has none to assume: only the same spelling matches.
    let odd = site("gemini://capsule.example.org");
    assert!(site_matches(&odd, Mode::Exact, &odd));
    assert!(!site_matches(
        &odd,
        Mode::Exact,
        &site("gemini://capsule.example.org:1966")
    ));
}

#[test]
fn an_address_needs_a_host_and_a_sensible_port() {
    for bad in [
        "",
        "https://",
        ":8080",
        "http://:80",
        "host:0",
        "host:70000",
        "host:x",
        "a b",
    ] {
        assert!(parse_site(bad).is_err(), "{bad:?} should be refused");
    }
    assert!(parse_site("https://user:pw@host")
        .unwrap_err()
        .contains("user name or password"));
}

#[test]
fn every_matching_rule_is_stored_under_its_own_name_and_read_back() {
    for mode in ["exact", "any_port", "host", "never"] {
        let v = site_value("sonarr.lan:8989", Some("web"), Some(mode), " tv ", "manual").unwrap();
        assert_eq!(v["mode"], mode);
        assert_eq!(v["label"], "tv", "the label is trimmed");
        let mut entry = json!({ "provider": "Sonarr" });
        assert!(add_site(&mut entry, v));
        let read = entry_sites(&entry);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].kind, "web");
        assert_eq!(read[0].label, "tv");
    }
    let e = site_value("x.test", None, Some("sometimes"), "", "manual").unwrap_err();
    assert!(e.contains("is not a mode"), "{e}");
    let e = site_value("x.test", Some("shrine"), None, "", "manual").unwrap_err();
    assert!(e.contains("is not a site kind"), "{e}");
}

#[test]
fn a_site_nobody_named_a_kind_for_gets_the_obvious_one() {
    assert_eq!(guess_kind(&site("192.168.1.20:8989")), "local");
    assert_eq!(guess_kind(&site("https://api.example.com")), "api");
    assert_eq!(guess_kind(&site("https://eu.api.example.com")), "api");
    assert_eq!(guess_kind(&site("https://www.example.com")), "web");
}

#[test]
fn an_old_api_url_counts_as_one_more_address_and_a_broken_stored_one_is_skipped() {
    let entry = json!({
        "provider": "Old", "secretType": "api_key",
        "api_url": "https://api.old.example",
        "sites": [ { "kind": "web" }, { "url": "not an address" }, { "url": "https://old.example" } ]
    });
    let read = entry_sites(&entry);
    let urls: Vec<String> = read.iter().map(|s| s.site.display()).collect();
    assert_eq!(urls, vec!["https://old.example", "https://api.old.example"]);
    assert_eq!(read[1].source, "api_url");
    assert_eq!(read[1].kind, "api");
    // A password's api_url is a login page.
    let pw = json!({ "secretType": "password", "api_url": "https://login.example" });
    assert_eq!(entry_sites(&pw)[0].kind, "web");
}

#[test]
fn removing_the_last_address_removes_the_list() {
    let mut entry = json!({ "provider": "X", "sites": [] });
    assert!(add_site(
        &mut entry,
        site_value("a.test", None, None, "", "manual").unwrap()
    ));
    assert!(!add_site(
        &mut entry,
        site_value("a.test", None, None, "", "manual").unwrap()
    ));
    assert!(remove_site(&mut entry, "a.test").unwrap());
    assert!(!remove_site(&mut entry, "a.test").unwrap(), "already gone");
    assert!(entry.get("sites").is_none(), "{entry}");
}

#[test]
fn a_family_name_is_short_text_and_hosts_are_added_once() {
    let mut vault = json!({});
    for bad in ["", "   ", &"n".repeat(65), "bad\nname"] {
        let e = family_add(&mut vault, bad, &["a.test".into()]).unwrap_err();
        assert!(e.contains("1 to 64 characters"), "{bad:?}: {e}");
    }
    assert!(
        vault.get("site_families").is_none(),
        "a refused name stores nothing"
    );
    family_add(
        &mut vault,
        "media",
        &["a.test".into(), "https://A.test/x".into(), "b.test".into()],
    )
    .unwrap();
    // Adding to an existing family extends it and skips a host it already has.
    family_add(&mut vault, "media", &["b.test".into(), "c.test".into()]).unwrap();
    let fam = families(&vault);
    assert_eq!(fam.len(), 1);
    assert_eq!(fam[0].hosts, vec!["a.test", "b.test", "c.test"]);
    // A bad host refuses the whole call.
    assert!(family_add(&mut vault, "media", &["a b".into()]).is_err());
    assert_eq!(families(&vault)[0].hosts.len(), 3);
}

#[test]
fn removing_from_a_family_says_whether_anything_changed() {
    let mut vault = json!({});
    assert!(
        !family_remove(&mut vault, "media", None).unwrap(),
        "no families at all"
    );
    family_add(&mut vault, "media", &["a.test".into(), "b.test".into()]).unwrap();
    assert!(!family_remove(&mut vault, "nope", Some("a.test")).unwrap());
    assert!(!family_remove(&mut vault, "media", Some("zzz.test")).unwrap());
    assert!(family_remove(&mut vault, "media", Some("a.test")).unwrap());
    assert_eq!(families(&vault)[0].hosts, vec!["b.test"]);
    assert!(family_remove(&mut vault, "media", None).unwrap());
    assert!(families(&vault).is_empty());
    assert!(family_remove(&mut vault, "media", Some("not an address")).is_err());
}
