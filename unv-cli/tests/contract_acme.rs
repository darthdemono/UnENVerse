// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! "ACME certificates" (R17): `unv acme issue|renew|check` runs the user's own lego, saves the result
//! as a certificate entry whose dates come from the certificate itself, renews only inside the
//! renewal window, and can confirm that a host serves the certificate the vault holds. The DNS token
//! reaches lego as one environment variable and nowhere else.
//!
//! lego here is a stub with the argument shape of lego 5 (checked against 5.5.2) that copies a
//! prepared certificate into `--path`. `pebble_issues_a_real_certificate` drives the genuine lego
//! against Pebble, the Let's Encrypt test CA, and runs only when `UNV_PEBBLE=1`.
#![cfg(unix)]

mod common;
use common::Vault;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Built at run time so the test source holds no credential-shaped literal.
fn dns_token() -> String {
    ["dns-tok", "en-", "4f9a1c07", "d2b8"].concat()
}

struct Chain {
    cert: String,
    key: String,
}

/// A certificate for `names` that has `days` left, signed by itself (the stub stands in for the CA).
fn chain(names: &[&str], days: i64) -> Chain {
    let key = rcgen::KeyPair::generate().unwrap();
    let mut params =
        rcgen::CertificateParams::new(names.iter().map(|n| n.to_string()).collect::<Vec<_>>())
            .unwrap();
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(days);
    let cert = params.self_signed(&key).unwrap();
    Chain {
        cert: cert.pem(),
        key: key.serialize_pem(),
    }
}

/// A lego that answers `--version` and otherwise copies `chain` into `--path/certificates`, after
/// recording its arguments and the two variables a leak would show up in.
fn stub(v: &Vault, version: &str, stem: &str, chain: &Chain) -> String {
    let crt = v.path("stub.crt");
    let key = v.path("stub.key");
    std::fs::write(&crt, &chain.cert).unwrap();
    std::fs::write(&key, &chain.key).unwrap();
    let log = v.path("lego.log");
    let path = v.path("lego-stub");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" = --version ]; then echo "lego version {version} linux/amd64"; exit 0; fi
echo "$@" >> {log}
env | grep -E '^(CF_DNS_API_TOKEN|HETZNER_API_TOKEN|MY_TOKEN|UNV_PASSWORD|LEGO_CA_CERTIFICATES)=' | sed 's/=.*//' >> {log}
echo "token=$CF_DNS_API_TOKEN$MY_TOKEN" >> {log}
while [ $# -gt 0 ]; do if [ "$1" = --path ]; then P="$2"; fi; shift; done
mkdir -p "$P/certificates"
cp {crt} "$P/certificates/{stem}.crt"
cp {key} "$P/certificates/{stem}.key"
"#,
        log = log.display(),
        crt = crt.display(),
        key = key.display(),
    );
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().into_owned()
}

fn lego_log(v: &Vault) -> String {
    std::fs::read_to_string(v.path("lego.log")).unwrap_or_default()
}

fn add_token(v: &Vault) {
    v.ok(&["entry", "add", "dns-token", "--key", &dns_token()]);
}

fn issue(v: &Vault, lego: &str, extra: &[&str]) -> common::Out {
    let mut args = vec![
        "acme",
        "issue",
        "web-cert",
        "--domain",
        "app.example.test",
        "--email",
        "ops@example.test",
        "--lego",
        lego,
    ];
    args.extend_from_slice(extra);
    v.run(&args)
}

const CF: &[&str] = &["--dns", "cloudflare", "--token-entry", "dns-token"];

fn entry_count(v: &Vault) -> usize {
    let o = v.run(&["list"]);
    if o.code != 0 {
        return 0;
    }
    o.json()["data"]["entries"].as_array().unwrap().len()
}

fn field(v: &Vault, name: &str, f: &str) -> String {
    v.reveal_field(name, f)
}

