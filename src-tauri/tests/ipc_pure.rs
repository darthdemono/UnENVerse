// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The desktop commands that need no vault key: each is a thin door onto the
//! one Rust implementation the CLI shares, so what is checked here is that the
//! door passes the arguments through, returns the documented shape and refuses
//! junk with a message instead of a panic.

mod common;
use common::desk;
use serde_json::{json, Value};

const GH: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

fn field(k: &str, v: &str) -> Value {
    json!({ "key": k, "value": v })
}

#[test]
fn the_config_checker_finds_a_proxy_pass_to_a_service_the_project_does_not_define() {
    let d = desk("check");
    let project = json!({ "id": "p", "name": "p", "chunks": [
        { "id": "a", "name": "loc", "chunk_type": "nginx_location", "fields": [field("path", "/"), field("proxy_pass", "http://api:8080")] },
        { "id": "b", "name": "web", "chunk_type": "docker_service", "fields": [field("image", "x")] },
    ]});
    let found = d.ok(
        "config_check_project",
        json!({ "project": project, "vaultNames": [] }),
    );
    assert_eq!(
        found[0]["rule"], "nginx-proxy-pass-unknown-service",
        "{found}"
    );
    // The same name defined elsewhere in the stack is not a mistake.
    let scoped = d.ok(
        "config_check_project",
        json!({ "project": project, "vaultNames": [], "elsewhere": ["api"] }),
    );
    assert!(scoped.as_array().unwrap().is_empty(), "{scoped}");
}

#[test]
fn toml_imports_flatten_tables_into_upper_snake_names() {
    let d = desk("toml");
    let r = d.ok(
        "parse_toml_import",
        json!({ "source": "a = \"1\"\n[t]\nb = \"2\"\n" }),
    );
    assert_eq!(r, json!([["A", "1"], ["T_B", "2"]]));
    assert!(d
        .call("parse_toml_import", json!({ "source": "not = = toml" }))
        .is_err());
}

#[test]
fn a_pasted_cookie_capture_becomes_cookies_with_the_domain_normalised() {
    let d = desk("capture");
    let r = d.ok(
        "session_capture_parse",
        json!({ "text": "sessionid=abc; Path=/; Domain=example.com", "origin": null }),
    );
    assert_eq!(r["cookies"][0]["name"], "sessionid", "{r}");
    assert_eq!(r["cookies"][0]["domain"], ".example.com", "{r}");
    assert!(
        d.call("session_capture_parse", json!({ "text": "" }))
            .is_err()
            || d.ok("session_capture_parse", json!({ "text": "" }))["cookies"]
                .as_array()
                .unwrap()
                .is_empty()
    );
}

#[test]
fn type_emit_refuses_an_output_the_entry_type_does_not_offer() {
    let d = desk("emit");
    let e = d.call("type_emit", json!({ "entry": { "provider": "X", "secretType": "api_key", "api_key": "k" }, "format": "env" })).unwrap_err();
    assert!(e.to_string().contains("is not an output of"), "{e}");
}

