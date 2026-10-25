// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The flags of `unv entry add` / `set`, checked against what their help promises:
//!
//! * `--icon-file` embeds an image whose TYPE is read from its bytes (a text file
//!   named `.png` is refused, SVG is refused because it can carry script) and is
//!   size-capped like the app's icons; a refused icon creates nothing;
//! * `--label` takes letters, digits, `_` and `-`, 24 at most;
//! * `--generate` makes a value of the requested format and never prints it;
//! * `--cert` / `--cert-key` read `@path`; `--key` does not (it is the value);
//! * `--expires` must be a real date, because the health scan compares dates as
//!   text and would otherwise never report a mistyped one;
//! * `--var` is `key=value`, a bad one is invalid input.

mod common;

use common::Vault;

fn field(v: &Vault, provider: &str, name: &str) -> String {
    v.reveal_field(provider, name)
}

/// `list` on a vault with nothing in it answers "not found" rather than an empty list.
fn count(v: &Vault) -> u64 {
    let o = v.run(&["list"]);
    match o.code {
        3 => 0,
        0 => o.json()["data"]["count"].as_u64().unwrap(),
        _ => panic!("{}", o.all()),
    }
}

fn write(v: &Vault, name: &str, bytes: &[u8]) -> String {
    let p = v.path(name);
    std::fs::write(&p, bytes).unwrap();
    p.to_str().unwrap().to_string()
}

#[test]
fn an_icon_is_typed_by_its_bytes_not_by_its_name() {
    let v = Vault::new("fields-icon");
    for (name, bytes, mime) in [
        ("a.dat", &b"\x89PNG\r\n\x1a\n0000"[..], "image/png"),
        ("b.dat", b"\xff\xd8\xff\xe0abc", "image/jpeg"),
        ("c.dat", b"GIF89a123", "image/gif"),
        ("d.dat", b"BMxxxxx", "image/bmp"),
        ("e.dat", b"\0\0\x01\0xxxx", "image/x-icon"),
        ("f.dat", b"RIFFabcdWEBPxxxx", "image/webp"),
    ] {
        let file = write(&v, name, bytes);
        v.ok(&["entry", "add", name, "--key", "k", "--icon-file", &file]);
        let icon = field(&v, name, "custom_icon");
        assert!(
            icon.starts_with(&format!("data:{mime};base64,")),
            "{name}: {icon}"
        );
    }
}

#[test]
fn an_icon_that_is_not_a_safe_image_is_refused_and_nothing_is_created() {
    let v = Vault::new("fields-icon-bad");
    let svg = write(&v, "x.svg", b"<svg onload=alert(1)/>");
    let fake = write(&v, "fake.png", b"this is text named .png");
    let mut big = b"\x89PNG".to_vec();
    big.extend(std::iter::repeat_n(7u8, 100 * 1024));
    let big = write(&v, "big.png", &big);
    for (name, file, code) in [
        ("Svg", svg, 10),
        ("Fake", fake, 10),
        ("Big", big, 10),
        (
            "Missing",
            v.path("nope.png").to_str().unwrap().to_string(),
            3,
        ),
    ] {
        let o = v.run(&["entry", "add", name, "--key", "k", "--icon-file", &file]);
        assert_eq!(o.code, code, "{name}: {}", o.all());
    }
    assert_eq!(
        count(&v),
        0,
        "a refused icon must not leave a half-made entry"
    );
}

#[test]
fn a_label_is_letters_digits_underscore_and_dash_up_to_24() {
    let v = Vault::new("fields-label");
    v.ok(&["entry", "add", "Ok", "--key", "k", "--label", "prod_db-1"]);
    for bad in ["bad label", "semi;colon", "ünï", &"a".repeat(25)] {
        let o = v.run(&["entry", "add", "Bad", "--key", "k", "--label", bad]);
        assert_eq!(o.code, 10, "{bad:?}: {}", o.all());
    }
    v.ok(&[
        "entry",
        "add",
        "Max",
        "--key",
        "k",
        "--label",
        &"a".repeat(24),
    ]);
    assert_eq!(count(&v), 2);
}

#[test]
fn a_generated_value_has_the_requested_format_and_is_never_printed() {
    let v = Vault::new("fields-generate");
    for fmt in ["hex", "base64", "base64url", "password"] {
        let o = v.run(&[
            "entry",
            "add",
            fmt,
            "--generate",
            "--generate-format",
            fmt,
            "--generate-bytes",
            "20",
        ]);
        assert_eq!(o.code, 0, "{fmt}: {}", o.all());
        let value = field(&v, fmt, "api_key");
        assert!(
            !o.all().contains(&value),
            "{fmt}: the value must not be printed"
        );
        match fmt {
            "hex" => {
                assert_eq!(value.len(), 40, "{value}");
                assert!(value.chars().all(|c| c.is_ascii_hexdigit()));
            }
            "base64" => assert_eq!(value.len(), 28, "20 bytes encode to 28 chars: {value}"),
            "base64url" => {
                assert!(!value.contains(['+', '/']), "{value}");
                assert!(value.len() >= 27, "{value}");
            }
            _ => assert_eq!(value.chars().count(), 20, "{value}"),
        }
    }
    // Two generations never agree, and a short request is raised to a safe minimum.
    v.ok(&[
        "entry",
        "add",
        "again",
        "--generate",
        "--generate-format",
        "hex",
    ]);
    assert_ne!(field(&v, "again", "api_key"), field(&v, "hex", "api_key"));
    v.ok(&[
        "entry",
        "add",
        "short",
        "--generate",
        "--generate-format",
        "password",
        "--generate-bytes",
        "3",
    ]);
    assert!(field(&v, "short", "api_key").chars().count() >= 12);
}

