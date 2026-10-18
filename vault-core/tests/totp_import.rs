// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Importing authenticator exports, from the module's own contract: a seed that
//! cannot be imported is REPORTED by name (never by secret) instead of dropped, so
//! the counts add up; an encrypted export is refused by app name whatever format the
//! caller forced; a hostile or damaged file fails with a reason instead of looping or
//! panicking; a counter-based seed keeps its counter; and an import never replaces a
//! different stored seed without `force`.

use serde_json::{json, Value};
use vault_core::totp::{Algorithm, Kind};
use vault_core::totp_import::{
    build, detect, new_entry, parse, parse_as, plan, write_fields, Format, Plan,
};

const SEED: &str = "JBSWY3DPEHPK3PXP";
const OTHER: &str = "GEZDGNBVGY3TQOJQ";
/// The bytes behind `SEED`.
const HELLO: &[u8] = b"Hello!\xde\xad\xbe\xef";

fn secrets_in(report: &vault_core::totp_import::ParseReport) -> String {
    format!("{:?}", report.skipped)
}

// ---- detection

#[test]
fn detection_names_the_app_when_an_export_is_encrypted_and_refuses_when_forced_too() {
    let cases = [
        (
            json!({ "kdfParams": {}, "encryptedData": "x", "encryptionNonce": "y" }),
            "Ente Auth",
            Format::OtpauthList,
        ),
        (
            json!({ "header": { "slots": [{}] }, "db": { "entries": [] } }),
            "Aegis",
            Format::Aegis,
        ),
        (
            json!({ "header": { "slots": null }, "db": "c3RyaW5n" }),
            "Aegis",
            Format::Aegis,
        ),
        (
            json!({ "services": [], "servicesEncrypted": "blob" }),
            "2FAS",
            Format::TwoFas,
        ),
        (
            json!({ "items": [], "encrypted": true }),
            "Bitwarden",
            Format::Bitwarden,
        ),
    ];
    for (doc, app, forced) in cases {
        let text = doc.to_string();
        let err = detect(&text).unwrap_err();
        assert!(
            err.contains("*encrypted*") && err.contains(app),
            "{app}: {err}"
        );
        // Choosing the format must not turn the refusal into a vague parse error.
        let forced_err = parse_as(&text, forced).unwrap_err();
        assert!(forced_err.contains(app), "{app} forced: {forced_err}");
    }
}

#[test]
fn detection_recognises_each_plain_format_and_refuses_what_it_cannot_place() {
    let bom = format!("\u{feff}otpauth://totp/A?secret={SEED}");
    assert_eq!(detect(&bom).unwrap(), Format::OtpauthList);
    assert_eq!(
        detect("OTPAUTH-MIGRATION://offline?data=AA").unwrap(),
        Format::GoogleMigration
    );
    assert_eq!(
        detect(&json!({ "header": { "slots": null }, "db": { "entries": [] } }).to_string())
            .unwrap(),
        Format::Aegis
    );
    assert_eq!(
        detect(&json!({ "services": [] }).to_string()).unwrap(),
        Format::TwoFas
    );
    assert_eq!(
        detect(&json!({ "items": [] }).to_string()).unwrap(),
        Format::Bitwarden
    );
    assert_eq!(
        detect(&json!([{ "secret": SEED }]).to_string()).unwrap(),
        Format::AndOtp
    );
    // A list is the loosest guess, so it is the last one: a URI buried in notes counts.
    assert_eq!(
        detect(&format!("my notes\n  otpauth://totp/A?secret={SEED}\n")).unwrap(),
        Format::OtpauthList
    );

    assert_eq!(detect("").unwrap_err(), "the file is empty");
    assert_eq!(detect("\u{feff}  \n").unwrap_err(), "the file is empty");
    assert!(detect("{\"hello\": 1}")
        .unwrap_err()
        .contains("not as an authenticator export"));
    assert!(detect(&json!([{ "name": "no secret" }]).to_string())
        .unwrap_err()
        .contains("not as an authenticator export"));
    assert!(detect("just some words")
        .unwrap_err()
        .contains("could not tell which"));
}

