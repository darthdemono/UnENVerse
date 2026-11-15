// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv recipe`: password recipes (R41). The engine is `vault_core::recipe`; this file is I/O and
//! policy. A recipe makes an ordinary password, so a stored result is never a fragment, and the
//! generated values are redacted like any other secret unless `--reveal` is passed.

use crate::access::Access;
use crate::data::{self, entries, entries_mut, find_entry_index};
use crate::error::{CliError, CliResult};
use crate::out;
use serde_json::{json, Value};
use vault_core::recipe as rc;

/// The recipe-level settings, as flags.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct RecipeOpts {
    /// The least characters the password may have (shorter ones are padded).
    #[arg(long)]
    pub min: Option<usize>,
    /// The most characters it may have (marked `?N` parts are dropped to fit).
    #[arg(long)]
    pub max: Option<usize>,
    /// What to do when the length is outside the bounds: fit (default) or refuse.
    #[arg(long, value_parser = ["fit", "refuse"])]
    pub policy: Option<String>,
    /// Classes that must appear, with an optional count: `upper+digit` or `upper:2+symbol`.
    #[arg(long)]
    pub require: Option<String>,
    /// Leave out 0 O 1 l I.
    #[arg(long)]
    pub avoid_ambiguous: bool,
    /// Characters that must not appear.
    #[arg(long)]
    pub forbid: Option<String>,
    /// The only characters allowed.
    #[arg(long)]
    pub allow: Option<String>,
    /// The longest run of one repeated character.
    #[arg(long)]
    pub max_run: Option<usize>,
    /// No sequences such as abc or 123.
    #[arg(long)]
    pub no_sequences: bool,
    /// Start with a letter.
    #[arg(long)]
    pub start_letter: bool,
    /// No symbol at the start or end.
    #[arg(long)]
    pub no_edge_symbol: bool,
    /// Group the result every N characters.
    #[arg(long)]
    pub chunk: Option<usize>,
    /// The separator between groups (default `-`).
    #[arg(long)]
    pub chunk_sep: Option<String>,
    /// The least random bits a result may have (default 64).
    #[arg(long)]
    pub floor: Option<f64>,
}

impl RecipeOpts {
    fn any(&self) -> bool {
        self.min.is_some()
            || self.max.is_some()
            || self.policy.is_some()
            || self.require.is_some()
            || self.avoid_ambiguous
            || self.forbid.is_some()
            || self.allow.is_some()
            || self.max_run.is_some()
            || self.no_sequences
            || self.start_letter
            || self.no_edge_symbol
            || self.chunk.is_some()
            || self.chunk_sep.is_some()
            || self.floor.is_some()
    }

    /// Applies the flags over `base` and returns the result.
    pub fn apply(&self, mut o: rc::Options) -> CliResult<rc::Options> {
        if let Some(v) = self.min {
            o.min_len = Some(v);
        }
        if let Some(v) = self.max {
            o.max_len = Some(v);
        }
        if let Some(v) = &self.policy {
            o.policy = v.clone();
        }
        if let Some(spec) = &self.require {
            for part in spec.split('+').filter(|p| !p.is_empty()) {
                let (class, n) = match part.split_once(':') {
                    Some((c, n)) => (
                        c,
                        n.parse::<usize>().map_err(|_| {
                            CliError::invalid(format!("--require: `{n}` is not a count"))
                        })?,
                    ),
                    None => (part, 1),
                };
                match class {
                    "upper" => o.require_upper = n,
                    "lower" => o.require_lower = n,
                    "digit" => o.require_digit = n,
                    "symbol" => o.require_symbol = n,
                    other => {
                        return Err(CliError::invalid(format!(
                            "--require: `{other}` is upper, lower, digit or symbol"
                        )))
                    }
                }
            }
        }
        o.avoid_ambiguous |= self.avoid_ambiguous;
        if let Some(v) = &self.forbid {
            o.forbid = v.clone();
        }
        if let Some(v) = &self.allow {
            o.allow = v.clone();
        }
        if let Some(v) = self.max_run {
            o.max_run = v;
        }
        o.no_sequences |= self.no_sequences;
        o.start_letter |= self.start_letter;
        o.no_edge_symbol |= self.no_edge_symbol;
        if let Some(v) = self.chunk {
            o.chunk = v;
        }
        if let Some(v) = &self.chunk_sep {
            o.chunk_sep = v.clone();
        }
        if let Some(v) = self.floor {
            if v < 0.0 {
                return Err(CliError::invalid("--floor cannot be negative"));
            }
            o.floor_bits = Some(v);
        }
        Ok(o)
    }
}

fn now() -> String {
    vault_core::iso_now().chars().take(10).collect()
}