#[test]
fn issue_saves_a_certificate_entry_with_the_dates_the_ca_signed() {
    let v = Vault::new("acme-issue");
    add_token(&v);
    let c = chain(&["app.example.test"], 60);
    let lego = stub(&v, "5.5.2", "app.example.test", &c);
    let o = issue(&v, &lego, CF);
    assert_eq!(o.code, 0, "{}", o.all());
    let d = &o.json()["data"];
    assert_eq!(d["entry"], "web-cert");
    assert_eq!(d["domains"][0], "app.example.test");
    assert_eq!(field(&v, "web-cert", "certificate_data"), c.cert);
    assert_eq!(field(&v, "web-cert", "cert_key_data"), c.key);
    let info = vault_core::certinfo::leaf(&c.cert).unwrap();
    assert_eq!(d["expires_at"], info.not_after.as_str());
    assert_eq!(d["issued_at"], info.not_before.as_str());
    let shown = v.ok(&["get", "web-cert"]);
    assert_eq!(shown["entries"][0]["secretType"], "certificate", "{shown}");
    assert!(
        !o.all().contains(&c.key) && !o.all().contains(&dns_token()),
        "neither the key nor the token is printed"
    );
}

#[test]
fn the_dns_token_reaches_lego_as_one_variable_and_nothing_else_leaks() {
    let v = Vault::new("acme-env");
    add_token(&v);
    let lego = stub(
        &v,
        "5.5.2",
        "app.example.test",
        &chain(&["app.example.test"], 60),
    );
    assert_eq!(issue(&v, &lego, CF).code, 0);
    let log = lego_log(&v);
    let args_line = log.lines().next().unwrap();
    assert!(
        !args_line.contains(&dns_token()),
        "the token is not an argument: {args_line}"
    );
    assert!(args_line.contains("--dns cloudflare"), "{args_line}");
    assert!(args_line.contains("--server https://acme-v02.api.letsencrypt.org/directory"));
    assert!(log.contains("CF_DNS_API_TOKEN"), "{log}");
    assert!(log.contains(&format!("token={}", dns_token())), "{log}");
    assert!(
        !log.contains("UNV_PASSWORD"),
        "the vault password is not inherited by lego: {log}"
    );
}

#[test]
fn a_provider_outside_the_built_in_four_names_its_own_variable() {
    let v = Vault::new("acme-var");
    add_token(&v);
    let lego = stub(
        &v,
        "5.5.2",
        "app.example.test",
        &chain(&["app.example.test"], 60),
    );
    let bare = issue(
        &v,
        &lego,
        &["--dns", "gandiv5", "--token-entry", "dns-token"],
    );
    assert_eq!(bare.error_code(), "invalid", "{}", bare.all());
    assert!(bare.all().contains("--token-var"), "{}", bare.all());
    let named = issue(
        &v,
        &lego,
        &[
            "--dns",
            "gandiv5",
            "--token-entry",
            "dns-token",
            "--token-var",
            "MY_TOKEN",
        ],
    );
    assert_eq!(named.code, 0, "{}", named.all());
    assert!(lego_log(&v).contains(&format!("token={}", dns_token())));
}

#[test]
fn bad_settings_are_refused_before_lego_runs() {
    let v = Vault::new("acme-validate");
    add_token(&v);
    let lego = stub(
        &v,
        "5.5.2",
        "app.example.test",
        &chain(&["app.example.test"], 60),
    );
    let cases: [(&[&str], &str); 7] = [
        (&[], "choose one challenge"),
        (
            &["--dns", "cloudflare", "--http", ":80"],
            "choose one challenge",
        ),
        (&["--http", "80"], "`:port`"),
        (
            &["--http", ":80", "--token-entry", "dns-token"],
            "belongs to a --dns",
        ),
        (
            &[
                "--dns",
                "cloudflare",
                "--directory",
                "http://ca.example.test/dir",
            ],
            "https://",
        ),
        (&["--dns", "cloud flare"], "DNS provider"),
        (
            &[
                "--dns",
                "cloudflare",
                "--token-entry",
                "dns-token",
                "--token-var",
                "bad name",
            ],
            "environment variable",
        ),
    ];
    for (extra, expect) in cases {
        let o = issue(&v, &lego, extra);
        assert_eq!(o.error_code(), "invalid", "{extra:?}: {}", o.all());
        assert!(o.all().contains(expect), "{extra:?}: {}", o.all());
    }
    let bad_domain = v.run(&[
        "acme",
        "issue",
        "web-cert",
        "--domain",
        "not a domain",
        "--email",
        "ops@example.test",
        "--http",
        ":80",
        "--lego",
        &lego,
    ]);
    assert_eq!(bad_domain.error_code(), "invalid", "{}", bad_domain.all());
    let bad_mail = v.run(&[
        "acme",
        "issue",
        "web-cert",
        "--domain",
        "a.example.test",
        "--email",
        "nobody",
        "--http",
        ":80",
        "--lego",
        &lego,
    ]);
    assert_eq!(bad_mail.error_code(), "invalid", "{}", bad_mail.all());
    assert_eq!(lego_log(&v), "", "lego never ran");
    assert_eq!(entry_count(&v), 1, "only the token entry exists");
}