// ---- a seed that cannot be imported is named, not dropped, and never printed

#[test]
fn every_json_format_reports_an_unusable_seed_by_name_and_keeps_the_good_ones() {
    let bad = "not base32 !!";
    let aegis = json!({ "header": { "slots": null }, "db": { "entries": [
        { "type": "totp", "name": "ok@x", "issuer": "Good", "info": { "secret": SEED } },
        { "type": "totp", "name": "bad@x", "issuer": "Broken", "info": { "secret": bad } },
        { "type": "totp", "info": { "secret": "" } }
    ]}});
    let twofas = json!({ "services": [
        { "name": "Good", "secret": SEED, "otp": {} },
        { "name": "Broken", "secret": bad, "otp": { "issuer": "Broken" } },
        { "secret": "", "otp": { "account": "only-account" } }
    ]});
    let andotp = json!([
        { "issuer": "Good", "secret": SEED },
        { "issuer": "Broken", "secret": bad },
        { "secret": "", "label": "label-only" }
    ]);
    let bitwarden = json!({ "items": [
        { "name": "Good", "login": { "totp": SEED } },
        { "name": "Broken", "login": { "totp": bad } },
        { "name": "No totp at all", "login": { "password": "x" } },
        { "login": { "totp": "!!" } }
    ]});
    for (doc, format) in [
        (aegis, Format::Aegis),
        (twofas, Format::TwoFas),
        (andotp, Format::AndOtp),
        (bitwarden, Format::Bitwarden),
    ] {
        let report = parse_as(&doc.to_string(), format).unwrap();
        assert_eq!(report.items.len(), 1, "{format:?}: {:?}", report.items);
        assert_eq!(report.items[0].stored.secret, SEED);
        let names: Vec<_> = report.skipped.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Broken"), "{format:?}: {names:?}");
        assert!(
            report
                .skipped
                .iter()
                .all(|s| !s.reason.is_empty() && !s.name.contains(bad)),
            "{format:?}: {:?}",
            report.skipped
        );
        assert!(
            !secrets_in(&report).contains(SEED),
            "{format:?} printed a seed"
        );
    }
    // Bitwarden does not count the many items that simply have no TOTP.
    let bw = parse_as(
        &json!({ "items": [{ "name": "x", "login": {} }, { "name": "y" }] }).to_string(),
        Format::Bitwarden,
    )
    .unwrap();
    assert!(bw.items.is_empty() && bw.skipped.is_empty());
}

#[test]
fn a_file_with_the_wrong_shape_for_its_format_says_what_is_missing() {
    for (format, doc, needle) in [
        (Format::Aegis, json!({ "db": {} }), "no db.entries"),
        (Format::TwoFas, json!({ "x": 1 }), "no services array"),
        (Format::AndOtp, json!({ "x": 1 }), "JSON array"),
        (Format::Bitwarden, json!({ "x": 1 }), "no items array"),
    ] {
        let err = parse_as(&doc.to_string(), format).unwrap_err();
        assert!(err.contains(needle), "{format:?}: {err}");
        let invalid = parse_as("{not json", format).unwrap_err();
        assert!(invalid.contains("not valid JSON"), "{format:?}: {invalid}");
    }
}

#[test]
fn a_uri_list_skips_comments_and_reports_a_bad_line_by_number_not_content() {
    let text = format!(
        "# Ente export\n\n// note\notpauth://totp/Good:me?secret={SEED}&issuer=Good\n\
         otpauth://totp/Bad:me?secret=@@@@\nnot a uri at all\n"
    );
    let report = parse(&text).unwrap();
    assert_eq!(report.items.len(), 1);
    let names: Vec<_> = report.skipped.iter().map(|s| s.name.clone()).collect();
    assert_eq!(names, ["line 5", "line 6"]);
    assert!(!secrets_in(&report).contains("@@@@"));
}

// ---- what each format carries