fn invalid(e: String) -> CliError {
    CliError::invalid(e)
}

fn draw() -> rc::PoolDraw {
    rc::PoolDraw::new(crate::gen::current())
}

fn options_json(o: &rc::Options) -> Value {
    serde_json::to_value(o).unwrap_or(Value::Null)
}

/// `unv recipe tokens`: every token the engine understands, from the registry.
pub fn tokens(group: Option<&str>) -> CliResult {
    let reg = rc::registry();
    let all: Vec<Value> = reg["tokens"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            group.is_none_or(|g| {
                t["group"]
                    .as_str()
                    .is_some_and(|x| x.eq_ignore_ascii_case(g))
            })
        })
        .collect();
    out::ok("recipe.tokens", json!({ "tokens": all }), || {
        for t in &all {
            println!(
                "{:<20} {:<18} {}",
                t["id"].as_str().unwrap_or(""),
                t["group"].as_str().unwrap_or(""),
                t["summary"].as_str().unwrap_or("")
            );
        }
    });
    Ok(())
}

pub fn ls(access: &Access) -> CliResult {
    let vault = access.load_vault()?;
    let all = rc::saved(&vault);
    let default = vault
        .get("recipe_default")
        .and_then(Value::as_str)
        .unwrap_or("");
    let rows: Vec<Value> = all
        .iter()
        .map(|r| json!({ "name": r.name, "template": r.template, "default": r.name == default }))
        .collect();
    out::ok("recipe.ls", json!({ "recipes": rows }), || {
        for r in &all {
            let mark = if r.name == default { " (default)" } else { "" };
            println!("{}{mark}: {}", r.name, r.template);
        }
    });
    Ok(())
}

pub fn show(access: &Access, name: &str) -> CliResult {
    let vault = access.load_vault()?;
    let r = rc::saved(&vault)
        .into_iter()
        .find(|r| r.name == name)
        .ok_or_else(|| CliError::not_found(format!("No recipe named '{name}'")))?;
    out::ok(
        "recipe.show",
        json!({ "name": r.name, "template": r.template, "options": options_json(&r.options) }),
        || println!("{}\n{}", r.template, options_json(&r.options)),
    );
    Ok(())
}

pub fn add(
    access: &Access,
    name: &str,
    template: &str,
    flags: &RecipeOpts,
    replace: bool,
) -> CliResult {
    let mut vault = access.load_vault()?;
    let base = if replace && !flags.any() {
        rc::saved(&vault)
            .into_iter()
            .find(|r| r.name == name)
            .map(|r| r.options)
            .unwrap_or_default()
    } else {
        rc::Options::default()
    };
    let options = flags.apply(base)?;
    rc::save(&mut vault, name, template, &options, replace).map_err(|e| {
        if e.contains("--replace") {
            CliError::conflict(e)
        } else {
            invalid(e)
        }
    })?;
    access.save(&vault)?;
    out::ok("recipe.add", json!({ "name": name }), || {
        println!("Saved recipe {name}")
    });
    Ok(())
}

pub fn rm(access: &Access, name: &str) -> CliResult {
    let mut vault = access.load_vault()?;
    if !rc::remove(&mut vault, name) {
        return Err(CliError::not_found(format!("No recipe named '{name}'")));
    }
    access.save(&vault)?;
    out::ok("recipe.rm", json!({ "name": name }), || {
        println!("Removed recipe {name}")
    });
    Ok(())
}

pub fn set_default(access: &Access, name: &str) -> CliResult {
    let mut vault = access.load_vault()?;
    if name.is_empty() {
        if let Some(o) = vault.as_object_mut() {
            o.remove("recipe_default");
        }
    } else {
        if !rc::saved(&vault).iter().any(|r| r.name == name) {
            return Err(CliError::not_found(format!("No recipe named '{name}'")));
        }
        vault["recipe_default"] = json!(name);
    }
    access.save(&vault)?;
    out::ok("recipe.default", json!({ "default": name }), || {
        println!("Done")
    });
    Ok(())
}

fn generated_json(g: &rc::Generated) -> Value {
    json!({
        "bits": g.bits.round(),
        "private_bits": g.private_bits.round(),
        "length": g.text.chars().count(),
        "parts": g.parts,
        "dropped": g.dropped,
        "warnings": g.warnings,
        "rebuildable_from_memory": g.rebuildable_from_memory,
    })
}

