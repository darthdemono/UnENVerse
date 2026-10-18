// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv import-vault onepassword` on the layout a real 1Password export uses: the
//! login's own username and password sit in `details.loginFields` marked by
//! `designation`, and every other field sits in a section with a value object keyed
//! by its type (`{"concealed": …}`, `{"totp": …}`, `{"url": …}`). An import that only
//! reads bare string values loses every one of those silently and reports an item
//! with no password.

mod common;

use common::Vault;
use serde_json::{json, Value};

fn export() -> Value {
    json!({ "accounts": [{
        "attrs": { "accountName": "Family" },
        "vaults": [{
            "attrs": { "name": "Personal" },
            "items": [
                { "item": {
                    "uuid": "u1", "categoryUuid": "001", "trashed": false,
                    "details": {
                        "loginFields": [
                            { "value": "alice", "id": "", "name": "username", "fieldType": "T", "designation": "username" },
                            { "value": "pw-secret-1", "id": "", "name": "password", "fieldType": "P", "designation": "password" }
                        ],
                        "notesPlain": "remember me",
                        "sections": [{ "title": "Extra", "name": "s", "fields": [
                            { "title": "one-time password", "id": "TOTP_9F", "value": { "totp": "JBSWY3DPEHPK3PXP" } },
                            { "title": "recovery code", "id": "rc", "value": { "concealed": "rec-code-123" } },
                            { "title": "support site", "id": "su", "value": { "url": "https://support.example.test" } },
                            { "title": "empty", "id": "em", "value": { "string": "" } },
                            { "title": "valid until", "id": "vu", "value": { "monthYear": 203012 } }
                        ]}],
                        "passwordHistory": []
                    },
                    "overview": { "title": "Example", "url": "https://example.test",
                                  "urls": [{ "label": "website", "url": "https://example.test" }] }
                } },
                { "item": { "uuid": "u2", "details": { "loginFields": [], "sections": [] },
                            "overview": { "title": "Empty login" } } },
                { "item": { "details": { "loginFields": [] }, "overview": { "title": "No uuid" } } }
            ]
        }]
    }]})
}

fn write_export(v: &Vault) -> String {
    let p = v.path("export.json");
    std::fs::write(&p, serde_json::to_vec(&export()).unwrap()).unwrap();
    p.to_str().unwrap().to_string()
}

fn entry(v: &Vault, name: &str) -> Value {
    let o = v.run_with(&["--reveal"], &["get", name]);
    assert_eq!(o.code, 0, "{name}: {}", o.all());
    o.json()["data"]["entries"][0].clone()
}

fn var<'a>(e: &'a Value, key: &str) -> &'a Value {
    e["extra_vars"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["key"] == key))
        .unwrap_or_else(|| panic!("no variable {key} in {e}"))
}

#[test]
fn the_logins_own_fields_and_typed_section_values_are_imported() {
    let v = Vault::new("op-layout");
    let f = write_export(&v);
    let o = v.run(&["import-vault", "onepassword", &f, "--apply"]);
    assert_eq!(o.code, 0, "{}", o.all());

    let e = entry(&v, "Example");
    assert_eq!(
        e["api_key"], "pw-secret-1",
        "the designated password is the secret"
    );
    assert_eq!(e["account_name"], "alice");
    assert_eq!(e["api_url"], "https://example.test");
    assert_eq!(e["notes"], "remember me");
    assert_eq!(
        e["totp_secret"], "JBSWY3DPEHPK3PXP",
        "a typed totp value is the seed"
    );
    let rc = var(&e, "recovery code");
    assert_eq!(rc["value"], "rec-code-123");
    assert_eq!(
        rc["public"], false,
        "a field's type is not a declaration of safety"
    );
    assert_eq!(
        var(&e, "support site")["value"],
        "https://support.example.test"
    );
    assert_eq!(var(&e, "valid until")["value"], "203012");
    assert!(
        e["extra_vars"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["key"] != "empty"),
        "an empty value is not imported: {e}"
    );
    assert!(
        e["extra_vars"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["value"] != "JBSWY3DPEHPK3PXP"),
        "the TOTP seed is kept as the seed, not duplicated as a variable"
    );
}

#[test]
fn a_login_without_a_password_is_imported_and_said_so_and_an_item_without_a_uuid_is_archived() {
    let v = Vault::new("op-layout-gaps");
    let f = write_export(&v);
    let o = v.run(&["import-vault", "onepassword", &f, "--apply"]);
    let items = o.json()["data"]["items"].clone();
    let status = |name: &str| {
        items
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == name)
            .unwrap_or_else(|| panic!("no {name} in {items}"))["status"]
            .clone()
    };
    assert_eq!(status("Example"), "imported");
    assert_eq!(status("Empty login"), "imported");
    assert_eq!(
        status("No uuid"),
        "archived",
        "it cannot be reconciled later without an id"
    );
    assert_eq!(v.run(&["get", "No uuid"]).code, 3);
    assert_eq!(entry(&v, "Empty login")["api_key"], "");
    // .1pux carries no passkeys, and the import says so rather than implying a full copy.
    assert!(
        o.stdout.contains("passkeys"),
        "what the format cannot carry is named: {}",
        o.stdout
    );
}

#[test]
fn the_preview_prints_no_value_and_a_rerun_of_the_same_export_changes_nothing() {
    let v = Vault::new("op-layout-rerun");
    let f = write_export(&v);
    let preview = v.run(&["import-vault", "onepassword", &f]);
    assert_eq!(preview.code, 0, "{}", preview.all());
    for secret in ["pw-secret-1", "rec-code-123", "JBSWY3DP", "alice"] {
        assert!(!preview.all().contains(secret), "preview leaked {secret}");
    }
    assert_eq!(v.run(&["list"]).code, 3);

    v.ok(&["import-vault", "onepassword", &f, "--apply"]);
    let again = v.run(&["import-vault", "onepassword", &f, "--apply"]);
    assert_eq!(again.code, 0, "{}", again.all());
    assert_eq!(again.json()["data"]["counts"]["imported"], 0);
    assert_eq!(v.ok(&["list"])["count"], 2, "no duplicates");
}
