// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv import-vault bitwarden` on an export that uses every item kind Bitwarden
//! has, as the import contract describes it: a preview that writes nothing and
//! prints no value; every readable item becomes an entry with its credential in the
//! primary field and its other data kept as variables, only booleans, URLs and
//! public keys declared public; password history, TOTP and the folder are kept;
//! anything it cannot map (an unknown item type, an item with no stable id) is
//! archived and counted rather than dropped; and what an individual export cannot
//! contain (organisation items, the trash) is named, not silently missing.

mod common;

use common::Vault;
use serde_json::{json, Value};

fn export() -> Value {
    json!({
        "encrypted": false,
        "folders": [{ "id": "f1", "name": "Work" }],
        "items": [
            { "id": "i1", "folderId": "f1", "type": 1, "name": "Login", "notes": "login note",
              "login": { "username": "me", "password": "pw-1-secret", "totp": "JBSWY3DPEHPK3PXP",
                         "uris": [{ "uri": "https://a.test" }, { "uri": "https://b.test" }] },
              "fields": [
                  { "name": "txt", "type": 0, "value": "v0" },
                  { "name": "hid", "type": 1, "value": "v1" },
                  { "name": "flag", "type": 2, "value": "true" },
                  { "name": "lnk", "type": 3, "value": null },
                  { "name": "odd", "type": 9, "value": "x" }
              ],
              "passwordHistory": [{ "password": "old-pw", "lastUsedDate": "2020-01-01T00:00:00Z" }] },
            { "id": "i2", "type": 2, "name": "Note", "notes": "just a note" },
            { "id": "i3", "type": 3, "name": "Card",
              "card": { "cardholderName": "A B", "brand": "Visa", "number": "4111111111111111",
                        "expMonth": "12", "expYear": "2030", "code": "123" } },
            { "id": "i4", "type": 4, "name": "Ident", "identity": { "firstName": "A", "lastName": "B" } },
            { "id": "i5", "type": 5, "name": "SSH",
              "sshKey": { "privateKey": "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----",
                          "publicKey": "ssh-ed25519 AAAA", "keyFingerprint": "SHA256:xyz" } },
            { "id": "i6", "type": 6, "name": "Bank",
              "bankAccount": { "accountNumber": "123456", "routingNumber": "987" } },
            { "id": "i7", "type": 7, "name": "Licence", "driverLicense": { "licenseNumber": "DL-1", "state": "CA" } },
            { "id": "i8", "type": 8, "name": "Passport", "passport": { "number": "P-1", "country": "HU" } },
            { "id": "i9", "type": 99, "name": "Future", "something": "x" },
            { "type": 1, "name": "No id", "login": { "password": "zzz-no-id" } },
            { "id": "i10", "type": 1, "name": "Bare", "login": {} }
        ]
    })
}

