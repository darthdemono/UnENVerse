// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The credential-type emitters, checked against the file formats they write
//! (npm, PyPI, Cargo, Docker, netrc, PostgreSQL/JDBC/URL connection strings, the
//! Wi-Fi QR string, BIP39) rather than against the code: each output is what the
//! consuming tool documents, a value that would break the format is refused or
//! escaped, and nothing an error says repeats the secret.

use base64::Engine;
use serde_json::{json, Value};
use vault_core::type_emit::{
    bip39_validate, code_status, emit, formats_for, is_public_format, mark_used, next_code,
    wifi_uri,
};

fn registry(extra: Value) -> Value {
    let mut e = json!({ "secretType": "registry_token", "provider": "Reg", "api_key": "tok-123" });
    for (k, v) in extra.as_object().unwrap() {
        e[k] = v.clone();
    }
    e
}

fn db(vars: &[(&str, &str)], extra: Value) -> Value {
    let mut e = json!({ "secretType": "database", "provider": "Db", "api_key": "pw" });
    e["extra_vars"] = Value::Array(
        vars.iter()
            .map(|(k, v)| json!({ "key": k, "value": v }))
            .collect(),
    );
    for (k, v) in extra.as_object().unwrap() {
        e[k] = v.clone();
    }
    e
}

#[test]
fn an_entry_offers_only_the_formats_of_its_own_type() {
    assert_eq!(formats_for("wifi"), ["wifi-uri"]);
    assert_eq!(formats_for("signing_key"), ["jwks"]);
    assert!(formats_for("api_key").is_empty());
    assert!(is_public_format("jwks") && !is_public_format("npmrc"));
    let err = emit(&json!({ "secretType": "database" }), "npmrc").unwrap_err();
    assert!(err.contains("'npmrc' is not an output of a database entry") && err.contains("dsn"));
    assert!(emit(&json!({}), "dsn").unwrap_err().contains("offers: )"));
}

// ---- registries

#[test]
fn registry_tokens_are_written_the_way_each_tool_reads_them() {
    assert_eq!(
        emit(&registry(json!({})), "npmrc").unwrap(),
        "//registry.npmjs.org/:_authToken=tok-123\n"
    );
    assert_eq!(
        emit(
            &registry(json!({ "api_url": "https://npm.pkg.github.com/" })),
            "npmrc"
        )
        .unwrap(),
        "//npm.pkg.github.com/:_authToken=tok-123\n"
    );
    assert_eq!(
        emit(&registry(json!({})), "pypirc").unwrap(),
        "[pypi]\nusername = __token__\npassword = tok-123\n"
    );
    assert_eq!(
        emit(&registry(json!({ "username": "me" })), "pypirc").unwrap(),
        "[pypi]\nusername = me\npassword = tok-123\n"
    );
    assert_eq!(
        emit(
            &registry(json!({ "extra_vars": [{ "key": "registry", "value": "crates" }] })),
            "cargo-credentials"
        )
        .unwrap(),
        "[registry]\ntoken = \"tok-123\"\n"
    );
    let other = emit(
        &registry(json!({ "api_key": "a\"b\\c", "api_url": "https://registry.example.com/" })),
        "cargo-credentials",
    )
    .unwrap();
    assert_eq!(
        other, "[registries.registry]\ntoken = \"a\\\"b\\\\c\"\n",
        "a quote or backslash is escaped in TOML"
    );
}

#[test]
fn the_registry_name_picks_its_host_and_an_unknown_one_is_used_as_written() {
    for (name, want) in [
        ("pypi", "//upload.pypi.org/legacy/:_authToken=tok-123\n"),
        ("GHCR", "//ghcr.io/:_authToken=tok-123\n"),
        (
            "my.registry.test",
            "//my.registry.test/:_authToken=tok-123\n",
        ),
    ] {
        let e = registry(json!({ "extra_vars": [{ "key": "registry", "value": name }] }));
        assert_eq!(emit(&e, "npmrc").unwrap(), want, "{name}");
    }
}

