//! Generators and entropy sources ("Entropy sources", README, and `unv gen`):
//!
//! * `gen ssh` produces an Ed25519 key in OpenSSH format, the private half is
//!   masked by default, revealed with `--reveal`, and `--save-as` stores a real
//!   key in the vault (not an empty entry);
//! * every generator says which entropy source produced the value;
//! * a device that is absent fails closed with exit 7 and names the device, a
//!   dead one is refused by the health test, and "not built in" is told apart
//!   from "not a thing";
//! * the shape of each generated value is what was asked for.

mod common;
use common::Vault;
use serde_json::Value;

fn data(v: &Vault, global: &[&str], args: &[&str]) -> Value {
    let o = v.run_with(global, args);
    assert_eq!(o.code, 0, "{args:?}: {}", o.all());
    o.json()["data"].clone()
}

// ---- ssh keys

#[test]
fn gen_ssh_reveals_a_real_openssh_keypair_only_on_request() {
    let v = Vault::new("ssh");
    let shown = data(&v, &["--reveal"], &["gen", "ssh", "--comment", "ci@host"]);
    let private = shown["private_key"].as_str().unwrap();
    assert!(
        private.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"),
        "private key: {private:?}"
    );
    assert!(
        private
            .trim_end()
            .ends_with("-----END OPENSSH PRIVATE KEY-----"),
        "{private:?}"
    );
    let public = shown["public_key"].as_str().unwrap();
    assert!(public.starts_with("ssh-ed25519 "), "{public}");
    assert!(
        public.ends_with(" ci@host"),
        "the comment rides on the public key: {public}"
    );

    // Masked by default: a fingerprint of the real key, not of nothing.
    let masked = data(&v, &[], &["gen", "ssh"]);
    let key = &masked["private_key"];
    assert_eq!(key["redacted"], true, "{masked}");
    assert_ne!(
        key["fingerprint"], "empty",
        "the private key must exist even when it is not shown: {masked}"
    );
    assert!(key["length"].as_u64().unwrap() > 100, "{masked}");
    let plain = v.run(&["gen", "ssh"]);
    assert!(
        !plain.all().contains("BEGIN OPENSSH PRIVATE KEY"),
        "{}",
        plain.all()
    );
}

