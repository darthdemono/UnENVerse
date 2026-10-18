//! Authenticator codes and key pools, as the README describes them
//! ("Authenticator codes", "Key pools"):
//!
//! * codes are computed per RFC 4226 (HOTP) and RFC 6238 (TOTP) and match the
//!   published test vectors;
//! * reading an HOTP code never moves its counter, `advance` does;
//! * the seed is a secret and the code is not: `ls` shows names and parameters,
//!   never codes or seeds; a second seed is never silently put over the first;
//! * a pool hands out members in turn, skips one that was reported limited,
//!   exits 7 naming the one that frees up first when all are cooling, and keeps
//!   its state in a 0600 file outside the vault.

mod common;
use common::Vault;

/// RFC 4226 appendix D: the secret is the ASCII string "12345678901234567890".
const RFC_SEED_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
/// The first five HOTP values for that secret (RFC 4226, appendix D).
const RFC_HOTP: [&str; 5] = ["755224", "287082", "359152", "969429", "338314"];

fn code(v: &Vault, provider: &str) -> Value {
    v.ok(&["totp", "code", provider])
}

use serde_json::Value;

#[test]
fn hotp_codes_match_the_rfc_4226_test_vectors_and_only_advance_moves_the_counter() {
    let v = Vault::new("hotp");
    v.ok(&[
        "entry",
        "add",
        "Hotp",
        "--key",
        "login-secret-12345",
        "--totp",
        RFC_SEED_BASE32,
        "--totp-kind",
        "hotp",
        "--totp-digits",
        "6",
    ]);
    for (counter, expected) in RFC_HOTP.iter().enumerate() {
        // Reading never moves it: ask twice, get the same answer.
        let first = code(&v, "Hotp");
        let second = code(&v, "Hotp");
        assert_eq!(first["code"], *expected, "counter {counter}: {first}");
        assert_eq!(first["counter"], counter as u64);
        assert_eq!(
            first["code"], second["code"],
            "reading a code must not spend it"
        );
        assert_eq!(
            v.ok(&["totp", "advance", "Hotp"])["counter"],
            (counter + 1) as u64
        );
    }
}

#[test]
fn advance_can_skip_several_positions_but_never_go_backwards() {
    let v = Vault::new("hotpby");
    v.ok(&[
        "entry",
        "add",
        "Hotp",
        "--key",
        "login-secret-12345",
        "--totp",
        RFC_SEED_BASE32,
        "--totp-kind",
        "hotp",
    ]);
    assert_eq!(
        v.ok(&["totp", "advance", "Hotp", "--by", "3"])["counter"],
        3
    );
    assert_eq!(code(&v, "Hotp")["code"], RFC_HOTP[3]);
    let back = v.run(&["totp", "advance", "Hotp", "--by", "-1"]);
    assert_ne!(back.code, 0, "a negative move is refused: {}", back.all());
    assert_eq!(
        code(&v, "Hotp")["counter"],
        3,
        "a refused advance changes nothing"
    );
}

#[test]
fn a_totp_code_is_six_digits_and_names_its_parameters() {
    let v = Vault::new("totp");
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "login-secret-12345",
        "--totp",
        RFC_SEED_BASE32,
    ]);
    let c = code(&v, "GitHub");
    let digits = c["code"].as_str().unwrap();
    assert_eq!(digits.len(), 6, "{c}");
    assert!(digits.chars().all(|d| d.is_ascii_digit()), "{c}");
    assert_eq!(c["kind"], "totp");
    assert_eq!(c["period"], 30);
    assert_eq!(c["algorithm"], "SHA1");
    // Without --next there is exactly one working code on screen.
    assert!(
        c.get("next_code").is_none_or(|n| n.is_null()),
        "the next code is opt-in: {c}"
    );
    let with_next = v.ok(&["totp", "code", "GitHub", "--next"]);
    assert!(
        with_next["next_code"].is_string(),
        "--next prints the code that replaces this one: {with_next}"
    );
}

#[test]
fn the_seed_is_a_secret_and_listing_never_shows_a_code() {
    let v = Vault::new("seed");
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "login-secret-12345",
        "--totp",
        RFC_SEED_BASE32,
    ]);
    let live = code(&v, "GitHub")["code"].as_str().unwrap().to_string();

    let ls = v.run(&["totp", "ls"]);
    assert_eq!(ls.code, 0, "{}", ls.all());
    assert!(ls.stdout.contains("GitHub"), "{}", ls.stdout);
    assert!(
        !ls.all().contains(&live),
        "ls lists names and parameters, never codes: {}",
        ls.all()
    );
    assert!(!ls.all().contains(RFC_SEED_BASE32), "{}", ls.all());

    for args in [
        vec!["get", "GitHub"],
        vec!["list"],
        vec!["entry", "get", "GitHub"],
    ] {
        let o = v.run(&args);
        assert!(
            !o.all().contains(RFC_SEED_BASE32),
            "{args:?} printed the seed: {}",
            o.all()
        );
    }
}

#[test]
fn a_seed_that_is_already_there_is_never_silently_replaced() {
    let v = Vault::new("noreplace");
    // HOTP, so the code depends only on the seed and the counter and cannot change
    // between two reads because a 30 second window rolled over.
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "login-secret-12345",
        "--totp",
        RFC_SEED_BASE32,
        "--totp-kind",
        "hotp",
    ]);
    assert_eq!(code(&v, "GitHub")["code"], RFC_HOTP[0]);
    let o = v.run(&["totp", "add", "GitHub", "--seed", "JBSWY3DPEHPK3PXP"]);
    assert_eq!(
        o.code,
        6,
        "re-enrolling must be a deliberate act: {}",
        o.all()
    );
    assert_eq!(
        code(&v, "GitHub")["code"],
        RFC_HOTP[0],
        "the original seed must still be in place"
    );
}

