// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv glossary`: what a term in the product means.
//!
//! The text comes from `glossary/terms.json`, the one file the website glossary and the app's
//! tooltips are built from, so the command line cannot say something different. It needs no vault
//! and no password.

use crate::error::{CliError, CliResult};
use crate::out;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const RAW: &str = include_str!("../../glossary/terms.json");

#[derive(Debug, Clone, Deserialize)]
pub struct Term {
    pub id: String,
    pub term: String,
    pub category: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub definition: String,
    #[serde(default)]
    pub general: Option<String>,
    #[serde(default)]
    pub example: Option<String>,
    #[serde(default)]
    pub see_also: Vec<String>,
}

#[derive(Deserialize)]
struct File {
    terms: Vec<Term>,
}

/// Every term in the embedded glossary.
pub fn terms() -> Vec<Term> {
    serde_json::from_str::<File>(RAW)
        .expect("glossary/terms.json failed to parse: a build defect")
        .terms
}

/// A key for comparing names: lower case, letters and digits only, so `api_key`, `API Key` and
/// `api-key` are one name.
fn key(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The term a name means: its id, its title or one of the names it goes by.
pub fn find<'a>(all: &'a [Term], name: &str) -> Option<&'a Term> {
    let k = key(name);
    if k.is_empty() {
        return None;
    }
    all.iter()
        .find(|t| key(&t.id) == k || key(&t.term) == k || t.aliases.iter().any(|a| key(a) == k))
}

/// Terms whose title or names contain what was typed, for a "did you mean".
pub fn near<'a>(all: &'a [Term], name: &str) -> Vec<&'a Term> {
    let k = key(name);
    if k.len() < 2 {
        return Vec::new();
    }
    all.iter()
        .filter(|t| {
            key(&t.term).contains(&k)
                || key(&t.id).contains(&k)
                || t.aliases.iter().any(|a| key(a).contains(&k))
        })
        .collect()
}

fn show(t: &Term) -> Value {
    json!({
        "id": t.id,
        "term": t.term,
        "category": t.category,
        "aliases": t.aliases,
        "definition": t.definition,
        "general": t.general,
        "example": t.example,
        "see_also": t.see_also,
    })
}

fn print_term(t: &Term) {
    println!("{} ({})", t.term, t.category);
    println!();
    println!("{}", t.definition);
    if let Some(g) = &t.general {
        println!("\nIn general use: {g}");
    }
    if let Some(e) = &t.example {
        println!("\nExample: {e}");
    }
    if !t.aliases.is_empty() {
        println!("\nAlso written: {}", t.aliases.join(", "));
    }
    if !t.see_also.is_empty() {
        println!("\nSee also: {}", t.see_also.join(", "));
    }
}

/// With no term, a summary; with `--category`, that category's terms; with a term, its meaning.
pub fn run(term: Option<&str>, category: Option<&str>) -> CliResult {
    let all = terms();
    if let Some(name) = term {
        if let Some(t) = find(&all, name) {
            out::ok("glossary", show(t), || print_term(t));
            return Ok(());
        }
        let close = near(&all, name);
        return Err(match close.as_slice() {
            [] => CliError::not_found(format!(
                "No glossary term '{name}'. `unv glossary` lists the categories."
            )),
            many => CliError::not_found(format!(
                "No glossary term '{name}'. Did you mean: {}?",
                many.iter()
                    .take(8)
                    .map(|t| t.term.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        });
    }
    if let Some(c) = category {
        let want = key(c);
        let list: Vec<&Term> = all.iter().filter(|t| key(&t.category) == want).collect();
        if list.is_empty() {
            return Err(CliError::not_found(format!(
                "No glossary category '{c}'. `unv glossary` lists the categories."
            )));
        }
        out::ok(
            "glossary.list",
            json!({ "category": c, "terms": list.iter().map(|t| json!({ "id": t.id, "term": t.term })).collect::<Vec<_>>() }),
            || {
                for t in &list {
                    println!("{}", t.term);
                }
            },
        );
        return Ok(());
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &all {
        *counts.entry(t.category.as_str()).or_default() += 1;
    }
    out::ok(
        "glossary.summary",
        json!({ "terms": all.len(), "categories": counts }),
        || {
            println!("{} terms.", all.len());
            for (c, n) in &counts {
                println!("  {c:<12} {n}");
            }
            println!(
                "\nunv glossary TERM shows one; unv glossary --category CATEGORY lists a category."
            );
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_file_parses_and_every_id_is_unique() {
        let all = terms();
        assert!(all.len() > 100, "{} terms", all.len());
        let mut ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
    }

    #[test]
    fn a_term_is_found_by_its_title_its_id_or_a_name_it_goes_by() {
        let all = terms();
        let by_title = find(&all, "Key Pool").map(|t| t.id.clone());
        assert_eq!(by_title.as_deref(), Some("pool"));
        // The stored field name, written either way, finds the same term.
        assert_eq!(
            find(&all, "api_key").map(|t| t.id.as_str()),
            Some("api-key")
        );
        assert_eq!(
            find(&all, "API-KEY").map(|t| t.id.as_str()),
            Some("api-key")
        );
        assert!(find(&all, "").is_none());
        assert!(find(&all, "no such term at all").is_none());
    }

    #[test]
    fn a_near_miss_suggests_terms_and_one_letter_suggests_nothing() {
        let all = terms();
        let close = near(&all, "rate limit");
        assert!(close.iter().any(|t| t.id == "rate-limit"), "{close:?}");
        assert!(near(&all, "x").is_empty());
    }

    #[test]
    fn an_unknown_term_is_not_found_with_a_suggestion_when_there_is_one() {
        let e = run(Some("rate lim"), None).unwrap_err();
        assert_eq!(e.code as i32, 3);
        assert!(e.message.contains("Did you mean"), "{}", e.message);
        let e = run(Some("zzzzqqq"), None).unwrap_err();
        assert!(!e.message.contains("Did you mean"), "{}", e.message);
        assert!(run(None, Some("no-such-category")).is_err());
    }
}