#[test]
fn a_certificate_that_does_not_cover_the_domain_is_not_saved() {
    let v = Vault::new("acme-san");
    add_token(&v);
    let lego = stub(
        &v,
        "5.5.2",
        "app.example.test",
        &chain(&["other.example.test"], 60),
    );
    let o = issue(&v, &lego, CF);
    assert_eq!(o.error_code(), "invalid", "{}", o.all());
    assert!(
        o.all().contains("does not cover app.example.test"),
        "{}",
        o.all()
    );
    assert_eq!(entry_count(&v), 1);
}

#[test]
fn lego_missing_or_too_old_is_a_plain_refusal() {
    let v = Vault::new("acme-version");
    add_token(&v);
    let missing = issue(&v, "/nonexistent/lego", CF);
    assert_eq!(missing.error_code(), "not_found", "{}", missing.all());
    let old = stub(
        &v,
        "4.21.0",
        "app.example.test",
        &chain(&["app.example.test"], 60),
    );
    let o = issue(&v, &old, CF);
    assert_eq!(o.error_code(), "invalid", "{}", o.all());
    assert!(o.all().contains("lego 5 or newer"), "{}", o.all());
    assert_eq!(entry_count(&v), 1);
}

#[test]
fn an_existing_entry_that_is_not_a_certificate_is_never_overwritten() {
    let v = Vault::new("acme-clash");
    add_token(&v);
    v.ok(&["entry", "add", "web-cert", "--key", "plain-value-12345"]);
    let lego = stub(
        &v,
        "5.5.2",
        "app.example.test",
        &chain(&["app.example.test"], 60),
    );
    let o = issue(&v, &lego, CF);
    assert_eq!(o.error_code(), "conflict", "{}", o.all());
    assert_eq!(field(&v, "web-cert", "api_key"), "plain-value-12345");
}

#[test]
fn renew_waits_for_the_window_and_swaps_the_certificate_inside_it() {
    let v = Vault::new("acme-renew");
    add_token(&v);
    let first = chain(&["app.example.test"], 60);
    assert_eq!(
        issue(&v, &stub(&v, "5.5.2", "app.example.test", &first), CF).code,
        0
    );
    let runs = |v: &Vault| {
        lego_log(v)
            .lines()
            .filter(|l| l.starts_with("run "))
            .count()
    };
    assert_eq!(runs(&v), 1);

    let lego = stub(
        &v,
        "5.5.2",
        "app.example.test",
        &chain(&["app.example.test"], 90),
    );
    let idle = v.run(&["acme", "renew", "web-cert", "--lego", &lego]);
    assert_eq!(idle.code, 0, "{}", idle.all());
    assert_eq!(idle.json()["data"]["renewed"], false);
    assert_eq!(runs(&v), 1, "outside the window lego is not run");
    assert_eq!(field(&v, "web-cert", "certificate_data"), first.cert);

    let forced = v.run(&["acme", "renew", "web-cert", "--force", "--lego", &lego]);
    assert_eq!(forced.code, 0, "{}", forced.all());
    assert_eq!(forced.json()["data"]["renewed"], true);
    assert_eq!(runs(&v), 2);
    assert_ne!(field(&v, "web-cert", "certificate_data"), first.cert);

    let wide = v.run(&[
        "acme", "renew", "web-cert", "--days", "200", "--lego", &lego,
    ]);
    assert_eq!(
        wide.json()["data"]["renewed"],
        true,
        "a wider window makes it due"
    );
    assert_eq!(runs(&v), 3);
}