/// Makes samples from a saved recipe (by name) or a template, with an entry's facts when `--for`.
pub fn test(
    access: &Access,
    target: &str,
    for_entry: Option<&str>,
    count: usize,
    flags: &RecipeOpts,
) -> CliResult {
    let vault = access.load_vault()?;
    let (template, base) = match rc::saved(&vault).into_iter().find(|r| r.name == target) {
        Some(r) => (r.template, r.options),
        None => (target.to_string(), rc::Options::default()),
    };
    let options = flags.apply(base)?;
    let entry = match for_entry {
        Some(q) => Some(entries(&vault)[find_entry_index(&vault, q)?].clone()),
        None => None,
    };
    let ctx = rc::context(&vault, entry.as_ref(), &now());
    let n = count.clamp(1, 10);
    let mut rng = draw();
    let mut samples = Vec::new();
    let mut last = None;
    for _ in 0..n {
        let g = rc::make(&template, &options, &ctx, &mut rng).map_err(invalid)?;
        samples.push(g.text.clone());
        last = Some(g);
    }
    let g = last.expect("at least one sample");
    let reveal = out::revealing();
    let shown: Vec<Value> = samples
        .iter()
        .map(|s| {
            if reveal {
                json!(s)
            } else {
                out::masked_json(s)
            }
        })
        .collect();
    let mut data = generated_json(&g);
    data["samples"] = json!(shown);
    out::ok("recipe.test", data, || {
        for s in &samples {
            println!("{}", if reveal { s.clone() } else { out::masked(s) });
        }
        println!(
            "{:.0} bits from the random parts ({} private); rebuildable from memory: {}",
            g.bits,
            g.private_bits.round(),
            if g.rebuildable_from_memory {
                "yes"
            } else {
                "no"
            }
        );
        for w in &g.warnings {
            println!("note: {w}");
        }
        if !reveal {
            eprintln!("Redacted. Use --reveal to print the samples.");
        }
    });
    Ok(())
}

/// Generates a password for an entry and stores it as its ordinary password.
pub fn apply(access: &Access, query: &str, recipe: Option<&str>) -> CliResult {
    let mut vault = access.load_vault()?;
    let idx = find_entry_index(&vault, query)?;
    let entry = entries(&vault)[idx].clone();
    let (saved, why) = match recipe {
        Some(name) => (
            rc::saved(&vault)
                .into_iter()
                .find(|r| r.name == name)
                .ok_or_else(|| CliError::not_found(format!("No recipe named '{name}'")))?,
            "named".to_string(),
        ),
        None => rc::resolve(&vault, &entry).ok_or_else(|| {
            CliError::not_found(
                "No recipe applies to this entry (pass --recipe, add a rule or set a default)",
            )
        })?,
    };
    let ctx = rc::context(&vault, Some(&entry), &now());
    let g = rc::make(&saved.template, &saved.options, &ctx, &mut draw()).map_err(invalid)?;
    let target = &mut entries_mut(&mut vault)[idx];
    target["api_key"] = json!(g.text);
    target["generated_by"] = rc::generated_by(&saved.name, ctx.counter, &now());
    access.save(&vault)?;
    let provider = data::provider_of(&entries(&vault)[idx]).to_string();
    let mut data = generated_json(&g);
    data["recipe"] = json!(saved.name);
    data["why"] = json!(why);
    data["value"] = out::masked_json(&g.text);
    out::ok("recipe.apply", data, || {
        println!(
            "'{provider}' now holds a password from recipe {} ({why}): {:.0} bits, {}",
            saved.name,
            g.bits,
            out::masked(&g.text)
        );
    });
    Ok(())
}

pub fn rule_add(access: &Access, kind: &str, value: &str, recipe: &str) -> CliResult {
    let mut vault = access.load_vault()?;
    rc::add_rule(&mut vault, kind, value, recipe).map_err(invalid)?;
    access.save(&vault)?;
    out::ok(
        "recipe.rule.add",
        json!({ "kind": kind, "value": value }),
        || println!("Rule added"),
    );
    Ok(())
}

pub fn rule_ls(access: &Access) -> CliResult {
    let vault = access.load_vault()?;
    let all = rc::rules(&vault);
    let rows: Vec<Value> = all
        .iter()
        .enumerate()
        .map(|(i, (k, v, r))| json!({ "n": i + 1, "kind": k, "value": v, "recipe": r }))
        .collect();
    out::ok("recipe.rule.ls", json!({ "rules": rows }), || {
        for (i, (k, v, r)) in all.iter().enumerate() {
            println!("{}. {k} {v} -> {r}", i + 1);
        }
    });
    Ok(())
}

pub fn rule_rm(access: &Access, n: usize) -> CliResult {
    let mut vault = access.load_vault()?;
    if !rc::remove_rule(&mut vault, n) {
        return Err(CliError::not_found(format!("No rule number {n}")));
    }
    access.save(&vault)?;
    out::ok("recipe.rule.rm", json!({ "n": n }), || {
        println!("Rule removed")
    });
    Ok(())
}

