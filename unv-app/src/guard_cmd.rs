// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv guard check` (R35): does this text hold a value the vault holds?
//!
//! The same exact-value matcher as `unv shield` and `unv scan --exposed`, pointed at text on stdin
//! instead of a child's output or a directory. Because it matches the vault's own values it needs
//! no token-shape guessing, so a secret with no recognisable prefix is caught and ordinary prose
//! is not. It prints **names only**: the entry and the field, never a value, a hash or a
//! fingerprint, and it makes no network call. A value shorter than eight characters is reported
//! as a warning and does not fail the check, because short strings occur in text by chance.
//!
//! Exit codes: 0 clean (or warnings only), 10 when a value of eight or more characters was found,
//! 7 when the vault cannot be opened (so a locked vault is never read as "clean").

use crate::access::Access;
use crate::error::{CliError, CliResult};
use crate::exposure::Matcher;
use crate::out;
use serde_json::{json, Value};
use std::io::Read;

/// The most text it will read. A paste is a few kilobytes; this stops a pipe from filling memory.
pub const MAX_INPUT: u64 = 8 * 1024 * 1024;

/// What a check found, as names only.
pub fn summarise(text: &str, vault: &Value) -> Value {
    let hits = Matcher::from_vault(vault).find(text);
    let blocking = hits.iter().filter(|h| !h.short).count();
    let rows: Vec<Value> = hits
        .iter()
        .map(|h| json!({ "entry": h.provider, "field": h.field, "short": h.short }))
        .collect();
    json!({
        "clean": hits.is_empty(),
        "blocking": blocking,
        "warnings": hits.len() - blocking,
        "hits": rows,
    })
}

pub fn check(access: &Access) -> CliResult {
    let mut text = String::new();
    std::io::stdin()
        .take(MAX_INPUT + 1)
        .read_to_string(&mut text)
        .map_err(|e| CliError::invalid(format!("Cannot read the text on stdin: {e}")))?;
    if text.len() as u64 > MAX_INPUT {
        return Err(CliError::invalid(format!(
            "The text is longer than {} MiB; check it in pieces",
            MAX_INPUT / 1024 / 1024
        )));
    }
    let vault = access.load_vault()?;
    run(&text, &vault)
}

/// The check over text and a vault, apart from stdin, so it can be tested without a terminal.
pub fn run(text: &str, vault: &Value) -> CliResult {
    let summary = summarise(text, vault);
    let blocking = summary["blocking"].as_u64().unwrap_or(0);
    if out::is_json() {
        if blocking > 0 {
            return Err(
                CliError::invalid(format!("{blocking} vault secret(s) found in the text"))
                    .with_details(summary),
            );
        }
        out::ok("guard.check", summary, || {});
        return Ok(());
    }
    if summary["clean"] == true {
        println!("No vault values found in the text.");
        return Ok(());
    }
    for h in summary["hits"].as_array().into_iter().flatten() {
        println!(
            "{} {} . {}",
            if h["short"] == true { "warn " } else { "FOUND" },
            h["entry"].as_str().unwrap_or(""),
            h["field"].as_str().unwrap_or("")
        );
    }
    if blocking > 0 {
        return Err(CliError::invalid(format!(
            "{blocking} vault secret(s) found in the text"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> Value {
        json!({ "api_keys": [
            { "id": "1", "provider": "GitHub", "api_key": "plain-token-with-no-known-prefix-9" },
            { "id": "2", "provider": "Tiny", "api_key": "abc123" }
        ] })
    }

    #[test]
    fn a_secret_with_no_recognisable_prefix_is_found_and_named_without_its_value() {
        let s = summarise(
            "please debug: plain-token-with-no-known-prefix-9 fails",
            &vault(),
        );
        assert_eq!(s["blocking"], 1);
        assert_eq!(s["hits"][0]["entry"], "GitHub");
        assert_eq!(s["hits"][0]["field"], "api_key");
        assert!(
            !s.to_string().contains("plain-token-with-no-known-prefix-9"),
            "the report must hold names only: {s}"
        );
    }

    #[test]
    fn prose_and_token_shaped_strings_that_are_not_in_the_vault_are_clean() {
        let s = summarise(
            "ghp_notinthevault0123456789 and sk-also-not-here-0000",
            &vault(),
        );
        assert_eq!(s["clean"], true);
        assert_eq!(s["blocking"], 0);
    }

    #[test]
    fn a_short_value_warns_but_does_not_fail() {
        let s = summarise("the number abc123 appears", &vault());
        assert_eq!(s["blocking"], 0);
        assert_eq!(s["warnings"], 1);
        assert!(run("the number abc123 appears", &vault()).is_ok());
    }

    #[test]
    fn a_blocking_hit_fails_with_the_invalid_code() {
        let err = run("x plain-token-with-no-known-prefix-9 y", &vault()).unwrap_err();
        assert_eq!(err.code, crate::error::Code::Invalid);
        assert!(!err.message.contains("plain-token"), "{}", err.message);
    }

    #[test]
    fn an_empty_vault_finds_nothing() {
        assert_eq!(
            summarise("anything", &json!({ "api_keys": [] }))["clean"],
            true
        );
    }
}