#[test]
fn an_otpauth_uri_is_split_not_stored_whole() {
    let v = Vault::new("uri");
    let uri = format!(
        "otpauth://totp/GitHub:octo?secret={RFC_SEED_BASE32}&issuer=GitHub&digits=8&period=60"
    );
    v.ok(&["totp", "add", "GitHub", "--seed", &uri]);
    let c = code(&v, "GitHub");
    assert_eq!(c["digits"], 8, "{c}");
    assert_eq!(c["period"], 60, "{c}");
    assert_eq!(c["code"].as_str().unwrap().len(), 8, "{c}");
}

// ---- pools

fn pool_vault(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&[
        "entry",
        "add",
        "OpenAI-1",
        "--key",
        "sk-aaaaaaaaaaaaaaaa",
        "--pool",
        "openai",
    ]);
    v.ok(&[
        "entry",
        "add",
        "OpenAI-2",
        "--key",
        "sk-bbbbbbbbbbbbbbbb",
        "--pool",
        "openai",
    ]);
    v.ok(&["entry", "add", "Unrelated", "--key", "sk-cccccccccccccccc"]);
    v
}

fn next(v: &Vault) -> Value {
    v.ok(&["pool", "next", "openai"])
}

#[test]
fn a_pool_lists_its_members_and_hands_them_out_in_turn() {
    let v = pool_vault("poolturn");
    let ls = v.ok(&["pool", "ls"]);
    assert_eq!(ls["pools"][0]["pool"], "openai");
    assert_eq!(
        ls["pools"][0]["members"], 2,
        "membership is explicit: only the two tagged entries: {ls}"
    );

    let order: Vec<String> = (0..4)
        .map(|_| next(&v)["provider"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        order,
        vec!["OpenAI-1", "OpenAI-2", "OpenAI-1", "OpenAI-2"],
        "two calls hand back two different keys"
    );
}

#[test]
fn a_key_taken_from_a_pool_is_redacted_like_every_other_stdout_path() {
    let v = pool_vault("poolredact");
    let o = v.run(&["pool", "next", "openai"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        !o.all().contains("sk-aaaaaaaaaaaaaaaa") && !o.all().contains("sk-bbbbbbbbbbbbbbbb"),
        "{}",
        o.all()
    );
    let shown = v.run_with(&["--reveal"], &["pool", "next", "openai"]);
    assert!(
        shown.stdout.contains("sk-bbbbbbbbbbbbbbbb"),
        "--reveal opts in: {}",
        shown.stdout
    );
}

#[test]
fn a_member_reported_limited_is_skipped_until_it_recovers() {
    let v = pool_vault("poolcool");
    assert_eq!(next(&v)["provider"], "OpenAI-1");
    // The CLI cannot see a 429 itself; the caller reports it. The default member is the
    // one this machine took most recently.
    v.ok(&["pool", "report", "openai", "--limited", "--for", "15m"]);
    for _ in 0..3 {
        assert_eq!(
            next(&v)["provider"],
            "OpenAI-2",
            "the cooling member must be skipped"
        );
    }
    let ls = v.ok(&["pool", "ls"]);
    assert_eq!(ls["pools"][0]["cooling"], 1, "{ls}");
    assert_eq!(ls["pools"][0]["available"], 1, "{ls}");

    v.ok(&["pool", "report", "openai", "--ok", "OpenAI-1"]);
    let names: Vec<String> = (0..2)
        .map(|_| next(&v)["provider"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"OpenAI-1".to_string()),
        "a recovered member is used again: {names:?}"
    );
}

#[test]
fn when_every_member_is_cooling_the_exit_is_7_and_names_the_one_that_frees_first() {
    let v = pool_vault("poolall");
    next(&v);
    v.ok(&[
        "pool",
        "report",
        "openai",
        "--limited",
        "--for",
        "30m",
        "OpenAI-1",
    ]);
    v.ok(&[
        "pool",
        "report",
        "openai",
        "--limited",
        "--for",
        "5m",
        "OpenAI-2",
    ]);
    let o = v.run(&["pool", "next", "openai"]);
    assert_eq!(o.code, 7, "a caller retries a 7: {}", o.all());
    assert_eq!(o.error_code(), "unavailable");
    assert!(
        o.all().contains("OpenAI-2"),
        "name the member that frees up first: {}",
        o.all()
    );
}

#[test]
fn an_unknown_pool_is_not_found() {
    let v = pool_vault("poolnone");
    let o = v.run(&["pool", "next", "nothing"]);
    assert_eq!(o.code, 3, "{}", o.all());
}

#[cfg(unix)]
#[test]
fn pool_state_lives_in_a_private_file_and_not_in_the_vault() {
    use common::mode_of;
    let v = pool_vault("poolstate");
    let audit_before = v.run(&["audit"]).stdout;
    for _ in 0..3 {
        next(&v);
    }
    // Beside sessions.json, in the per-user state directory (not in the data directory).
    let state = v.dir.join("state").join("unv").join("pools.json");
    assert!(
        state.exists(),
        "the cursor must persist between runs: {}",
        state.display()
    );
    assert_eq!(mode_of(&state), 0o600);
    // A CI loop must not grow the audit chain: taking a key writes no vault row.
    assert_eq!(
        v.run(&["audit"]).stdout,
        audit_before,
        "pool use must not touch the vault"
    );
}
