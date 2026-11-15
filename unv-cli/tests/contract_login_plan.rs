// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv login-plan` and the recovery-code commands (R36): the order of a login is fixed, a plan
//! names entries and never values, pasted codes are added once, and a mistaken "used" is undone.

mod common;

use common::Vault;

fn steps(v: &Vault, bundle: &str) -> Vec<String> {
    v.run(&["login-plan", bundle]).json()["data"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["step"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_bundle_logs_in_with_password_2fa_phrase_then_a_recovery_code() {
    let v = Vault::new("login-plan");
    // Added in the opposite order to the login's.
    v.ok(&[
        "entry",
        "add",
        "AcmeCodes",
        "--type",
        "recovery_codes",
        "--var",
        "codes=first-00001",
    ]);
    v.ok(&[
        "entry",
        "add",
        "AcmePhrase",
        "--type",
        "security_phrase",
        "--key",
        "blue-cat-7",
    ]);
    v.ok(&[
        "entry",
        "add",
        "AcmePw",
        "--type",
        "password",
        "--key",
        "hunter-two-three",
    ]);
    let pasted = v.dir.join("codes.txt");
    std::fs::write(&pasted, "Your codes\nabcde-12345\nfghij-67890\n").unwrap();
    let added = v.run(&["codes", "import", "AcmeCodes", pasted.to_str().unwrap()]);
    assert_eq!(added.json()["data"]["added"], 2, "{}", added.all());
    v.ok(&[
        "bundle",
        "new",
        "Acme",
        "--member",
        "codes=AcmeCodes",
        "--member",
        "phrase=AcmePhrase",
        "--member",
        "password=AcmePw",
    ]);
    // No seed yet: three steps, in the fixed order.
    assert_eq!(steps(&v, "Acme"), ["password", "phrase", "recovery_code"]);
    v.ok(&["entry", "set", "AcmePw", "--totp", "JBSWY3DPEHPK3PXP"]);
    assert_eq!(
        steps(&v, "Acme"),
        ["password", "2fa", "phrase", "recovery_code"]
    );
    // The plan names entries and fields; no value reaches the output.
    let shown = v.run(&["login-plan", "Acme"]).all();
    for secret in [
        "blue-cat-7",
        "hunter-two-three",
        "JBSWY3DPEHPK3PXP",
        "abcde-12345",
    ] {
        assert!(!shown.contains(secret), "a plan printed one of its values");
    }
    let text = v.run_text(&["login-plan", "Acme"]).all();
    assert!(
        text.contains("1. Password") && text.contains("Used recovery codes: ask"),
        "{text}"
    );
}

#[test]
fn the_terminate_setting_is_stored_and_a_bad_mode_is_refused() {
    let v = Vault::new("login-terminate");
    v.ok(&[
        "entry",
        "add",
        "Pw",
        "--type",
        "password",
        "--key",
        "k-k-k-k-k",
    ]);
    v.ok(&["bundle", "new", "Site", "--member", "password=Pw"]);
    let r = v
        .run(&["login-plan", "Site", "--terminate-codes", "auto"])
        .json();
    assert_eq!(r["data"]["terminate_codes"], "auto");
    assert_eq!(
        v.run(&["login-plan", "Site"]).json()["data"]["terminate_codes"],
        "auto"
    );
    assert_ne!(
        v.run(&["login-plan", "Site", "--terminate-codes", "sometimes"])
            .code,
        0
    );
    assert_eq!(v.run(&["login-plan", "Nope"]).code, 3);
    // A bundle with nothing in it says so.
    v.ok(&["bundle", "new", "Empty"]);
    assert!(v
        .run_text(&["login-plan", "Empty"])
        .all()
        .contains("nothing to fill"));
}

#[test]
fn a_used_code_can_be_put_back_and_pasted_codes_are_added_once() {
    let v = Vault::new("codes-unuse");
    v.ok(&[
        "entry",
        "add",
        "Rc",
        "--type",
        "recovery_codes",
        "--var",
        "codes=1234 5678",
    ]);
    let f = v.dir.join("c.txt");
    std::fs::write(&f, "1. 1234 5678\n2. 8765 4321\n").unwrap();
    let first = v
        .run(&["codes", "import", "Rc", f.to_str().unwrap()])
        .json();
    assert_eq!(first["data"]["added"], 1, "the first line was already held");
    let again = v
        .run(&["codes", "import", "Rc", f.to_str().unwrap()])
        .json();
    assert_eq!(again["data"]["added"], 0);
    assert_eq!(again["data"]["remaining"], 2);
    v.ok(&["codes", "use", "Rc", "1234 5678"]);
    assert_eq!(
        v.run(&["codes", "status", "Rc"]).json()["data"]["remaining"],
        1
    );
    let back = v.run(&["codes", "unuse", "Rc", "1234 5678"]).json();
    assert_eq!(back["data"]["remaining"], 2);
    // Not marked used any more, and a code that is not there.
    assert_eq!(v.run(&["codes", "unuse", "Rc", "1234 5678"]).code, 10);
    assert_eq!(v.run(&["codes", "unuse", "Rc", "0000 0000"]).code, 10);
    // Text with no code in it, and an entry that is not a recovery-codes entry.
    std::fs::write(&f, "nothing here\n").unwrap();
    assert_eq!(
        v.run(&["codes", "import", "Rc", f.to_str().unwrap()]).code,
        10
    );
    std::fs::write(&f, "abcde-12345\n").unwrap();
    v.ok(&["entry", "add", "Plain", "--key", "k"]);
    assert_eq!(
        v.run(&["codes", "import", "Plain", f.to_str().unwrap()])
            .code,
        10
    );
}