#[test]
fn gen_ssh_save_as_stores_the_private_key_in_the_vault() {
    let v = Vault::new("sshsave");
    let o = v.run(&[
        "gen",
        "ssh",
        "--comment",
        "deploy@host",
        "--save-as",
        "DeployKey",
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_ne!(
        o.json()["data"]["fingerprint"],
        "empty",
        "an empty fingerprint means an empty key was stored: {}",
        o.stdout
    );
    let stored = v.reveal_field("DeployKey", "api_key");
    assert!(
        stored.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"),
        "the vault must hold the generated private key, not an empty value: {stored:?}"
    );
    let entry = &v.ok(&["get", "DeployKey"])["entries"][0];
    assert_eq!(entry["secretType"], "ssh_key", "{entry}");
    assert!(
        entry["description"]
            .as_str()
            .unwrap_or("")
            .starts_with("ssh-ed25519 "),
        "the public half is kept beside it: {entry}"
    );
}

#[test]
fn two_generated_ssh_keys_differ() {
    let v = Vault::new("sshtwo");
    let a = data(&v, &["--reveal"], &["gen", "ssh"])["public_key"].clone();
    let b = data(&v, &["--reveal"], &["gen", "ssh"])["public_key"].clone();
    assert_ne!(a, b);
}

// ---- certificates

#[test]
fn gen_cert_makes_a_pem_certificate_and_key_masked_by_default() {
    let v = Vault::new("cert");
    let shown = data(
        &v,
        &["--reveal"],
        &["gen", "cert", "example.test", "--days", "30"],
    );
    assert!(
        shown["cert_pem"]
            .as_str()
            .unwrap()
            .starts_with("-----BEGIN CERTIFICATE-----"),
        "{shown}"
    );
    assert!(
        shown["key_pem"]
            .as_str()
            .unwrap()
            .contains("PRIVATE KEY-----"),
        "{shown}"
    );
    let masked = data(&v, &[], &["gen", "cert", "example.test"]);
    assert_eq!(masked["key_pem"]["redacted"], true, "{masked}");
    assert!(
        masked["key_pem"]["length"].as_u64().unwrap() > 100,
        "{masked}"
    );
}

#[test]
fn gen_cert_save_as_stores_the_certificate_and_its_key() {
    let v = Vault::new("certsave");
    v.ok(&["gen", "cert", "example.test", "--save-as", "Site"]);
    let entry = &v.ok(&["get", "Site"])["entries"][0];
    assert_eq!(entry["secretType"], "certificate", "{entry}");
    let shown = v.run_with(&["--reveal"], &["get", "Site"]).stdout;
    assert!(shown.contains("BEGIN CERTIFICATE"), "{shown}");
    assert!(
        shown.contains("PRIVATE KEY"),
        "the key is stored with the certificate: {shown}"
    );
}

// ---- secrets and passwords

#[test]
fn gen_secret_has_the_requested_size_and_format() {
    let v = Vault::new("secret");
    let hex = data(&v, &["--reveal"], &["gen", "secret", "--bytes", "16"])["value"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(hex.len(), 32, "16 bytes as hex: {hex}");
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()), "{hex}");
    let b64 = data(
        &v,
        &["--reveal"],
        &["gen", "secret", "--bytes", "16", "--format", "base64"],
    )["value"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        b64.len() >= 22 && b64.len() <= 24,
        "16 bytes as base64: {b64}"
    );
    let url = data(
        &v,
        &["--reveal"],
        &["gen", "secret", "--bytes", "32", "--format", "base64url"],
    )["value"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        !url.contains('+') && !url.contains('/') && !url.contains('='),
        "url-safe: {url}"
    );
    // Masked by default, with the real length.
    let masked = data(&v, &[], &["gen", "secret", "--bytes", "16"]);
    assert_eq!(masked["value"]["length"], 32);
}

#[test]
fn gen_password_honours_length_and_character_sets() {
    let v = Vault::new("password");
    let pw = |args: &[&str]| -> String {
        let mut a = vec!["gen", "password"];
        a.extend_from_slice(args);
        data(&v, &["--reveal"], &a)["value"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let long = pw(&["--length", "40"]);
    assert_eq!(long.chars().count(), 40);
    let letters_only = pw(&["--length", "60", "--no-digits"]);
    assert!(
        letters_only.chars().all(|c| c.is_ascii_alphabetic()),
        "{letters_only}"
    );
    let lower_digits = pw(&["--length", "60", "--no-upper"]);
    assert!(
        lower_digits
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
        "{lower_digits}"
    );
    let with_symbols = pw(&["--length", "200", "--symbols"]);
    assert!(
        with_symbols.chars().any(|c| !c.is_ascii_alphanumeric()),
        "--symbols must include symbols: {with_symbols}"
    );
    assert_ne!(pw(&[]), pw(&[]));
}

// ---- entropy sources

#[test]
fn every_generator_reports_which_entropy_source_produced_the_value() {
    let v = Vault::new("source");
    for args in [
        vec!["gen", "secret"],
        vec!["gen", "password"],
        vec!["gen", "ssh"],
        vec!["gen", "cert", "example.test"],
    ] {
        let d = data(&v, &[], &args);
        assert_eq!(
            d["entropy_source"], "os",
            "{args:?}: the default stays the OS source: {d}"
        );
    }
    let d = data(
        &v,
        &["--entropy-source", "file:/dev/urandom"],
        &["gen", "secret"],
    );
    assert_eq!(
        d["entropy_source"], "file:/dev/urandom",
        "'the flag was accepted' and 'the bytes came from there' differ: {d}"
    );
}

#[test]
fn gen_sources_says_which_sources_work_without_generating_a_key() {
    let v = Vault::new("sources");
    let d = data(&v, &[], &["gen", "sources"]);
    let sources = d["sources"].as_array().unwrap();
    let os = sources
        .iter()
        .find(|s| s["id"] == "os")
        .expect("os is always listed");
    assert_eq!(os["ready"], true);
    assert_eq!(os["hardware"], false);
    for s in sources.iter().filter(|s| s["ready"] == false) {
        assert!(
            !s["detail"].as_str().unwrap_or("").is_empty(),
            "an unusable source says why: {s}"
        );
    }
}

#[test]
fn an_absent_device_fails_closed_with_exit_7_naming_it() {
    let v = Vault::new("absent");
    let o = v.run_with(
        &["--entropy-source", "file:/dev/no-such-hwrng"],
        &["gen", "secret"],
    );
    assert_eq!(o.code, 7, "{}", o.all());
    assert_eq!(o.error_code(), "unavailable");
    assert!(
        o.all().contains("/dev/no-such-hwrng"),
        "name the device: {}",
        o.all()
    );
}

#[test]
fn a_dead_device_is_refused_not_silently_mixed_in() {
    let v = Vault::new("dead");
    // /dev/zero returns the same byte forever: after HKDF mixing it would look
    // random, which is exactly why the health test exists.
    let o = v.run_with(&["--entropy-source", "file:/dev/zero"], &["gen", "secret"]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(
        o.all().to_lowercase().contains("repeat") || o.all().to_lowercase().contains("same byte"),
        "{}",
        o.all()
    );
    assert!(
        o.json().get("data").is_none(),
        "no value may be produced from a failed device"
    );
}

#[test]
fn not_built_in_is_told_apart_from_not_a_thing() {
    let v = Vault::new("kinds");
    let not_built = v.run_with(&["--entropy-source", "pkcs11:slot0"], &["gen", "secret"]);
    let unknown = v.run_with(&["--entropy-source", "banana"], &["gen", "secret"]);
    assert_eq!(not_built.code, 10, "{}", not_built.all());
    assert_eq!(unknown.code, 10, "{}", unknown.all());
    assert!(
        not_built.all().contains("not available in this build"),
        "{}",
        not_built.all()
    );
    assert!(
        unknown.all().contains("Unknown entropy source"),
        "{}",
        unknown.all()
    );
    assert!(
        !(not_built.error_code() == unknown.error_code() && not_built.stdout == unknown.stdout),
        "an unbuilt source and an unknown one are told apart"
    );
}