#[test]
fn docker_and_netrc_need_a_username_and_encode_the_pair() {
    let e = registry(json!({ "username": "bot", "api_url": "https://ghcr.io/" }));
    let cfg: Value = serde_json::from_str(&emit(&e, "docker-config").unwrap()).unwrap();
    let auth = cfg["auths"]["ghcr.io"]["auth"].as_str().unwrap();
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(auth)
            .unwrap(),
        b"bot:tok-123"
    );
    assert_eq!(
        emit(&e, "netrc").unwrap(),
        "machine ghcr.io login bot password tok-123\n"
    );
    let from_var = registry(
        json!({ "extra_vars": [{ "key": "username", "value": "viavar" }], "api_url": "https://h.test/path/x" }),
    );
    assert!(emit(&from_var, "netrc")
        .unwrap()
        .starts_with("machine h.test login viavar"));
    for format in ["docker-config", "netrc"] {
        let err = emit(&registry(json!({})), format).unwrap_err();
        assert!(err.contains("username"), "{format}: {err}");
    }
}

#[test]
fn a_token_that_cannot_be_written_safely_is_refused_without_echoing_it() {
    let err = emit(&registry(json!({ "api_key": "line1\nline2" })), "npmrc").unwrap_err();
    assert!(err.contains("line break") && !err.contains("line1"));
    assert!(emit(&registry(json!({ "api_key": "" })), "npmrc")
        .unwrap_err()
        .contains("the token is empty"));
}

// ---- databases

#[test]
fn a_dsn_uses_the_engines_scheme_and_encodes_what_would_move_the_host() {
    let e = db(
        &[
            ("host", "db.internal"),
            ("port", "5432"),
            ("database", "app"),
            ("user", "svc"),
            ("sslmode", "require"),
        ],
        json!({ "api_key": "p@ss/w:rd#1" }),
    );
    let dsn = emit(&e, "dsn").unwrap();
    assert!(dsn.starts_with("postgresql://svc:"), "{dsn}");
    assert!(
        dsn.ends_with("@db.internal:5432/app?sslmode=require"),
        "{dsn}"
    );
    assert_eq!(
        dsn.matches('@').count(),
        1,
        "the password's @ must not end the userinfo: {dsn}"
    );
    assert!(!dsn.contains("p@ss"), "{dsn}");

    for (engine, scheme) in [
        ("mysql", "mysql"),
        ("MariaDB", "mysql"),
        ("mongo", "mongodb"),
        ("mongodb", "mongodb"),
        ("redis", "redis"),
        ("rabbitmq", "amqp"),
        ("postgresql", "postgresql"),
    ] {
        let e = db(
            &[("host", "h"), ("user", "u"), ("engine", engine)],
            json!({}),
        );
        assert!(
            emit(&e, "dsn")
                .unwrap()
                .starts_with(&format!("{scheme}://u:pw@h")),
            "{engine}"
        );
    }
    let nouser = db(&[("host", "h")], json!({}));
    assert_eq!(emit(&nouser, "dsn").unwrap(), "postgresql://pw@h");
    let redis = db(&[("host", "r"), ("engine", "redis")], json!({}));
    assert_eq!(
        emit(&redis, "dsn").unwrap(),
        "redis://:pw@r",
        "redis takes a password with no user"
    );
    let account = db(&[("host", "h")], json!({ "username": "acct" }));
    assert_eq!(
        emit(&account, "dsn").unwrap(),
        "postgresql://acct:pw@h",
        "the entry's username is the fallback user"
    );
    let unknown = db(&[("host", "h"), ("engine", "oracle")], json!({}));
    assert!(emit(&unknown, "dsn").unwrap_err().contains("'oracle'"));
    assert!(emit(&db(&[], json!({})), "dsn")
        .unwrap_err()
        .contains("the host is empty"));
}

#[test]
fn libpq_variables_are_postgres_only_and_skip_what_is_not_set() {
    let e = db(
        &[
            ("host", "h"),
            ("port", "5433"),
            ("database", "d"),
            ("user", "u"),
        ],
        json!({}),
    );
    assert_eq!(
        emit(&e, "libpq").unwrap(),
        "PGHOST=h\nPGPORT=5433\nPGDATABASE=d\nPGUSER=u\nPGPASSWORD=pw\n"
    );
    let ssl = db(
        &[("host", "h"), ("sslmode", "verify-full")],
        json!({ "api_key": "" }),
    );
    assert_eq!(
        emit(&ssl, "libpq").unwrap(),
        "PGHOST=h\nPGSSLMODE=verify-full\n"
    );
    let mysql = db(&[("host", "h"), ("engine", "mysql")], json!({}));
    assert!(emit(&mysql, "libpq").unwrap_err().contains("PostgreSQL's"));
    let spaced = db(&[("host", "h")], json!({ "api_key": "has space $x" }));
    assert!(emit(&spaced, "libpq").unwrap().contains("PGPASSWORD="));
}

