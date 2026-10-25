// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! `unv plugin`: find, install, enable, disable, remove and test plugins (`unv-plugin-rpc/1`).
//!
//! Installing records the program's hash and enables nothing. Enabling is the user's decision, made
//! after being shown what the plugin declares and that it runs with their rights.

use crate::access::{self, Access};
use crate::error::{CliError, CliResult};
use crate::fmt;
use crate::out;
use crate::plugin_host;
use serde_json::json;
use std::path::{Path, PathBuf};
use vault_core::plugin::{self, Discovered, Manifest};

pub fn plugins_dir(over: Option<&Path>) -> PathBuf {
    over.map(Path::to_path_buf).unwrap_or_else(|| {
        access::default_db_path()
            .parent()
            .map(|p| p.join("plugins"))
            .unwrap_or_else(|| PathBuf::from("plugins"))
    })
}

fn conn(access: &Access) -> CliResult<vault_core::SqlConnection> {
    let c = access.conn()?;
    plugin::init_schema(&c).map_err(CliError::from)?;
    Ok(c)
}

fn find(dir: &Path, name: &str) -> CliResult<Discovered> {
    let (found, problems) = plugin::discover(dir);
    found
        .into_iter()
        .find(|d| d.manifest.name == name)
        .ok_or_else(|| {
            let hint = if problems.is_empty() {
                String::new()
            } else {
                format!(" ({})", problems.join("; "))
            };
            CliError::not_found(format!("No plugin '{name}' in {}{hint}", dir.display()))
        })
}

pub fn ls(access: &Access, dir: Option<&Path>) -> CliResult {
    let c = conn(access)?;
    let installed = plugin::list(&c).map_err(CliError::from)?;
    let dir = plugins_dir(dir);
    let (found, problems) = plugin::discover(&dir);
    let available: Vec<_> = found
        .iter()
        .filter(|d| !installed.iter().any(|i| i.name == d.manifest.name))
        .map(|d| json!({ "name": d.manifest.name, "version": d.manifest.version, "capabilities": d.manifest.capabilities }))
        .collect();
    out::ok(
        "plugin.ls",
        json!({ "dir": dir, "installed": installed, "available": available, "problems": problems }),
        || {
            if installed.is_empty() && available.is_empty() {
                println!("No plugins in {}.", dir.display());
            }
            for i in &installed {
                println!(
                    "{} {} [{}] {}  sha256:{}",
                    i.name,
                    i.version,
                    i.capabilities.join(", "),
                    if i.enabled { "enabled" } else { "disabled" },
                    &i.hash[..12.min(i.hash.len())]
                );
            }
            for a in &available {
                println!(
                    "{} {} (not installed)",
                    a["name"].as_str().unwrap_or(""),
                    a["version"].as_str().unwrap_or("")
                );
            }
            for p in &problems {
                println!("problem: {p}");
            }
        },
    );
    Ok(())
}

pub fn install(access: &Access, dir: Option<&Path>, name: &str) -> CliResult {
    let c = conn(access)?;
    let found = find(&plugins_dir(dir), name)?;
    if crate::out::dry_run() {
        out::ok(
            "plugin.install",
            json!({ "name": name, "dry_run": true }),
            || {
                println!("Would install {name} (disabled)");
            },
        );
        return Ok(());
    }
    let row = plugin::install(&c, &found).map_err(CliError::from)?;
    out::ok("plugin.install", json!({ "plugin": row }), || {
        println!(
            "Installed {} {} (disabled). Enable it with: unv plugin enable {}",
            row.name, row.version, row.name
        );
    });
    Ok(())
}

pub fn enable(access: &Access, name: &str, yes: bool) -> CliResult {
    enable_with(access, name, &mut |question| fmt::confirm(question, yes))
}

/// [`enable`] with the question put to `ask`, so a "no" is as testable as a "yes".
pub fn enable_with(
    access: &Access,
    name: &str,
    ask: &mut dyn FnMut(&str) -> CliResult<bool>,
) -> CliResult {
    let c = conn(access)?;
    let row = plugin::get(&c, name)
        .map_err(CliError::from)?
        .ok_or_else(|| CliError::not_found(format!("No installed plugin '{name}'")))?;
    let question = format!(
        "Enable '{}' {} ({})? It runs with your rights and can act as you; slots: {}",
        row.name,
        row.version,
        row.capabilities.join(", "),
        if row.slots.is_empty() {
            "none".into()
        } else {
            row.slots.join(", ")
        }
    );
    if !ask(&question)? {
        return Err(CliError::invalid("Not enabled"));
    }
    if !crate::out::dry_run() {
        plugin::set_enabled(&c, name, true).map_err(CliError::from)?;
    }
    out::ok("plugin.enable", json!({ "name": name }), || {
        println!("Enabled {name}")
    });
    Ok(())
}

pub fn disable(access: &Access, name: &str) -> CliResult {
    let c = conn(access)?;
    if !crate::out::dry_run() {
        plugin::set_enabled(&c, name, false).map_err(CliError::from)?;
    }
    out::ok("plugin.disable", json!({ "name": name }), || {
        println!("Disabled {name}")
    });
    Ok(())
}

pub fn rm(access: &Access, name: &str) -> CliResult {
    let c = conn(access)?;
    let removed = !crate::out::dry_run() && plugin::remove(&c, name).map_err(CliError::from)?;
    out::ok(
        "plugin.rm",
        json!({ "name": name, "removed": removed }),
        || {
            println!(
                "{}",
                if removed {
                    "Removed (the files stay on disk)"
                } else {
                    "Nothing removed"
                }
            );
        },
    );
    Ok(())
}

/// Runs the conformance suite on a plugin directory. Needs no vault.
pub fn test(path: &Path) -> CliResult {
    let text = std::fs::read_to_string(path.join("plugin.json"))
        .map_err(|e| CliError::not_found(format!("{}: {e}", path.join("plugin.json").display())))?;
    let manifest = Manifest::parse(&text).map_err(CliError::invalid)?;
    let checks = plugin_host::conformance(path, &manifest);
    let failed = checks.iter().filter(|c| !c.ok).count();
    out::ok(
        "plugin.test",
        json!({ "plugin": manifest.name, "passed": failed == 0, "checks": checks }),
        || {
            for c in &checks {
                if c.ok {
                    println!("ok    {}", c.name);
                } else {
                    println!("FAIL  {}: {}", c.name, c.detail);
                }
            }
        },
    );
    if failed > 0 {
        return Err(CliError::conflict(format!(
            "{failed} conformance check(s) failed"
        )));
    }
    Ok(())
}