#[test]
fn a_certificate_is_read_from_a_path_but_a_key_that_starts_with_at_is_just_a_key() {
    let v = Vault::new("fields-cert");
    let pem = "-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n";
    let file = write(&v, "c.pem", pem.as_bytes());
    v.ok(&[
        "entry",
        "add",
        "C",
        "--key",
        "k",
        "--cert",
        &format!("@{file}"),
    ]);
    assert_eq!(field(&v, "C", "certificate_data"), pem.trim_end());

    let o = v.run(&[
        "entry",
        "add",
        "C2",
        "--key",
        "k",
        "--cert",
        "@/no/such/file.pem",
    ]);
    assert_eq!(o.code, 3, "{}", o.all());

    v.ok(&["entry", "add", "At", "--key", &format!("@{file}")]);
    assert_eq!(
        field(&v, "At", "api_key"),
        format!("@{file}"),
        "--key is the value itself"
    );
}

/// BUG: `--expires soon` (or `2030-13-45`) was accepted as is. The health scan
/// compares the first ten characters as text, so such a value was never reported
/// expired and the entry's expiry warning silently never fired.
#[test]
fn a_mistyped_expiry_is_refused_instead_of_silently_disabling_the_warning() {
    let v = Vault::new("fields-expires");
    for bad in [
        "soon",
        "2030-13-45",
        "2030-02-30",
        "30-01-2030",
        "2030-1-5",
        "2030-01-01x",
    ] {
        let o = v.run(&["entry", "add", "E", "--key", "k", "--expires", bad]);
        assert_eq!(o.code, 10, "{bad:?}: {}", o.all());
    }
    assert_eq!(count(&v), 0);
    v.ok(&[
        "entry",
        "add",
        "Old",
        "--key",
        "k",
        "--expires",
        "2001-01-01",
    ]);
    v.ok(&[
        "entry",
        "add",
        "Stamp",
        "--key",
        "k",
        "--expires",
        "2001-01-01T10:00:00Z",
    ]);
    let scan = v.run(&["scan"]).stdout;
    assert!(scan.contains("Expired on 2001-01-01"), "{scan}");
    // And it can be cleared again.
    v.ok(&["entry", "set", "Old", "--expires", ""]);
    let scan = v.run(&["scan"]).stdout;
    assert_eq!(scan.matches("Expired on 2001-01-01").count(), 1, "{scan}");
}

#[test]
fn extra_variables_are_key_value_pairs_and_a_bang_marks_one_secret() {
    let v = Vault::new("fields-var");
    v.ok(&[
        "entry",
        "add",
        "V",
        "--key",
        "k",
        "--var",
        "REGION=eu",
        "--var",
        "!TOKEN=t0k",
        "--var",
        "REGION=us",
    ]);
    assert_eq!(
        field(&v, "V", "REGION"),
        "us",
        "a repeated key keeps the last value"
    );
    let o = v.run(&["get", "V"]);
    assert!(
        !o.all().contains("t0k"),
        "a secret variable is redacted: {}",
        o.all()
    );
    let bad = v.run(&["entry", "add", "Bad", "--key", "k", "--var", "NOEQUALS"]);
    assert_eq!(bad.code, 10, "{}", bad.all());
}

#[test]
fn a_secret_read_from_stdin_never_needs_the_command_line_and_cannot_be_given_twice() {
    let v = Vault::new("fields-stdin");
    let o = v.run_stdin(&["entry", "add", "S", "--key-stdin"], "from-stdin-value");
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(field(&v, "S", "api_key"), "from-stdin-value");
    let both = v.run_stdin(&["entry", "add", "T", "--key", "a", "--key-stdin"], "b");
    assert_ne!(both.code, 0, "{}", both.all());
    assert_eq!(count(&v), 1);
}

#[test]
fn an_app_name_and_purpose_are_stored_apart_from_the_purpose_given_to_the_issuer() {
    // The app a secret was made for, and what it does with it, are not the sentence
    // told to the issuer (`--purpose`). Both are plain metadata, so they are readable like the purpose.
    let v = Vault::new("fields-app-meta");
    v.ok(&[
        "entry",
        "add",
        "Acme",
        "--key",
        "k",
        "--purpose",
        "told the issuer",
        "--app-name",
        "Release bot",
        "--app-purpose",
        "posts notes",
    ]);
    assert_eq!(field(&v, "Acme", "purpose"), "told the issuer");
    assert_eq!(field(&v, "Acme", "app_name"), "Release bot");
    assert_eq!(field(&v, "Acme", "app_purpose"), "posts notes");
}

#[test]
fn a_new_secret_starts_on_a_value_role_and_an_explicit_role_wins() {
    // Every secret with a primary value carries a role, so the variable is
    // PROVIDER_ROLE (GITHUB_KEY) and never a bare PROVIDER. Without a value there is
    // nothing to name.
    let v = Vault::new("fields-default-role");
    v.ok(&["entry", "add", "Gh", "--key", "k"]);
    assert_eq!(field(&v, "Gh", "primary_role"), "key");
    v.ok(&["entry", "add", "Site", "--type", "password", "--key", "pw"]);
    assert_eq!(field(&v, "Site", "primary_role"), "password");
    v.ok(&["entry", "add", "DB_URL", "--type", "env_var", "--key", "x"]);
    assert_eq!(field(&v, "DB_URL", "primary_role"), "value");
    v.ok(&["entry", "add", "Cid", "--key", "k", "--role", "id"]);
    assert_eq!(field(&v, "Cid", "primary_role"), "id");
    // Nothing to name without a value: the role is left unset.
    v.ok(&["entry", "add", "Empty"]);
    assert_eq!(v.run(&["get", "Empty", "--field", "primary_role"]).code, 3);
}