#[test]
fn jdbc_urls_follow_each_drivers_syntax() {
    let pg = db(
        &[
            ("host", "h"),
            ("port", "5432"),
            ("database", "d"),
            ("user", "u"),
            ("sslmode", "require"),
        ],
        json!({}),
    );
    assert_eq!(
        emit(&pg, "jdbc").unwrap(),
        "jdbc:postgresql://h:5432/d?user=u&password=pw&sslmode=require"
    );
    let mysql = db(
        &[("host", "h"), ("engine", "mariadb")],
        json!({ "api_key": "" }),
    );
    assert_eq!(emit(&mysql, "jdbc").unwrap(), "jdbc:mariadb://h");
    let ms = db(
        &[
            ("host", "sql.test"),
            ("port", "1433"),
            ("database", "a}b"),
            ("user", "u;x"),
            ("engine", "mssql"),
        ],
        json!({ "api_key": "p}w;1" }),
    );
    assert_eq!(
        emit(&ms, "jdbc").unwrap(),
        "jdbc:sqlserver://sql.test:1433;databaseName={a}}b};user={u;x};password={p}}w;1}",
        "SQL Server brace-escapes instead of percent-encoding"
    );
    let bare_ms = db(
        &[("host", "s"), ("engine", "sqlserver")],
        json!({ "api_key": "" }),
    );
    assert_eq!(emit(&bare_ms, "jdbc").unwrap(), "jdbc:sqlserver://s");
    let unknown = db(&[("host", "h"), ("engine", "oracle")], json!({}));
    assert!(emit(&unknown, "jdbc").unwrap_err().contains("'oracle'"));
}

// ---- Wi-Fi

fn wifi(vars: &[(&str, &str)], pass: &str) -> Value {
    json!({
        "secretType": "wifi", "provider": "Home", "api_key": pass,
        "extra_vars": vars.iter().map(|(k, v)| json!({ "key": k, "value": v })).collect::<Vec<_>>()
    })
}

#[test]
fn the_wifi_string_escapes_what_would_end_a_field_and_names_the_security_type() {
    assert_eq!(
        wifi_uri(&wifi(&[], "secret")).unwrap(),
        "WIFI:T:WPA;S:Home;P:secret;;"
    );
    assert_eq!(
        wifi_uri(&wifi(
            &[("ssid", "My;Net:\"1\""), ("security", "WPA3")],
            "a;b,c:d\\e\"f"
        ))
        .unwrap(),
        r#"WIFI:T:SAE;S:My\;Net\:\"1\";P:a\;b\,c\:d\\e\"f;;"#
    );
    assert_eq!(
        wifi_uri(&wifi(&[("security", "WEP")], "k")).unwrap(),
        "WIFI:T:WEP;S:Home;P:k;;"
    );
    assert_eq!(
        wifi_uri(&wifi(&[("security", "open")], "")).unwrap(),
        "WIFI:T:nopass;S:Home;;"
    );
    assert_eq!(
        wifi_uri(&wifi(&[("hidden", "Yes")], "p")).unwrap(),
        "WIFI:T:WPA;S:Home;P:p;H:true;;"
    );
    assert_eq!(
        wifi_uri(&wifi(&[("hidden", "no")], "p")).unwrap(),
        "WIFI:T:WPA;S:Home;P:p;;"
    );
    assert_eq!(
        emit(&wifi(&[], "secret"), "wifi-uri").unwrap(),
        "WIFI:T:WPA;S:Home;P:secret;;"
    );
}