#[test]
fn kinds_algorithms_digits_periods_counters_and_notes_survive_each_json_format() {
    let aegis = json!({ "header": { "slots": null }, "db": { "entries": [
        { "type": "hotp", "name": "a", "issuer": "H", "note": "keep me",
          "info": { "secret": SEED, "algo": "SHA256", "digits": 8, "counter": 41 } },
        { "type": "steam", "name": "s", "issuer": "Steam", "info": { "secret": SEED, "digits": 6 } },
        { "type": "totp", "name": "t", "issuer": "T", "info": { "secret": SEED, "period": 60, "algo": "SHA512" } }
    ]}});
    let r = parse(&aegis.to_string()).unwrap();
    let (hotp, steam, totp) = (&r.items[0], &r.items[1], &r.items[2]);
    assert_eq!(hotp.stored.params.kind, Kind::Hotp);
    assert_eq!(
        (hotp.stored.params.counter, hotp.stored.params.digits),
        (41, 8)
    );
    assert_eq!(hotp.stored.params.algorithm, Algorithm::Sha256);
    assert_eq!(hotp.note.as_deref(), Some("keep me"));
    assert_eq!(steam.stored.params.kind, Kind::Steam);
    assert_eq!(steam.stored.params.digits, 5, "Steam fixes its own shape");
    assert_eq!(totp.stored.params.period, 60);
    assert_eq!(totp.stored.params.algorithm, Algorithm::Sha512);
    assert_eq!(
        totp.stored.params.counter, 0,
        "a counter is only kept for a counter-based seed"
    );

    let twofas = json!({ "services": [
        { "name": "Svc", "secret": SEED, "otp": { "account": "me", "tokenType": "HOTP", "counter": 7 } },
        { "secret": SEED, "otp": { "label": "from-label", "issuer": "Iss" } }
    ]});
    let r = parse(&twofas.to_string()).unwrap();
    assert_eq!(
        r.items[0].issuer.as_deref(),
        Some("Svc"),
        "the display name is the fallback issuer"
    );
    assert_eq!(r.items[0].stored.params.counter, 7);
    assert_eq!(r.items[1].account.as_deref(), Some("from-label"));

    let andotp = json!([{ "issuer": "I", "label": "l", "secret": SEED, "tags": ["work", "vpn"] },
                        { "secret": SEED, "tags": [] }]);
    let r = parse(&andotp.to_string()).unwrap();
    assert_eq!(r.items[0].note.as_deref(), Some("work, vpn"));
    assert_eq!(r.items[1].note, None);
    assert_eq!(r.items[1].suggested_provider(), "Unnamed");

    let bw = json!({ "items": [{ "name": "Site", "login": { "username": "u",
        "totp": format!("otpauth://totp/Other:x?secret={SEED}&issuer=Other") } }]});
    let r = parse(&bw.to_string()).unwrap();
    assert_eq!(
        r.items[0].issuer.as_deref(),
        Some("Site"),
        "the item name beats the URI's issuer"
    );
    assert_eq!(r.items[0].account.as_deref(), Some("u"));
}

// ---- Google Authenticator migration payloads

fn varint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return out;
        }
        out.push(b | 0x80);
    }
}

fn field_bytes(num: u64, data: &[u8]) -> Vec<u8> {
    let mut out = varint(num << 3 | 2);
    out.extend(varint(data.len() as u64));
    out.extend(data);
    out
}

fn field_varint(num: u64, v: u64) -> Vec<u8> {
    let mut out = varint(num << 3);
    out.extend(varint(v));
    out
}

fn otp(
    secret: &[u8],
    name: &str,
    issuer: &str,
    algo: u64,
    digits: u64,
    kind: u64,
    counter: u64,
) -> Vec<u8> {
    let mut m = field_bytes(1, secret);
    m.extend(field_bytes(2, name.as_bytes()));
    m.extend(field_bytes(3, issuer.as_bytes()));
    m.extend(field_varint(4, algo));
    m.extend(field_varint(5, digits));
    m.extend(field_varint(6, kind));
    if counter > 0 {
        m.extend(field_varint(7, counter));
    }
    m
}

fn uri(payload: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(payload);
    format!(
        "otpauth-migration://offline?data={}",
        b64.replace('+', "%2B")
            .replace('/', "%2F")
            .replace('=', "%3D")
    )
}

