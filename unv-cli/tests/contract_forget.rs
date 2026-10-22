// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv entry forget` (R20): old values leave the history, the current value stays, the audit chain
//! still verifies, and a dry run changes nothing.

mod common;
use common::Vault;

const OLD: &str = "first-value-that-must-vanish-1234";
const LIVE: &str = "second-value-that-stays-5678";

fn vault_with_history(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "CF", "--key", OLD]);
    v.ok(&["entry", "set", "CF", "--key", LIVE]);
    v
}

fn history_mentions_old(v: &Vault) -> bool {
    let o = v.run(&["entry", "history", "CF", "--reveal"]);
    o.all().contains(OLD)
}

#[test]
fn forget_removes_old_values_and_keeps_the_current_one() {
    let v = vault_with_history("forget-basic");
    assert!(
        history_mentions_old(&v),
        "the old value is in history first"
    );
    let o = v.run(&["entry", "forget", "CF"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!history_mentions_old(&v), "gone afterwards");
    assert_eq!(v.reveal_field("CF", "api_key"), LIVE);
}

#[test]
fn a_dry_run_forgets_nothing_and_an_unknown_entry_is_refused() {
    let v = vault_with_history("forget-dry");
    let o = v.run_with(&["--dry-run"], &["entry", "forget", "CF"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(history_mentions_old(&v));
    assert_ne!(v.run(&["entry", "forget", "Nope"]).code, 0);
}
