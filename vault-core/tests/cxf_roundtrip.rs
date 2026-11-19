// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! CXF export then import: what each native credential keeps, what falls back, what is refused.

use serde_json::{json, Value};
use vault_core::cxf::{export, import, parse};

const NOW: &str = "2026-01-01T00:00:00Z";

fn round(entries: &[Value]) -> Vec<Value> {
    let doc = export(entries);
    let doc = parse(&serde_json::to_vec(&doc).unwrap()).unwrap();
    let mut n = 0;
    import(
        &doc,
        || {
            n += 1;
            format!("id-{n}")
        },
        NOW,
    )
}

fn one(entry: Value) -> Value {
    let mut out = round(&[entry]);
    assert_eq!(out.len(), 1, "{out:?}");
    out.remove(0)
}

#[test]
fn a_note_an_ssh_key_and_a_wifi_network_keep_their_fields() {
    let note = one(
        json!({ "id": "n", "provider": "Safe", "secretType": "secure_note", "description": "on the fridge" }),
    );
    assert_eq!(note["description"], "on the fridge");
    let primary = one(
        json!({ "id": "n2", "provider": "Old", "secretType": "secure_note", "api_key": "in primary" }),
    );
    assert_eq!(primary["description"], "in primary");
    let ssh = one(
        json!({ "id": "k", "provider": "Key", "secretType": "ssh_key", "api_key": "private", "api_secret": "public" }),
    );
    assert_eq!(
        (ssh["api_key"].as_str(), ssh["api_secret"].as_str()),
        (Some("private"), Some("public"))
    );
    let wifi = one(
        json!({ "id": "w", "provider": "Home", "account_name": "HomeNet", "secretType": "wifi",
        "api_key": "pass", "extra_vars": [{ "key": "security", "value": "WPA2" }, { "key": "hidden", "value": "true" }] }),
    );
    assert_eq!(wifi["account_name"], "HomeNet");
    let vars = wifi["extra_vars"].as_array().unwrap();
    assert!(vars
        .iter()
        .any(|v| v["key"] == "security" && v["value"] == "WPA2"));
    assert!(vars
        .iter()
        .any(|v| v["key"] == "hidden" && v["value"] == "true"));
    let open = one(json!({ "id": "w2", "provider": "Cafe", "secretType": "wifi" }));
    assert_eq!(open["account_name"], "Cafe");
    assert!(open["extra_vars"].as_array().unwrap().is_empty());
}

#[test]
fn an_api_key_keeps_address_expiry_user_and_a_known_type() {
    let e = one(
        json!({ "id": "a", "provider": "Reg", "secretType": "registry_token", "api_key": "tok",
        "account_name": "ci", "api_url": "https://reg.example", "expires_at": "2027-01-01" }),
    );
    assert_eq!(e["secretType"], "registry_token");
    assert_eq!(e["account_name"], "ci");
    assert_eq!(e["api_url"], "https://reg.example");
    assert_eq!(e["expires_at"], "2027-01-01");
    // A type this build does not know becomes a plain API key.
    let doc = br#"{"items":[{"id":"i","title":"Odd","credentials":[{"type":"api-key","key":"k","keyType":"not_a_type"}]}]}"#;
    if let Ok(parsed) = parse(doc) {
        let out = import(&parsed, || "x".into(), NOW);
        assert_eq!(out[0]["secretType"], "api_key");
    }
}

#[test]
fn totp_parameters_survive_and_unreadable_files_are_refused() {
    let e = one(
        json!({ "id": "t", "provider": "Bank", "secretType": "password", "api_key": "pw",
        "totp_secret": "JBSWY3DPEHPK3PXP", "totp_period": 60, "totp_digits": 8, "totp_algorithm": "SHA256" }),
    );
    assert_eq!(
        (e["totp_period"].as_i64(), e["totp_digits"].as_i64()),
        (Some(60), Some(8))
    );
    assert_eq!(e["totp_algorithm"], "SHA256");
    assert!(matches!(parse(b"not json"), Err(m) if m.starts_with("Not a readable CXF file")));
}

#[test]
fn bundle_locals_named_like_the_markers_come_back_unchanged() {
    let bundle = json!({ "id": "b1", "provider": "Svc", "secretType": "bundle", "extra_vars": [
        { "key": "_unenverse_type", "value": "marker-lookalike" },
        { "key": "__unenverse_local__x", "value": "double" },
        { "key": "plain", "value": "p" }, { "key": "novalue" } ] });
    let out = round(&[bundle]);
    let vars = out[0]["extra_vars"].as_array().unwrap();
    assert!(vars
        .iter()
        .any(|v| v["key"] == "_unenverse_type" && v["value"] == "marker-lookalike"));
    assert!(vars.iter().any(|v| v["key"] == "__unenverse_local__x"));
    assert!(vars.iter().any(|v| v["key"] == "plain"));
    assert!(!vars.iter().any(|v| v["key"] == "novalue"));
}

#[test]
fn a_member_without_a_parent_bundle_still_imports_with_its_seed() {
    let m = json!({ "id": "m", "provider": "Login", "secretType": "password", "api_key": "pw",
        "bundle_id": "orphan", "totp_secret": "JBSWY3DPEHPK3PXP" });
    let out = round(&[m]);
    assert_eq!(out[0]["secretType"], "bundle");
    assert_eq!(out[0]["provider"], "Bundle");
    assert_eq!(out[1]["bundle_slot"], "basic-auth");
    assert_eq!(out[1]["totp_secret"], "JBSWY3DPEHPK3PXP");
}
