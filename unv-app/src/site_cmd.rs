// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv site`: the places an entry is used (R40). The logic is `vault_core::sites`; this file is
//! I/O and policy. Addresses are public metadata, so every command here prints in clear.

use crate::access::Access;
use crate::data::{self, entries, entries_mut, find_entry_index};
use crate::error::{CliError, CliResult};
use crate::out;
use serde_json::{json, Value};
use vault_core::sites as vs;

fn site_json(e: &vs::EntrySite) -> Value {
    json!({
        "url": e.site.display(), "kind": e.kind, "mode": format!("{:?}", e.mode).to_lowercase(),
        "label": e.label, "source": e.source,
    })
}

pub fn add(
    access: &Access,
    query: &str,
    url: &str,
    kind: Option<&str>,
    mode: Option<&str>,
    label: Option<&str>,
) -> CliResult {
    let mut vault = access.load_vault()?;
    let idx = find_entry_index(&vault, query)?;
    let value = vs::site_value(url, kind, mode, label.unwrap_or(""), "manual")
        .map_err(CliError::invalid)?;
    let added = vs::add_site(&mut entries_mut(&mut vault)[idx], value.clone());
    if added {
        access.save(&vault)?;
    }
    out::ok("site.add", json!({ "added": added, "site": value }), || {
        if added {
            println!("Added {}", value["url"].as_str().unwrap_or(""));
        } else {
            println!("Already there: {}", value["url"].as_str().unwrap_or(""));
        }
    });
    Ok(())
}

pub fn rm(access: &Access, query: &str, url: &str) -> CliResult {
    let mut vault = access.load_vault()?;
    let idx = find_entry_index(&vault, query)?;
    let removed = vs::remove_site(&mut entries_mut(&mut vault)[idx], url).map_err(CliError::invalid)?;
    if !removed {
        return Err(CliError::not_found(format!("No site '{url}' on '{query}'")));
    }
    access.save(&vault)?;
    out::ok("site.rm", json!({ "removed": true }), || println!("Removed"));
    Ok(())
}

pub fn ls(access: &Access, query: &str) -> CliResult {
    let vault = access.load_vault()?;
    let entry = &entries(&vault)[find_entry_index(&vault, query)?];
    let list: Vec<Value> = vs::entry_sites(entry).iter().map(site_json).collect();
    let family = entry.get("site_family").and_then(Value::as_str).unwrap_or("");
    out::ok("site.ls", json!({ "sites": list, "family": family }), || {
        for s in &list {
            println!(
                "{}  {}  {}  {}",
                s["url"].as_str().unwrap_or(""),
                s["kind"].as_str().unwrap_or(""),
                s["mode"].as_str().unwrap_or(""),
                s["label"].as_str().unwrap_or("")
            );
        }
        if !family.is_empty() {
            println!("family: {family}");
        }
    });
    Ok(())
}

/// Which entries are offered at `origin`: names and ids only, never a value.
pub fn matches(access: &Access, origin: &str) -> CliResult {
    let vault = access.load_vault()?;
    let mut origin_site = vs::parse_site(origin).map_err(CliError::invalid)?;
    if origin_site.scheme.is_none() {
        origin_site.scheme = Some("https".into());
    }
    let fams = vs::families(&vault);
    let hits: Vec<Value> = entries(&vault)
        .iter()
        .filter(|e| vs::entry_offered(e, &fams, &origin_site))
        .map(|e| json!({ "id": data::id_of(e), "name": data::provider_of(e) }))
        .collect();
    out::ok("site.match", json!({ "origin": origin_site.display(), "entries": hits }), || {
        if hits.is_empty() {
            println!("Nothing is offered at {}", origin_site.display());
        }
        for h in &hits {
            println!("{}  {}", h["id"].as_str().unwrap_or(""), h["name"].as_str().unwrap_or(""));
        }
    });
    Ok(())
}

pub fn family_ls(access: &Access) -> CliResult {
    let vault = access.load_vault()?;
    let fams = vs::families(&vault);
    let list: Vec<Value> = fams.iter().map(|f| json!({ "name": f.name, "hosts": f.hosts })).collect();
    out::ok("site.family.ls", json!({ "families": list }), || {
        for f in &fams {
            println!("{}: {}", f.name, f.hosts.join(", "));
        }
    });
    Ok(())
}

pub fn family_add(access: &Access, name: &str, hosts: &[String]) -> CliResult {
    let mut vault = access.load_vault()?;
    vs::family_add(&mut vault, name, hosts).map_err(CliError::invalid)?;
    access.save(&vault)?;
    out::ok("site.family.add", json!({ "family": name }), || println!("Family {name} updated"));
    Ok(())
}

pub fn family_rm(access: &Access, name: &str, host: Option<&str>) -> CliResult {
    let mut vault = access.load_vault()?;
    if !vs::family_remove(&mut vault, name, host).map_err(CliError::invalid)? {
        return Err(CliError::not_found(format!("No family '{name}' (or host) to remove")));
    }
    access.save(&vault)?;
    out::ok("site.family.rm", json!({ "family": name }), || println!("Removed"));
    Ok(())
}

/// Point an entry at a family (`None` clears it).
pub fn family_use(access: &Access, query: &str, name: Option<&str>) -> CliResult {
    let mut vault = access.load_vault()?;
    if let Some(n) = name {
        if !vs::families(&vault).iter().any(|f| f.name == n) {
            return Err(CliError::not_found(format!("No family '{n}' (see `unv site family ls`)")));
        }
    }
    let idx = find_entry_index(&vault, query)?;
    let entry = &mut entries_mut(&mut vault)[idx];
    match name {
        Some(n) => entry["site_family"] = json!(n),
        None => {
            if let Some(o) = entry.as_object_mut() {
                o.remove("site_family");
            }
        }
    }
    access.save(&vault)?;
    out::ok("site.family.use", json!({ "family": name }), || println!("Done"));
    Ok(())
}