#[test]
fn a_wifi_entry_that_the_qr_format_cannot_carry_is_refused_with_the_reason() {
    for (vars, pass, needle) in [
        (vec![("security", "WPA2-Enterprise")], "p", "WPA-Enterprise"),
        (vec![("security", "eap-tls")], "p", "WPA-Enterprise"),
        (
            vec![("security", "carrier-pigeon")],
            "p",
            "unknown security 'carrier-pigeon'",
        ),
        (vec![], "", "the passphrase is empty"),
        (
            vec![("ssid", ""), ("security", "open")],
            "",
            "the SSID is empty",
        ),
    ] {
        let mut e = wifi(&vars, pass);
        if needle.contains("SSID") {
            e["provider"] = json!("");
        }
        let err = wifi_uri(&e).unwrap_err();
        assert!(err.contains(needle), "{needle}: {err}");
        assert!(!err.contains("carrier-pigeon pass"), "{err}");
    }
}

// ---- recovery codes

#[test]
fn recovery_codes_are_never_consumed_by_reading_and_marking_keeps_the_text() {
    let e = json!({ "extra_vars": [{ "key": "codes", "value": "aaaa-1111\nbbbb-2222  \n\n   \ncccc-3333\tUSED 2026-01-01" }] });
    assert_eq!((code_status(&e).total, code_status(&e).remaining), (3, 2));
    assert_eq!(next_code(&e).as_deref(), Some("aaaa-1111"));
    assert_eq!(
        next_code(&e).as_deref(),
        Some("aaaa-1111"),
        "reading does not spend a code"
    );

    let marked = mark_used(&e, None, "2026-10-19").unwrap();
    assert!(
        marked.starts_with("aaaa-1111\tUSED 2026-10-19\nbbbb-2222"),
        "{marked}"
    );
    assert!(marked.contains("cccc-3333\tUSED 2026-01-01"));
    let named = mark_used(&e, Some(" bbbb-2222 "), "d").unwrap();
    assert!(named.contains("bbbb-2222\tUSED d"));
    assert!(mark_used(&e, Some("cccc-3333"), "d")
        .unwrap_err()
        .contains("already marked used"));
    assert!(mark_used(&e, Some("zzzz"), "d")
        .unwrap_err()
        .contains("not in this entry"));

    let spent = json!({ "extra_vars": [{ "key": "codes", "value": "x\tUSED d" }] });
    assert_eq!(next_code(&spent), None);
    assert!(mark_used(&spent, None, "d")
        .unwrap_err()
        .contains("no unused codes"));
    assert_eq!(code_status(&json!({})).total, 0);
}

// ---- BIP39

#[test]
fn a_mnemonic_is_checked_against_the_wordlist_and_its_checksum() {
    let twelve = format!("{} about", ["abandon"; 11].join(" "));
    assert!(bip39_validate(&twelve).is_ok());
    assert!(
        bip39_validate(&twelve.to_uppercase()).is_ok(),
        "case does not matter"
    );
    assert!(bip39_validate(&format!("  {}\n", twelve.replace(' ', "  "))).is_ok());
    let twenty_four = format!("{} art", ["abandon"; 23].join(" "));
    assert!(bip39_validate(&twenty_four).is_ok());

    let bad_checksum = ["abandon"; 12].join(" ");
    assert!(bip39_validate(&bad_checksum)
        .unwrap_err()
        .to_lowercase()
        .contains("checksum"));
    let wrong_count = ["abandon"; 11].join(" ");
    assert_eq!(
        bip39_validate(&wrong_count).unwrap_err(),
        "11 words; a mnemonic has 12, 15, 18, 21 or 24"
    );
    let not_a_word = format!("{} zzzzzz", ["abandon"; 11].join(" "));
    let err = bip39_validate(&not_a_word).unwrap_err();
    assert!(
        err.contains("word 12 is not in the BIP39 English wordlist"),
        "{err}"
    );
    assert!(
        !err.contains("zzzzzz"),
        "a recovery phrase is never echoed: {err}"
    );
}

// ---- JWKS

#[test]
fn a_signing_key_set_needs_a_public_key_and_never_holds_a_private_one() {
    let missing = json!({ "secretType": "signing_key", "extra_vars": [] });
    assert!(emit(&missing, "jwks")
        .unwrap_err()
        .contains("public_key is empty"));
    let private = json!({ "secretType": "signing_key", "extra_vars": [
        { "key": "public_key", "value": "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----" }
    ]});
    assert!(
        emit(&private, "jwks").is_err(),
        "a private PEM is refused, not published"
    );
}