#[test]
fn renew_only_touches_entries_that_acme_issue_made() {
    let v = Vault::new("acme-foreign");
    v.ok(&["entry", "add", "hand-made", "--key", "plain-value-12345"]);
    let o = v.run(&["acme", "renew", "hand-made", "--lego", "/nonexistent/lego"]);
    assert_eq!(o.error_code(), "invalid", "{}", o.all());
    assert!(o.all().contains("was not issued by"), "{}", o.all());
}

/// Serves `chain` over TLS on a free port until the returned guard is dropped.
fn serve(chain: &Chain, handshakes: usize) -> u16 {
    use vault_core::tls::rustls;
    let config = vault_core::tls::server_config_tls13(&chain.cert, &chain.key).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = std::sync::Arc::new(config);
    std::thread::spawn(move || {
        for _ in 0..handshakes {
            let Ok((mut tcp, _)) = listener.accept() else {
                return;
            };
            let Ok(mut conn) = rustls::ServerConnection::new(config.clone()) else {
                return;
            };
            while conn.is_handshaking() {
                if conn.complete_io(&mut tcp).is_err() {
                    break;
                }
            }
            let _ = conn.writer().write_all(b"");
            let _ = conn.complete_io(&mut tcp);
            let mut sink = [0u8; 16];
            let _ = tcp.read(&mut sink);
        }
    });
    port
}

#[test]
fn check_confirms_the_served_certificate_is_the_one_in_the_vault() {
    let v = Vault::new("acme-check");
    add_token(&v);
    let c = chain(&["app.example.test"], 60);
    assert_eq!(
        issue(&v, &stub(&v, "5.5.2", "app.example.test", &c), CF).code,
        0
    );

    let port = serve(&c, 1).to_string();
    let ok = v.run(&[
        "acme",
        "check",
        "web-cert",
        "127.0.0.1",
        "--port",
        &port,
        "--sni",
        "app.example.test",
    ]);
    assert_eq!(ok.code, 0, "{}", ok.all());
    assert_eq!(ok.json()["data"]["matches"], true);

    let stale = chain(&["app.example.test"], 60);
    let port = serve(&stale, 1).to_string();
    let bad = v.run(&[
        "acme",
        "check",
        "web-cert",
        "127.0.0.1",
        "--port",
        &port,
        "--sni",
        "app.example.test",
    ]);
    assert_eq!(bad.error_code(), "conflict", "{}", bad.all());
}

#[test]
fn check_reports_an_unreachable_host_as_unavailable() {
    let v = Vault::new("acme-down");
    add_token(&v);
    let c = chain(&["app.example.test"], 60);
    assert_eq!(
        issue(&v, &stub(&v, "5.5.2", "app.example.test", &c), CF).code,
        0
    );
    let free = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port().to_string()
    };
    let o = v.run(&[
        "acme",
        "check",
        "web-cert",
        "127.0.0.1",
        "--port",
        &free,
        "--sni",
        "app.example.test",
    ]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert_eq!(o.error_code(), "unavailable", "{}", o.all());
}

#[test]
fn pebble_issues_a_real_certificate() {
    if std::env::var("UNV_PEBBLE").as_deref() != Ok("1") {
        eprintln!("UNV_PEBBLE is not set; the stub tests above cover the contract");
        return;
    }
    let lego = std::env::var("LEGO_BIN").unwrap_or_else(|_| "lego".into());
    let ca = std::env::var("PEBBLE_CA").expect("PEBBLE_CA names pebble.minica.pem");
    assert!(Path::new(&ca).is_file(), "{ca}");
    let v = Vault::new("acme-pebble");
    let o = v.run(&[
        "acme",
        "issue",
        "web-cert",
        "--domain",
        "acme.test",
        "--email",
        "ops@example.test",
        "--directory",
        "https://localhost:14000/dir",
        "--http",
        ":5002",
        "--ca-file",
        &ca,
        "--lego",
        &lego,
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    let pem = field(&v, "web-cert", "certificate_data");
    let info = vault_core::certinfo::leaf(&pem).unwrap();
    assert_eq!(info.dns_names, ["acme.test"]);
    assert_eq!(o.json()["data"]["expires_at"], info.not_after.as_str());
}
