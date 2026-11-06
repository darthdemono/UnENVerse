// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Per-command secret release (R31): an entry can name the commands `unv exec` may hand it to.
//! An allowed command gets the value, a command that is not on the list gets nothing and exit 11
//! (not the wrong-password code), the refusal is on the audit chain, and a rule that would let a
//! shell run anything needs `--i-understand`.

mod common;
use common::Vault;

const SECRET: &str = "deploy-token-value-that-must-not-leak-2087";

fn vault(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "Deploy", "--key", SECRET]);
    v.ok(&["entry", "set", "Deploy", "--release", "printenv TOK"]);
    v
}

#[test]
fn an_allowed_command_gets_the_value() {
    let v = vault("release-allowed");
    let o = v.run(&["exec", "--entry", "Deploy=TOK", "--", "printenv", "TOK"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(o.stdout.contains(SECRET), "{}", o.stdout);
}

#[test]
fn a_command_that_is_not_listed_gets_nothing_and_exit_11() {
    let v = vault("release-refused");
    let o = v.run(&[
        "exec",
        "--entry",
        "Deploy=TOK",
        "--",
        "sh",
        "-c",
        "printenv TOK",
    ]);
    assert_eq!(o.code, 11, "{}", o.all());
    assert!(
        !o.all().contains(SECRET),
        "the value must not appear: {}",
        o.all()
    );
    assert!(o.all().contains("Deploy"), "{}", o.all());
}

#[test]
fn the_refusal_and_the_command_are_on_the_audit_chain() {
    let v = vault("release-audit");
    let _ = v.run(&["exec", "--entry", "Deploy=TOK", "--", "sh", "-c", "evil"]);
    let a = v.run(&["audit"]);
    assert_eq!(a.code, 0, "{}", a.all());
    assert!(a.all().contains("exec.refused"), "{}", a.all());
    assert!(a.all().contains("sh -c evil"), "{}", a.all());
    assert!(!a.all().contains(SECRET), "{}", a.all());
    let verify = v.run(&["audit", "--verify"]);
    assert_eq!(verify.code, 0, "the chain still verifies: {}", verify.all());
}

#[test]
fn a_dry_run_is_refused_too_and_writes_no_audit_row() {
    let v = vault("release-dry");
    let o = v.run_with(
        &["--dry-run"],
        &["exec", "--entry", "Deploy=TOK", "--", "sh", "-c", "x"],
    );
    assert_eq!(o.code, 11, "{}", o.all());
    assert!(!v.run(&["audit"]).all().contains("exec.refused"));
}

#[test]
fn an_entry_without_a_rule_is_released_to_any_command() {
    let v = Vault::new("release-none");
    v.ok(&["entry", "add", "Open", "--key", SECRET]);
    let o = v.run(&[
        "exec",
        "--entry",
        "Open=TOK",
        "--",
        "sh",
        "-c",
        "printenv TOK",
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
}

#[test]
fn a_rule_that_is_only_a_shell_needs_i_understand_and_an_empty_rule_removes_it() {
    let v = vault("release-broad");
    let o = v.run(&["entry", "set", "Deploy", "--release", "sh -c"]);
    assert_eq!(o.code, 10, "{}", o.all());
    let ok = v.run(&[
        "entry",
        "set",
        "Deploy",
        "--release",
        "sh -c",
        "--i-understand",
    ]);
    assert_eq!(ok.code, 0, "{}", ok.all());
    let clear = v.run(&["entry", "set", "Deploy", "--release", ""]);
    assert_eq!(clear.code, 0, "{}", clear.all());
    let o = v.run(&[
        "exec",
        "--entry",
        "Deploy=TOK",
        "--",
        "sh",
        "-c",
        "printenv TOK",
    ]);
    assert_eq!(o.code, 0, "no rule left, so any command: {}", o.all());
}

#[test]
fn the_new_exit_code_is_part_of_the_described_contract() {
    let v = Vault::new("release-describe");
    let d = v.run(&["describe"]).json();
    let text = d["exit_codes"]["11"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(text.starts_with("released"), "{text}");
}