#[test]
fn a_google_migration_payload_keeps_algorithm_digits_kind_and_counter() {
    let mut payload = field_bytes(1, &otp(HELLO, "me@x", "Svc", 2, 2, 1, 9));
    payload.extend(field_bytes(1, &otp(HELLO, "plain", "", 0, 0, 2, 0)));
    payload.extend(field_varint(2, 1)); // version: skipped
    payload.extend(field_varint(3, 1)); // batch size: skipped
    let r = parse(&uri(&payload)).unwrap();
    assert_eq!(r.format, Format::GoogleMigration);
    let (a, b) = (&r.items[0], &r.items[1]);
    assert_eq!(a.stored.secret, SEED);
    assert_eq!(a.stored.params.algorithm, Algorithm::Sha256);
    assert_eq!(
        (a.stored.params.digits, a.stored.params.kind),
        (8, Kind::Hotp)
    );
    assert_eq!(
        a.stored.params.counter, 9,
        "dropping the counter would hand over a seed at zero"
    );
    assert_eq!(
        (a.issuer.as_deref(), a.account.as_deref()),
        (Some("Svc"), Some("me@x"))
    );
    assert_eq!(
        (b.stored.params.digits, b.stored.params.kind),
        (6, Kind::Totp)
    );
    assert_eq!(b.stored.params.period, 30);
    assert_eq!(b.issuer, None);
    assert_eq!(b.stored.params.counter, 0);
}

#[test]
fn several_migration_uris_accumulate_and_either_base64_alphabet_is_read() {
    use base64::Engine;
    let one = field_bytes(
        1,
        &otp(&[0xfb, 0xff, 0xfe, 1, 2, 3, 4, 5], "a", "A", 1, 1, 2, 0),
    );
    let url_safe = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&one);
    let two = field_bytes(1, &otp(HELLO, "b", "B", 1, 1, 2, 0));
    let text = format!(
        "# batch 1\notpauth-migration://offline?data={url_safe}&extra=1\n\n{}\n",
        uri(&two)
    );
    // A leading comment hides the format from detection, so the caller names it.
    let r = parse_as(&text, Format::GoogleMigration).unwrap();
    assert_eq!(r.items.len(), 2, "{:?}", r.skipped);
    assert_eq!(r.items[1].issuer.as_deref(), Some("B"));
}

#[test]
fn a_damaged_migration_payload_fails_with_a_reason_instead_of_looping_or_panicking() {
    let good = otp(HELLO, "n", "I", 1, 1, 2, 0);
    // The URI without data=, a non-URI line, and a file with no migration URI at all.
    assert!(parse("otpauth-migration://offline")
        .unwrap_err()
        .contains("no data="));
    assert!(parse_as("hello", Format::GoogleMigration)
        .unwrap_err()
        .contains("no otpauth-migration://"));
    let mixed = format!("garbage line\n{}", uri(&field_bytes(1, &good)));
    let r = parse(&format!(
        "{}\n{}",
        uri(&field_bytes(1, &good)),
        "also not a uri"
    ))
    .unwrap();
    assert!(r
        .skipped
        .iter()
        .any(|s| s.reason.contains("not an otpauth-migration")));
    let r = parse_as(&mixed, Format::GoogleMigration).unwrap();
    assert_eq!(r.items.len(), 1);
    assert!(r.skipped.iter().any(|s| s.name == "line 1"));

    assert!(parse("otpauth-migration://offline?data=!!!not-base64!!!")
        .unwrap_err()
        .contains("not valid base64"));
    // Truncated length-delimited field, and a varint that never ends.
    assert!(parse(&uri(&[0x0a, 0x7f, 1, 2, 3]))
        .unwrap_err()
        .contains("truncated"));
    assert!(parse(&uri(&[0x0a])).unwrap_err().contains("truncated"));
    assert!(parse(&uri(&[0x80; 12]))
        .unwrap_err()
        .contains("malformed varint"));
    // An unknown wire type at the top level is refused; one inside an entry skips that entry.
    assert!(parse(&uri(&[0x0b])).unwrap_err().contains("wire type"));
    let mut inner = good.clone();
    inner.push(0x0b);
    let r = parse(&uri(&field_bytes(1, &inner))).unwrap();
    assert!(r.items.is_empty());
    assert!(r.skipped[0].reason.contains("wire type"), "{:?}", r.skipped);
    assert_eq!(r.skipped[0].name, "I", "the issuer names the skipped entry");

    // Fixed-width and unknown fields are stepped over, and a missing secret is a skip.
    let mut with_extras = good.clone();
    with_extras.extend([0x39, 1, 2, 3, 4, 5, 6, 7, 8]); // field 7, wire 1 (fixed64)
    with_extras.extend([0x45, 1, 2, 3, 4]); // field 8, wire 5 (fixed32)
    with_extras.extend(field_varint(9, 5));
    let r = parse(&uri(&field_bytes(1, &with_extras))).unwrap();
    assert_eq!(r.items.len(), 1, "{:?}", r.skipped);
    let no_secret = otp(b"", "orphan", "", 1, 1, 2, 0);
    let r = parse(&uri(&field_bytes(1, &no_secret))).unwrap();
    assert_eq!(r.skipped[0].name, "orphan");
    assert!(r.skipped[0].reason.contains("no secret"));
    // A truncated entry inside an otherwise good message.
    let r = parse(&uri(&field_bytes(1, &[0x0a, 0x7f]))).unwrap();
    assert_eq!(r.skipped[0].name, "unnamed");
}

