//! Importing and exporting authenticator seeds ("Authenticator codes", README):
//!
//! * a list of `otpauth://` URIs imports as entries, kinds and parameters
//!   included, and an HOTP entry keeps its counter through import and export;
//! * re-importing an unchanged list changes nothing, and a different seed under
//!   a name that already holds one is reported (as fingerprints) and left alone
//!   unless `--force` is given;
//! * an export is nothing but seeds, so it is refused to the terminal and
//!   written 0600 by `--out`; it round-trips through Aegis and 2FAS;
//! * an encrypted export from another app is refused by name, not decrypted.

mod common;
use common::Vault;
use serde_json::Value;

const SEED_A: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"; // RFC 4226 test secret
const SEED_B: &str = "JBSWY3DPEHPK3PXP";

fn list_file(v: &Vault, name: &str, body: &str) -> String {
    let p = v.path(name);
    std::fs::write(&p, body).unwrap();
    p.to_str().unwrap().to_string()
}

fn standard(v: &Vault) -> String {
    list_file(
        v,
        "list.txt",
        &format!(
            "otpauth://totp/GitHub:octo?secret={SEED_A}&issuer=GitHub\n\
             otpauth://hotp/Bank:me?secret={SEED_B}&issuer=Bank&counter=7\n"
        ),
    )
}