fn write_export(v: &Vault) -> String {
    let p = v.path("bw.json");
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
fn the_preview_counts_everything_and_writes_and_prints_nothing() {
    let v = Vault::new("bw-types-preview");
    let f = write_export(&v);
    let o = v.run(&["import-vault", "bitwarden", &f]);
    assert_eq!(o.code, 0, "{}", o.all());
    let c = &o.json()["data"]["counts"];
    assert_eq!(
        (c["imported"].as_u64(), c["archived"].as_u64()),
        (Some(9), Some(2))
    );
    for (n, secret) in [
        "pw-1-secret",
        "4111111111111111",
        "zzz-no-id",
        "old-pw",
        "JBSWY3DP",
    ]
    .iter()
    .enumerate()
    {
        assert!(
            !o.all().contains(secret),
            "the preview leaked value number {n}"
        );
    }
    assert_eq!(v.run(&["list"]).code, 3, "a preview writes no entry");
    // What an individual export cannot contain is named, not silently absent.
    let names = o.stdout.to_lowercase();
    assert!(names.contains("organization-owned items") && names.contains("trashed items"));
}

#[test]
fn each_item_kind_keeps_its_credential_in_the_primary_field_and_the_rest_as_variables() {
    let v = Vault::new("bw-types-apply");
    let f = write_export(&v);
    let before = std::fs::read(&f).unwrap();
    let o = v.run(&["import-vault", "bitwarden", &f, "--apply"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(
        std::fs::read(&f).unwrap(),
        before,
        "the source is never modified"
    );

    let login = entry(&v, "Login");
    assert_eq!(login["api_key"], "pw-1-secret");
    assert_eq!(login["account_name"], "me");
    assert_eq!(login["api_url"], "https://a.test");
    assert_eq!(login["notes"], "login note");
    assert_eq!(login["totp_secret"], "JBSWY3DPEHPK3PXP");
    assert_eq!(login["version_history"][0]["value"], "old-pw");
    assert_eq!(
        login["version_history"][0]["saved_at"],
        "2020-01-01T00:00:00Z"
    );
    // Only a boolean and the URLs are declared public; plain and hidden text are not.
    assert_eq!(var(&login, "flag")["public"], true);
    assert_eq!(var(&login, "txt")["public"], false);
    assert_eq!(var(&login, "hid")["public"], false);
    assert_eq!(var(&login, "url_1")["value"], "https://a.test");
    assert_eq!(var(&login, "url_2")["value"], "https://b.test");
    assert!(
        login["extra_vars"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["key"] != "lnk" && x["key"] != "odd"),
        "a linked or unknown field type is reported, not invented: {login}"
    );

    let note = entry(&v, "Note");
    assert_eq!(
        (note["secretType"].as_str(), note["api_key"].as_str()),
        (Some("secure_note"), Some("just a note"))
    );
    let card = entry(&v, "Card");
    assert_eq!(card["api_key"], "4111111111111111");
    assert_eq!(var(&card, "brand")["public"], true);
    assert_eq!(
        var(&card, "code")["public"],
        false,
        "a card code is never public"
    );
    assert_eq!(var(&card, "expYear")["public"], false);
    let ssh = entry(&v, "SSH");
    assert_eq!(ssh["secretType"], "ssh_key");
    assert!(ssh["api_key"]
        .as_str()
        .unwrap()
        .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
    assert_eq!(var(&ssh, "public_key")["public"], true);
    assert_eq!(var(&ssh, "fingerprint")["public"], true);
    let bank = entry(&v, "Bank");
    assert_eq!(bank["api_key"], "123456");
    assert_eq!(var(&bank, "routingNumber")["value"], "987");
    assert!(bank["extra_vars"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| x["key"] != "accountNumber"));
    assert_eq!(entry(&v, "Licence")["api_key"], "DL-1");
    assert_eq!(entry(&v, "Passport")["api_key"], "P-1");
    let ident = entry(&v, "Ident");
    assert_eq!(ident["secretType"], "identity_document");
    assert_eq!(
        var(&ident, "firstName")["public"],
        false,
        "identity data is never public"
    );
    assert_eq!(entry(&v, "Bare")["api_key"], "");
}

#[test]
fn what_cannot_be_mapped_is_archived_and_counted_not_dropped() {
    let v = Vault::new("bw-types-archive");
    let f = write_export(&v);
    let o = v.run(&["import-vault", "bitwarden", &f, "--apply"]);
    let items = o.json()["data"]["items"].clone();
    let status = |name: &str| {
        items
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == name)
            .unwrap_or_else(|| panic!("no item {name}"))["status"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(
        status("Future"),
        "archived",
        "an unknown item type is kept in the archive"
    );
    assert_eq!(
        status("No id"),
        "archived",
        "an item without a stable id cannot be reconciled"
    );
    assert_eq!(
        v.run(&["get", "Future"]).code,
        3,
        "and does not become a half-made entry"
    );
    assert_eq!(v.run(&["get", "No id"]).code, 3);
    assert!(!o.all().contains("zzz-no-id"), "{}", o.all());
}

#[test]
fn keep_folders_turns_the_folder_into_a_category_and_the_default_does_not() {
    let plain = Vault::new("bw-types-nofolder");
    let f = write_export(&plain);
    plain.ok(&["import-vault", "bitwarden", &f, "--apply"]);
    assert_eq!(entry(&plain, "Login")["categories"], json!([]));

    let kept = Vault::new("bw-types-folder");
    let f = write_export(&kept);
    kept.ok(&["import-vault", "bitwarden", &f, "--apply", "--keep-folders"]);
    assert_eq!(entry(&kept, "Login")["categories"], json!(["Work"]));
}

#[test]
fn an_export_that_is_not_readable_as_a_bitwarden_vault_is_refused_with_the_reason() {
    let v = Vault::new("bw-types-bad");
    for (name, body, reason) in [
        (
            "garbage.json",
            "not json".to_string(),
            "Not a Bitwarden JSON export",
        ),
        ("empty.json", "{}".to_string(), "no items array"),
        (
            "enc.json",
            json!({ "encrypted": true, "encKeyValidation_DO_NOT_EDIT": "x" }).to_string(),
            "refused",
        ),
    ] {
        let p = v.path(name);
        std::fs::write(&p, body).unwrap();
        let o = v.run(&["import-vault", "bitwarden", p.to_str().unwrap(), "--apply"]);
        assert_ne!(o.code, 0, "{name}: {}", o.all());
        assert!(o.all().contains(reason), "{name}: {}", o.all());
    }
    assert_eq!(v.run(&["list"]).code, 3, "a refused source writes nothing");
}

#[test]
fn flags_that_only_make_sense_for_keepass_are_refused_for_other_sources() {
    let v = Vault::new("bw-types-flags");
    let f = write_export(&v);
    let key = v.path("k.key");
    std::fs::write(&key, "x").unwrap();
    let o = v.run(&[
        "import-vault",
        "bitwarden",
        &f,
        "--key-file",
        key.to_str().unwrap(),
    ]);
    assert_eq!(o.code, 10, "{}", o.all());
    let o = v.run(&[
        "import-vault",
        "bitwarden",
        &f,
        "--no-password",
        "--key-file",
        key.to_str().unwrap(),
    ]);
    assert_eq!(o.code, 10, "{}", o.all());
    let o = v.run(&["import-vault", "bitwarden", &f, "--force", "--apply"]);
    assert_eq!(
        o.code,
        10,
        "--force is not accepted for durable migrations: {}",
        o.all()
    );
}
