// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv emit` and `unv codes` — what the Phase 24.5 types do beyond holding
//! fields. The logic is `vault_core::type_emit`; this file is I/O and policy.
//!
//! **`emit` is a materialising path.** The output is a `.npmrc`, a Docker
//! `config.json`, a DSN or a Wi-Fi string, so it holds the credential: refused to
//! stdout unless `--reveal`, written `0600` by `--out` (Phase 14, unchanged).
//! The one exception is a format that is public by construction (`jwks`: the
//! public half of a signing key), which prints.

use crate::access::Access;
use crate::data::{self, entries_mut, find_entry_index};
use crate::error::{CliError, CliResult};
use crate::out;
use serde_json::{json, Value};
use vault_core::type_emit as te;

pub fn emit(
    access: &Access,
    query: &str,
    format: Option<&str>,
    out_path: Option<&std::path::Path>,
) -> CliResult {
    let vault = access.load_vault()?;
    let entry = data::entries(&vault)[find_entry_index(&vault, query)?].clone();
    let ty = data::secret_type_of(&entry).to_string();
    let offered = te::formats_for(&ty);
    let Some(format) = format else {
        // Listing formats reveals nothing, so it needs no --reveal.
        out::ok(
            "emit.formats",
            json!({ "type": ty, "formats": offered }),
            || {
                if offered.is_empty() {
                    println!("A {ty} entry has no emit formats.");
                } else {
                    println!("{}", offered.join("\n"));
                }
            },
        );
        return Ok(());
    };
    // Refuse for the data first (equally true with --out), then for the policy.
    let text = te::emit(&entry, format).map_err(CliError::invalid)?;
    if out_path.is_none() && !out::revealing() && !te::is_public_format(format) {
        return Err(out::refuse_reveal(&format!("A {format} file")));
    }
    match out_path {
        Some(p) => {
            vault_core::write_private(p, text.as_bytes())
                .map_err(|e| CliError::from(format!("Cannot write {}: {e}", p.display())))?;
            out::ok(
                "emit",
                json!({ "format": format, "written": p.display().to_string() }),
                || println!("Wrote {} (0600)", p.display()),
            );
        }
        None => crate::fmt::emit(&text, None)?,
    }
    Ok(())
}

fn codes_entry(vault: &Value, query: &str) -> CliResult<usize> {
    let idx = find_entry_index(vault, query)?;
    let e = &data::entries(vault)[idx];
    // Any entry can carry codes (a password card holds its own); the type does not matter.
    if data::secret_type_of(e) != "recovery_codes" && te::var(e, "codes").is_empty() {
        return Err(CliError::invalid(format!(
            "'{query}' has no recovery codes (add a `codes` value or use a recovery_codes entry)"
        )));
    }
    Ok(idx)
}

pub fn codes_status(access: &Access, query: &str) -> CliResult {
    let vault = access.load_vault()?;
    let e = &data::entries(&vault)[codes_entry(&vault, query)?];
    let st = te::code_status(e);
    out::ok(
        "codes.status",
        json!({ "total": st.total, "remaining": st.remaining, "low": st.remaining <= 2 }),
        || {
            println!(
                "{} of {} unused{}",
                st.remaining,
                st.total,
                if st.remaining <= 2 {
                    " (running low)"
                } else {
                    ""
                }
            )
        },
    );
    Ok(())
}

/// The next unused code. **Reading never consumes it**; `codes use` does.
pub fn codes_next(access: &Access, query: &str) -> CliResult {
    let vault = access.load_vault()?;
    let e = &data::entries(&vault)[codes_entry(&vault, query)?];
    let code = te::next_code(e).ok_or_else(|| CliError::not_found("No unused codes remain"))?;
    if !out::revealing() {
        return Err(out::refuse_reveal("A recovery code"));
    }
    out::ok("codes.next", json!({ "code": code }), || println!("{code}"));
    Ok(())
}

pub fn codes_use(access: &Access, query: &str, code: Option<&str>) -> CliResult {
    let mut vault = access.load_vault()?;
    let idx = codes_entry(&vault, query)?;
    // The full timestamp: "used when" is worth more than "used which day".
    let date = vault_core::iso_now();
    let new = te::mark_used(&data::entries(&vault)[idx], code, &date).map_err(CliError::invalid)?;
    let entry = &mut entries_mut(&mut vault)[idx];
    let vars = entry
        .get_mut("extra_vars")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| CliError::invalid("entry has no `codes` variable"))?;
    let slot = vars
        .iter_mut()
        .find(|v| v.get("key").and_then(Value::as_str) == Some("codes"))
        .ok_or_else(|| CliError::invalid("entry has no `codes` variable"))?;
    slot["value"] = json!(new);
    let st = te::code_status(&data::entries(&vault)[idx]);
    access.save(&vault)?;
    out::ok("codes.use", json!({ "remaining": st.remaining }), || {
        println!("Marked used; {} remaining", st.remaining)
    });
    Ok(())
}

/// Put a used code back: the undo for a code a page rejected after it was marked.
pub fn codes_unuse(access: &Access, query: &str, code: &str) -> CliResult {
    let mut vault = access.load_vault()?;
    let idx = codes_entry(&vault, query)?;
    let new = te::unmark_used(&data::entries(&vault)[idx], code).map_err(CliError::invalid)?;
    set_codes(&mut vault, idx, &new)?;
    let st = te::code_status(&data::entries(&vault)[idx]);
    access.save(&vault)?;
    out::ok("codes.unuse", json!({ "remaining": st.remaining }), || {
        println!("Restored; {} remaining", st.remaining)
    });
    Ok(())
}

/// Add pasted codes (a file, or `-` for stdin) to an entry. Nothing is printed back.
pub fn codes_import(access: &Access, query: &str, source: &str) -> CliResult {
    let text = if source == "-" {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .map_err(|e| CliError::from(format!("Cannot read stdin: {e}")))?;
        buf
    } else {
        std::fs::read_to_string(source)
            .map_err(|e| CliError::from(format!("Cannot read {source}: {e}")))?
    };
    let found = te::parse_codes(&text);
    if found.is_empty() {
        return Err(CliError::invalid("No recovery codes found in that text"));
    }
    let mut vault = access.load_vault()?;
    let idx = codes_entry(&vault, query)?;
    let (new, added) = te::add_codes(&data::entries(&vault)[idx], &found);
    set_codes(&mut vault, idx, &new)?;
    let st = te::code_status(&data::entries(&vault)[idx]);
    access.save(&vault)?;
    out::ok(
        "codes.import",
        json!({ "found": found.len(), "added": added, "remaining": st.remaining }),
        || {
            println!(
                "{added} added ({} found); {} unused",
                found.len(),
                st.remaining
            )
        },
    );
    Ok(())
}

/// Write `value` into the entry's `codes` variable, creating the variable when it is missing.
fn set_codes(vault: &mut Value, idx: usize, value: &str) -> CliResult {
    let entry = &mut entries_mut(vault)[idx];
    if entry.get("extra_vars").and_then(Value::as_array).is_none() {
        entry["extra_vars"] = json!([]);
    }
    let vars = entry["extra_vars"].as_array_mut().expect("just made");
    match vars
        .iter_mut()
        .find(|v| v.get("key").and_then(Value::as_str) == Some("codes"))
    {
        Some(slot) => slot["value"] = json!(value),
        None => vars.push(json!({ "key": "codes", "value": value })),
    }
    Ok(())
}