fn rows(v: &Vault) -> Vec<Value> {
    v.run(&["totp", "ls"]).json()["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn row(v: &Vault, provider: &str) -> Value {
    rows(v)
        .into_iter()
        .find(|r| r["provider"] == provider)
        .unwrap_or_else(|| panic!("no {provider}"))
}

fn export(v: &Vault, fmt: &str, extra: &[&str]) -> String {
    let out = v.path(&format!("export.{fmt}"));
    let mut a = vec![
        "totp",
        "export",
        "--format",
        fmt,
        "--out",
        out.to_str().unwrap(),
    ];
    a.extend_from_slice(extra);
    let o = v.run(&a);
    assert_eq!(o.code, 0, "{fmt}: {}", o.all());
    std::fs::read_to_string(out).unwrap()
}

#[test]
fn an_otpauth_list_imports_with_kinds_parameters_and_the_hotp_counter() {
    let v = Vault::new("totpimport");
    let f = standard(&v);
    let o = v.run(&["totp", "import", &f]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(
        o.json()["data"]["created"].as_array().unwrap().len(),
        2,
        "{}",
        o.stdout
    );

    let gh = row(&v, "GitHub");
    assert_eq!(
        (
            gh["kind"].as_str().unwrap(),
            gh["digits"].as_u64().unwrap(),
            gh["period"].as_u64().unwrap()
        ),
        ("totp", 6, 30)
    );
    let bank = row(&v, "Bank");
    assert_eq!(bank["kind"], "hotp");
    assert_eq!(
        bank["counter"], 7,
        "an HOTP entry keeps its counter through import"
    );
    assert!(
        !v.run(&["totp", "ls"]).all().contains(SEED_A),
        "listing never shows a seed"
    );
}

#[test]
fn a_rerun_is_a_no_op_and_a_different_seed_is_a_conflict_unless_forced() {
    let v = Vault::new("totpconflict");
    let f = standard(&v);
    v.ok(&["totp", "import", &f]);
    let again = v.ok(&["totp", "import", &f]);
    assert_eq!(again["unchanged"].as_array().unwrap().len(), 2, "{again}");
    assert!(again["created"].as_array().unwrap().is_empty());

    let different = list_file(
        &v,
        "diff.txt",
        &format!("otpauth://totp/GitHub:octo?secret={SEED_B}&issuer=GitHub\n"),
    );
    let o = v.run(&["totp", "import", &different]);
    assert_eq!(o.code, 0, "{}", o.all());
    let conflicts = o.json()["data"]["conflicts"].clone();
    assert_eq!(conflicts.as_array().unwrap().len(), 1, "{}", o.stdout);
    assert!(
        conflicts[0]["incoming"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"),
        "fingerprints, never seeds: {conflicts}"
    );
    assert!(
        !o.all().contains(SEED_B) && !o.all().contains(SEED_A),
        "{}",
        o.all()
    );
    // Left alone: the original seed still produces the original code.
    let kept = export(&v, "otpauth", &["--only", "GitHub"]);
    assert!(
        kept.contains(SEED_A) && !kept.contains(SEED_B),
        "a second factor is not recoverable once overwritten: {kept}"
    );

    let forced = v.ok(&["totp", "import", &different, "--force"]);
    assert_eq!(forced["updated"].as_array().unwrap().len(), 1, "{forced}");
    let after = export(&v, "otpauth", &["--only", "GitHub"]);
    assert!(
        after.contains(SEED_B) && !after.contains(SEED_A),
        "--force replaces: {after}"
    );
}

#[test]
fn an_export_is_refused_to_the_terminal_and_written_owner_only() {
    let v = Vault::new("totpexport");
    v.ok(&["totp", "import", &standard(&v)]);
    let refused = v.run(&["totp", "export", "--format", "otpauth"]);
    assert_eq!(refused.code, 9, "{}", refused.all());
    assert!(!refused.all().contains(SEED_A), "{}", refused.all());

    let text = export(&v, "otpauth", &[]);
    assert!(text.contains(&format!("secret={SEED_A}")), "{text}");
    assert!(
        text.contains("counter=7"),
        "the HOTP counter survives export: {text}"
    );
    #[cfg(unix)]
    assert_eq!(common::mode_of(&v.path("export.otpauth")), 0o600);
}

#[test]
fn only_limits_an_export_to_the_named_entries() {
    let v = Vault::new("totponly");
    v.ok(&["totp", "import", &standard(&v)]);
    let one = export(&v, "otpauth", &["--only", "Bank"]);
    assert!(one.contains("Bank") && !one.contains("GitHub"), "{one}");
}

#[test]
fn aegis_and_2fas_exports_import_back_to_the_same_seeds() {
    for fmt in ["aegis", "2fas"] {
        let v = Vault::new(&format!("totp-{fmt}"));
        v.ok(&["totp", "import", &standard(&v)]);
        let body = export(&v, fmt, &[]);
        assert!(
            serde_json::from_str::<Value>(&body).is_ok(),
            "{fmt} is JSON: {body}"
        );
        assert!(body.contains(SEED_A), "{fmt}: {body}");

        let fresh = Vault::new(&format!("totp-{fmt}-in"));
        let f = list_file(&fresh, &format!("in.{fmt}"), &body);
        let o = fresh.run(&["totp", "import", &f, "--format", fmt]);
        assert_eq!(o.code, 0, "{fmt}: {}", o.all());
        assert_eq!(rows(&fresh).len(), 2, "{fmt}: {}", o.stdout);
        let back = export(&fresh, "otpauth", &["--only", "GitHub"]);
        assert!(back.contains(SEED_A), "{fmt} round trip: {back}");
    }
}

#[test]
fn an_encrypted_export_is_refused_by_name_and_imports_nothing() {
    let v = Vault::new("totpencrypted");
    let enc = list_file(
        &v,
        "aegis-encrypted.json",
        r#"{"version":1,"header":{"slots":[{"type":1,"uuid":"x","key":"y"}],"params":{"nonce":"n","tag":"t"}},"db":"Zm9vYmFy"}"#,
    );
    let o = v.run(&["totp", "import", &enc, "--format", "aegis"]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(
        o.all().to_lowercase().contains("encrypt"),
        "refused BY NAME, not 'parse error': {}",
        o.all()
    );
    assert!(rows(&v).is_empty());
}

#[test]
fn a_file_that_is_not_a_seed_export_is_refused() {
    let v = Vault::new("totpjunk");
    let junk = list_file(&v, "junk.txt", "this is not an authenticator export\n");
    let o = v.run(&["totp", "import", &junk]);
    assert!(
        o.code != 0
            || o.json()["data"]["created"]
                .as_array()
                .is_none_or(|c| c.is_empty()),
        "{}",
        o.all()
    );
    assert!(rows(&v).is_empty());
}