#[test]
fn enrich_proposes_from_public_prefixes_and_lists_who_could_be_probed_online() {
    let d = desk("enrich");
    let plan = d.ok(
        "enrich_plan",
        json!({ "entries": [{ "provider": "GitHub", "api_key": GH }], "force": false }),
    );
    let fields: Vec<&str> = plan[0]["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["field"].as_str().unwrap())
        .collect();
    assert!(
        fields.contains(&"api_url") && fields.contains(&"tags"),
        "{fields:?}"
    );
    assert!(
        !plan.to_string().contains(GH),
        "a plan carries a fingerprint, never the key"
    );
    let targets = d.ok(
        "enrich_online_targets",
        json!({ "entries": [{ "provider": "GitHub", "api_key": GH }] }),
    );
    assert_eq!(targets[0]["issuer"], "GitHub", "{targets}");
    assert!(!targets.to_string().contains(GH));
    assert!(d.ok("enrich_plan", json!({ "entries": [{ "provider": "Plain", "api_key": "no-known-prefix-1234567" }], "force": false }))[0]["proposals"].as_array().is_none_or(|p| p.is_empty()));
}

#[test]
fn the_document_doctor_warns_about_a_partial_vault_and_catalogue_status_names_its_source() {
    let d = desk("doctor");
    let r = d.ok(
        "doctor_document",
        json!({ "vault": { "api_keys": [], "projects": [] } }),
    );
    assert!(
        r.as_array()
            .unwrap()
            .iter()
            .any(|c| c["check"] == "schema" && c["level"] == "warn"),
        "{r}"
    );
    assert_eq!(d.ok("catalogue_status", json!({}))["source"], "bundled");
}

#[test]
fn a_mnemonic_is_checked_by_word_count_and_checksum() {
    let d = desk("bip39");
    let good = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    assert_eq!(
        d.ok("bip39_validate", json!({ "mnemonic": good })),
        Value::Null
    );
    let few = d
        .call(
            "bip39_validate",
            json!({ "mnemonic": "nonsense words here" }),
        )
        .unwrap_err();
    assert!(few.to_string().contains("12, 15, 18, 21 or 24"), "{few}");
    let bad_sum = good.replace("about", "abandon");
    assert!(
        d.call("bip39_validate", json!({ "mnemonic": bad_sum }))
            .is_err(),
        "right length, wrong checksum"
    );
}

#[test]
fn credential_exchange_and_calendar_exports_carry_what_they_promise() {
    let d = desk("exports");
    let cxf = d.ok(
        "cxf_export",
        json!({ "entries": [{ "provider": "X", "api_key": "export-secret-123456" }] }),
    );
    let back = d.ok("cxf_import", json!({ "text": cxf }));
    assert_eq!(back[0]["provider"], "X", "round trip: {back}");
    assert!(d
        .ok("cxf_import", json!({ "text": "{}" }))
        .as_array()
        .unwrap()
        .is_empty());

    let ics = d.ok("calendar_build_ics", json!({ "entries": [{ "provider": "X", "api_key": "calendar-secret-1234", "expires_at": "2030-01-01" }], "kinds": ["expiry"], "calendarName": "T" }));
    let ics = ics.as_str().unwrap();
    assert!(
        ics.starts_with("BEGIN:VCALENDAR")
            && ics.contains("DTSTART;VALUE=DATE:20300101")
            && ics.contains("SUMMARY:Expires: X"),
        "{ics}"
    );
    assert!(
        !ics.contains("calendar-secret-1234"),
        "names and dates only"
    );
}

#[test]
fn authenticator_import_merges_into_entries_and_export_builds_each_format() {
    let d = desk("totp");
    let uri = format!("otpauth://totp/GitHub:me?secret={SEED}&issuer=GitHub");
    let merged = d.ok(
        "totp_import_merge",
        json!({ "entries": [], "text": uri, "force": false, "project": null, "category": null }),
    );
    assert_eq!(merged["created"][0]["provider"], "GitHub", "{merged}");
    assert_eq!(merged["entries"].as_array().unwrap().len(), 1);
    let again = d.ok(
        "totp_import_merge",
        json!({ "entries": merged["entries"], "text": uri, "force": false }),
    );
    assert!(
        again["created"].as_array().unwrap().is_empty(),
        "a rerun adds nothing: {again}"
    );
    let junk = d.call(
        "totp_import_merge",
        json!({ "entries": [], "text": "nothing here", "force": false }),
    );
    assert!(junk.is_err() || junk.unwrap()["created"].as_array().unwrap().is_empty());
}

#[test]
fn entropy_and_key_generation_return_usable_material() {
    let d = desk("generate");
    let sources = d.ok("entropy_sources", json!({}));
    assert!(
        sources
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == "os" && s["ready"] == true),
        "{sources}"
    );
    let hex = d.ok("entropy_fill", json!({ "source": "os", "length": 8 }));
    assert_eq!(hex.as_str().unwrap().len(), 16, "8 bytes as hex");
    assert_ne!(
        hex,
        d.ok("entropy_fill", json!({ "source": "os", "length": 8 }))
    );
    assert!(
        d.call(
            "entropy_fill",
            json!({ "source": "file:/no/such/device", "length": 8 })
        )
        .is_err(),
        "an unavailable source is an error, not a silent fallback"
    );

    let ssh = d.ok("generate_ssh_keypair", json!({ "comment": "test@host" }));
    assert!(
        ssh["private_key"]
            .as_str()
            .unwrap()
            .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"),
        "{ssh}"
    );
    assert!(
        ssh["public_key"]
            .as_str()
            .unwrap()
            .starts_with("ssh-ed25519 "),
        "{ssh}"
    );
    let cert = d.ok(
        "generate_certificate",
        json!({ "commonName": "x.example", "validityDays": 30 }),
    );
    assert!(
        cert["cert_pem"]
            .as_str()
            .unwrap()
            .contains("BEGIN CERTIFICATE"),
        "{cert}"
    );
    assert!(
        cert["key_pem"]
            .as_str()
            .is_some_and(|k| k.contains("PRIVATE KEY")),
        "{cert}"
    );
}

#[test]
fn the_approver_key_is_stable_and_only_signatures_cross_the_boundary() {
    let d = desk("approver");
    let a = d.ok("approver_public", json!({}));
    let b = d.ok("approver_public", json!({}));
    assert_eq!(a, b, "one device, one key");
    assert_eq!(a["fingerprint"].as_str().unwrap().len(), 64);
    let signed = d.ok(
        "approver_sign",
        json!({ "nodeId": "n1", "target": "t", "sha256": "ab".repeat(32), "approvalId": "a1" }),
    );
    assert!(
        !signed.to_string().contains("seed") && !signed.to_string().contains("private"),
        "{signed}"
    );
}