/// Which recipe applies to an entry, and why. Names only.
pub fn rule_test(access: &Access, query: &str) -> CliResult {
    let vault = access.load_vault()?;
    let entry = entries(&vault)[find_entry_index(&vault, query)?].clone();
    let hit = rc::resolve(&vault, &entry);
    let data = match &hit {
        Some((r, why)) => json!({ "recipe": r.name, "why": why }),
        None => json!({ "recipe": null, "why": "plain random (no recipe applies)" }),
    };
    out::ok("recipe.rule.test", data, || match &hit {
        Some((r, why)) => println!("{} ({why})", r.name),
        None => println!("No recipe applies: plain random"),
    });
    Ok(())
}

pub fn list_add(
    access: &Access,
    name: &str,
    words: &[String],
    file: Option<&str>,
    private: bool,
) -> CliResult {
    let mut all: Vec<String> = words.to_vec();
    if let Some(f) = file {
        let text = std::fs::read_to_string(f)
            .map_err(|e| CliError::not_found(format!("cannot read {f}: {e}")))?;
        all.extend(text.lines().map(String::from));
    }
    let mut vault = access.load_vault()?;
    let n = rc::save_list(&mut vault, name, &all, private).map_err(invalid)?;
    access.save(&vault)?;
    let bits = (n as f64).log2();
    out::ok(
        "recipe.list.add",
        json!({ "name": name, "words": n, "bits_per_pick": bits }),
        || {
            println!("List {name}: {n} words, {bits:.1} bits a pick");
            if n < 256 {
                println!("note: under 256 words is a small list; use more picks or a longer list");
            }
        },
    );
    Ok(())
}

pub fn list_ls(access: &Access) -> CliResult {
    let vault = access.load_vault()?;
    let mine = rc::custom_lists(&vault);
    let mut rows: Vec<Value> = rc::builtin_lists()
        .iter()
        .map(|(n, w)| json!({ "name": n, "words": w.len(), "builtin": true, "private": false }))
        .collect();
    rows.extend(
        mine.iter().map(
            |(n, w, p)| json!({ "name": n, "words": w.len(), "builtin": false, "private": p }),
        ),
    );
    out::ok("recipe.list.ls", json!({ "lists": rows }), || {
        for r in &rows {
            println!(
                "{:<12} {:>4} words{}{}",
                r["name"].as_str().unwrap_or(""),
                r["words"],
                if r["builtin"] == true {
                    "  built in"
                } else {
                    ""
                },
                if r["private"] == true {
                    "  private"
                } else {
                    ""
                }
            );
        }
    });
    Ok(())
}

pub fn list_rm(access: &Access, name: &str) -> CliResult {
    let mut vault = access.load_vault()?;
    if !rc::remove_list(&mut vault, name) {
        return Err(CliError::not_found(format!(
            "No list of yours named '{name}'"
        )));
    }
    access.save(&vault)?;
    out::ok("recipe.list.rm", json!({ "name": name }), || {
        println!("List removed")
    });
    Ok(())
}

pub fn fact_set(access: &Access, name: &str, value: &str, public: bool) -> CliResult {
    let mut vault = access.load_vault()?;
    rc::set_fact(&mut vault, name, value, !public).map_err(invalid)?;
    access.save(&vault)?;
    out::ok(
        "recipe.fact.set",
        json!({ "name": name, "private": !public }),
        || println!("Saved fact {name}"),
    );
    Ok(())
}

pub fn fact_ls(access: &Access) -> CliResult {
    let vault = access.load_vault()?;
    let rows: Vec<Value> = rc::facts(&vault)
        .iter()
        .map(|(n, v, p)| {
            json!({ "name": n, "private": p, "value": if *p && !out::revealing() { out::masked_json(v) } else { json!(v) } })
        })
        .collect();
    out::ok("recipe.fact.ls", json!({ "facts": rows }), || {
        for r in &rows {
            println!(
                "{} {}",
                r["name"].as_str().unwrap_or(""),
                if r["private"] == true {
                    "(private)"
                } else {
                    ""
                }
            );
        }
    });
    Ok(())
}

pub fn fact_rm(access: &Access, name: &str) -> CliResult {
    let mut vault = access.load_vault()?;
    if !rc::remove_fact(&mut vault, name) {
        return Err(CliError::not_found(format!("No fact named '{name}'")));
    }
    access.save(&vault)?;
    out::ok("recipe.fact.rm", json!({ "name": name }), || {
        println!("Fact removed")
    });
    Ok(())
}