// ---- writing

#[test]
fn only_some_formats_can_be_written_and_the_error_says_which() {
    let items = parse(&format!("otpauth://totp/A?secret={SEED}"))
        .unwrap()
        .items;
    for format in [Format::AndOtp, Format::Bitwarden, Format::GoogleMigration] {
        let err = build(&items, format).unwrap_err();
        assert!(
            err.contains("can be imported but not written") && err.contains("aegis"),
            "{format:?}: {err}"
        );
        assert!(!format.is_exportable());
    }
}

#[test]
fn what_is_written_reads_back_as_the_same_seeds_parameters_and_counters() {
    let aegis_in = json!({ "header": { "slots": null }, "db": { "entries": [
        { "type": "hotp", "name": "h@x", "issuer": "H", "info": { "secret": SEED, "counter": 12, "digits": 8, "algo": "SHA256" } },
        { "type": "totp", "name": "t@x", "issuer": "T", "note": "n", "info": { "secret": OTHER, "period": 45 } }
    ]}});
    let items = parse(&aegis_in.to_string()).unwrap().items;
    for format in [Format::OtpauthList, Format::Aegis, Format::TwoFas] {
        let text = build(&items, format).unwrap();
        let back = parse(&text).unwrap_or_else(|e| panic!("{format:?}: {e}\n{text}"));
        assert_eq!(back.format, format);
        assert!(back.skipped.is_empty(), "{format:?}: {:?}", back.skipped);
        assert_eq!(back.items.len(), 2);
        for (a, b) in items.iter().zip(&back.items) {
            assert_eq!(a.stored.secret, b.stored.secret, "{format:?}");
            assert_eq!(a.stored.params, b.stored.params, "{format:?}");
        }
    }
    // An Aegis file is explicitly unencrypted, and its entry ids are stable per seed
    // without spelling the seed.
    let aegis = build(&items, Format::Aegis).unwrap();
    let doc: Value = serde_json::from_str(&aegis).unwrap();
    assert!(doc["header"]["slots"].is_null());
    let id0 = doc["db"]["entries"][0]["uuid"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        build(&items, Format::Aegis).unwrap(),
        aegis,
        "re-exporting is byte-stable"
    );
    assert_ne!(id0, doc["db"]["entries"][1]["uuid"].as_str().unwrap());
    assert!(!id0.contains(SEED));
    assert!(
        doc["db"]["entries"][0]["info"].get("period").is_none(),
        "hotp has no period"
    );
    assert!(
        doc["db"]["entries"][1]["info"].get("counter").is_none(),
        "totp has no counter"
    );
}

// ---- merging into a vault

fn entry(provider: &str, account: &str, seed: Option<&str>) -> Value {
    let mut e = json!({ "provider": provider, "account_name": account });
    if let Some(s) = seed {
        e["totp_secret"] = json!(s);
    }
    e
}