#[test]
fn pool_commands_hand_out_members_in_turn_and_reset_forgets() {
    let d = desk("pool");
    let m = |id: &str| json!({ "id": id, "provider": id, "account_name": "", "key_id": "" });
    let members = json!([m("one"), m("two")]);
    assert!(d
        .ok("pool_state_path", json!({}))
        .as_str()
        .unwrap()
        .ends_with("pools.json"));
    let first = d.ok("pool_next", json!({ "pool": "p", "members": members }));
    let second = d.ok("pool_next", json!({ "pool": "p", "members": members }));
    assert_ne!(first, second, "two calls hand back two members");
    assert_eq!(
        d.ok("pool_next", json!({ "pool": "p", "members": [] })),
        Value::Null,
        "an empty pool has nobody to hand out"
    );

    d.ok(
        "pool_set_cooldown",
        json!({ "pool": "p", "member": m("one"), "seconds": 600 }),
    );
    for _ in 0..3 {
        assert_eq!(
            d.ok("pool_next", json!({ "pool": "p", "members": members })),
            json!(1),
            "the cooling member is skipped"
        );
    }
    let state = d.ok("pool_state", json!({ "pool": "p", "members": members }));
    assert_eq!(state.as_array().unwrap().len(), 2, "{state}");
    d.ok("pool_reset", json!({ "pool": "p" }));
    let after = d.ok("pool_state", json!({ "pool": "p", "members": members }));
    assert!(!after.to_string().contains("cooling\":true"), "{after}");
}

#[test]
fn the_materialisation_note_is_accepted_and_the_export_file_lands_in_a_private_directory() {
    let d = desk("export");
    assert_eq!(
        d.ok(
            "matlog_note",
            json!({ "text": "A=1", "note": "copied", "vault": {} })
        ),
        Value::Null
    );
    let path = d.ok(
        "write_export_file",
        json!({ "filename": "out.env", "content": "A=1\n" }),
    );
    assert_eq!(
        std::fs::read_to_string(path.as_str().unwrap()).unwrap(),
        "A=1\n"
    );
    let escaped = d.ok(
        "write_export_file",
        json!({ "filename": "../../escape.env", "content": "x" }),
    );
    assert!(!escaped.as_str().unwrap().contains(".."), "{escaped}");
}

#[test]
fn authenticator_export_builds_each_format_and_refuses_an_unknown_one() {
    let d = desk("totpexport");
    let item = json!({
        "issuer": "GitHub", "account": "me", "secret": SEED, "note": null,
        "algorithm": "SHA1", "digits": 6, "period": 30, "kind": "totp", "counter": 0,
    });
    let uri = d.ok(
        "totp_export_build",
        json!({ "items": [item], "format": "otpauth" }),
    );
    assert!(
        uri.as_str().unwrap().contains(&format!("secret={SEED}")),
        "{uri}"
    );
    for fmt in ["aegis", "2fas"] {
        let body = d.ok(
            "totp_export_build",
            json!({ "items": [item], "format": fmt }),
        );
        assert!(
            serde_json::from_str::<Value>(body.as_str().unwrap()).is_ok(),
            "{fmt} is JSON"
        );
    }
    let bad = d
        .call(
            "totp_export_build",
            json!({ "items": [item], "format": "nope" }),
        )
        .unwrap_err();
    assert!(bad.to_string().contains("Unknown format"), "{bad}");
}

#[test]
fn network_commands_refuse_before_any_request_when_the_input_is_not_acceptable() {
    let d = desk("netrefuse");
    // The catalogue is fetched over https only; an http URL is refused without a connection.
    let e = d
        .call(
            "catalogue_update",
            json!({ "url": "http://127.0.0.1:1/catalogue.json" }),
        )
        .unwrap_err();
    assert!(e.to_string().contains("only fetched over https"), "{e}");
    // An https URL nobody listens on is an error the pane can show, not a panic.
    let e = d
        .call(
            "catalogue_update",
            json!({ "url": "https://127.0.0.1:1/catalogue.json" }),
        )
        .unwrap_err();
    assert!(
        e.to_string().contains("could not fetch the catalogue"),
        "{e}"
    );
    assert_eq!(
        d.ok("catalogue_status", json!({}))["source"],
        "bundled",
        "a failed update caches nothing"
    );

    // An entry that is not an OAuth client has nothing to refresh.
    assert!(d
        .call(
            "oauth_refresh",
            json!({ "entry": { "provider": "X", "secretType": "api_key", "api_key": "k" } })
        )
        .is_err());
    // Online enrichment only probes entries it recognises; the rest are not contacted.
    let none = d.ok("enrich_online", json!({ "entries": [{ "provider": "Plain", "api_key": "no-known-prefix-1234567" }], "force": false }));
    assert!(none.as_array().unwrap().is_empty(), "{none}");
}

#[test]
fn a_damaged_key_block_is_refused() {
    let d = desk("pgp");
    let junk = d.call("pgp_inspect", json!({ "text": "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\nnot base64!\n-----END PGP PUBLIC KEY BLOCK-----" }));
    assert!(junk.is_err(), "a damaged block is refused");
}