#[test]
fn an_import_never_replaces_a_different_seed_without_force() {
    let items = parse(&format!(
        "otpauth://totp/GitHub:Me?secret={SEED}&issuer=GitHub\n\
         otpauth://totp/GitHub:Other?secret={SEED}&issuer=GitHub\n\
         otpauth://totp/AWS:me?secret={OTHER}&issuer=AWS\n\
         otpauth://totp/New:me?secret={SEED}&issuer=New\n\
         otpauth://totp/Empty:me?secret={SEED}&issuer=Empty\n"
    ))
    .unwrap()
    .items;
    let vault = [
        entry("github", " me ", Some("jbsw y3dp ehpk 3pxp")), // same seed, other case and spacing
        entry("GitHub", "Other", Some(OTHER)),                // a different seed
        entry("AWS", "ME", Some(OTHER)),                      // identical
        entry("Empty", "me", None),                           // nothing stored yet
    ];
    assert_eq!(
        plan(&vault, &items, false),
        [
            Plan::Unchanged { index: 0 },
            Plan::Conflict { index: 1 },
            Plan::Unchanged { index: 2 },
            Plan::Create,
            Plan::Update { index: 3 },
        ]
    );
    assert_eq!(
        plan(&vault, &items, true)[1],
        Plan::Update { index: 1 },
        "force may replace"
    );
}

#[test]
fn defaults_are_removed_not_written_and_a_counter_is_kept_even_at_zero() {
    let mut e = json!({ "provider": "X", "totp_algorithm": "SHA512", "totp_digits": 8,
                        "totp_period": 60, "totp_kind": "hotp", "totp_counter": 5 });
    let plain = parse(&format!("otpauth://totp/X?secret={SEED}"))
        .unwrap()
        .items;
    write_fields(&mut e, &plain[0].stored);
    for gone in [
        "totp_algorithm",
        "totp_digits",
        "totp_period",
        "totp_kind",
        "totp_counter",
    ] {
        assert!(
            e.get(gone).is_none(),
            "re-importing a default clears {gone}: {e}"
        );
    }
    assert_eq!(e["totp_secret"], SEED);

    let hotp = parse(&format!(
        "otpauth://hotp/X?secret={SEED}&counter=0&algorithm=SHA256&digits=8&period=45"
    ))
    .unwrap()
    .items;
    write_fields(&mut e, &hotp[0].stored);
    assert_eq!(e["totp_kind"], "hotp");
    assert_eq!(e["totp_counter"], 0, "zero is a real position");
    assert_eq!(e["totp_algorithm"], "SHA256");
    assert_eq!(e["totp_digits"], 8);
    assert_eq!(e["totp_period"], 45);
}

#[test]
fn a_new_entry_from_a_seed_is_a_password_entry_with_the_account_as_login() {
    let item = &parse(&format!(
        "otpauth://totp/GitHub:me@x?secret={SEED}&issuer=GitHub"
    ))
    .unwrap()
    .items[0];
    let e = new_entry(
        item,
        "id-1",
        "2026-01-01T00:00:00Z",
        Some("work"),
        Some("2fa"),
    );
    assert_eq!(e["provider"], "GitHub");
    assert_eq!(e["secretType"], "password");
    assert_eq!(e["api_key"], "");
    assert_eq!(e["account_name"], "me@x");
    assert_eq!(e["username"], "me@x");
    assert_eq!(e["projectIds"], json!(["Universal", "work"]));
    assert_eq!(e["categories"], json!(["2fa"]));
    assert_eq!(e["totp_secret"], SEED);
    let bare = new_entry(item, "id-2", "now", Some("Universal"), None);
    assert_eq!(bare["projectIds"], json!(["Universal"]));
    assert_eq!(bare["categories"], json!([]));
    let mut noted = item.clone();
    noted.note = Some("hello".into());
    noted.account = None;
    let e = new_entry(&noted, "id-3", "now", None, None);
    assert_eq!(e["description"], "hello");
    assert!(e.get("account_name").is_none());
}
