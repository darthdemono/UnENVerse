// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The scripting contract (`book/src/contract.md`) pinned from outside: every command and flag
//! `unv describe` lists is compared with a committed snapshot, so a rename or removal that would
//! break a script fails here until it is made on purpose. Adding a command or flag also changes the
//! snapshot, which is the prompt to say so in the changelog.
//!
//! Regenerate with `UNV_UPDATE_CONTRACT=1 cargo test -p unv-cli --test contract_surface`, then read
//! the diff.

mod common;
use common::Vault;
use serde_json::Value;
use std::path::PathBuf;

fn surface(node: &Value, out: &mut Vec<String>) {
    let subs = node["subcommands"].as_array().cloned().unwrap_or_default();
    if subs.is_empty() {
        let mut positional = Vec::new();
        let mut flags = Vec::new();
        for a in node["args"].as_array().cloned().unwrap_or_default() {
            let id = a["id"].as_str().unwrap_or("").to_string();
            let required = a["required"].as_bool().unwrap_or(false);
            if a["kind"] == "positional" {
                positional.push(if required {
                    format!("<{id}>")
                } else {
                    format!("[{id}]")
                });
            } else if let Some(long) = a["long"].as_str() {
                let value = if a["takes_value"].as_bool().unwrap_or(false) {
                    "="
                } else {
                    ""
                };
                flags.push(format!("{long}{value}"));
            }
        }
        flags.sort();
        out.push(format!(
            "{} {} {}",
            node["path"].as_str().unwrap_or(""),
            positional.join(" "),
            flags.join(" ")
        ));
    }
    for s in &subs {
        surface(s, out);
    }
}

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/contract/surface.txt")
}

#[test]
fn the_described_surface_matches_the_committed_snapshot() {
    let v = Vault::new("surface");
    let doc = v.run(&["describe"]).json();
    assert_eq!(
        doc["contract_version"], doc["contract"],
        "the old key stays as an alias"
    );
    assert!(doc["contract_version"].as_u64().unwrap() >= 1);
    let mut lines = Vec::new();
    surface(&doc["command"], &mut lines);
    lines.sort();
    let mut text = lines
        .iter()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    text.push('\n');
    // Exit codes are part of the contract too.
    let mut codes: Vec<String> = doc["exit_codes"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            format!(
                "exit {k} {}",
                v.as_str().unwrap().split(" — ").next().unwrap()
            )
        })
        .collect();
    codes.sort();
    text.push_str(&codes.join("\n"));
    text.push('\n');
    if std::env::var_os("UNV_UPDATE_CONTRACT").is_some() {
        std::fs::write(snapshot_path(), &text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(snapshot_path()).unwrap_or_default();
    if expected != text {
        let old: std::collections::BTreeSet<_> = expected.lines().collect();
        let new: std::collections::BTreeSet<_> = text.lines().collect();
        let removed: Vec<_> = old.difference(&new).collect();
        let added: Vec<_> = new.difference(&old).collect();
        panic!(
            "the described surface changed.\nremoved or altered (can break scripts): {removed:#?}\nadded: {added:#?}\nIf this is on purpose, regenerate (see the file header), add a Contract section to CHANGELOG.md, and raise CONTRACT_VERSION for a removal."
        );
    }
}

/// The command reference page, written from the same description. A command added, renamed or
/// re-described in code changes this page, and the test fails until the page is regenerated.
fn reference_rows(node: &Value, rows: &mut Vec<(String, String)>) {
    let subs = node["subcommands"].as_array().cloned().unwrap_or_default();
    if subs.is_empty() {
        let mut parts = vec![node["path"].as_str().unwrap_or("").to_string()];
        for a in node["args"].as_array().cloned().unwrap_or_default() {
            let id = a["id"].as_str().unwrap_or("");
            if a["kind"] == "positional" {
                parts.push(if a["required"].as_bool().unwrap_or(false) {
                    format!("<{id}>")
                } else {
                    format!("[{id}]")
                });
            }
        }
        let about = node["about"]
            .as_str()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .replace('|', "\\|");
        rows.push((parts.join(" "), about));
    }
    for s in &subs {
        reference_rows(s, rows);
    }
}

#[test]
fn the_command_reference_page_matches_the_described_commands() {
    let v = Vault::new("reference");
    let doc = v.run(&["describe"]).json();
    let mut rows = Vec::new();
    reference_rows(&doc["command"], &mut rows);
    rows.sort();
    let mut text = String::from(
        "# Command reference\n\n\
         Written from `unv describe` by `unv-cli/tests/contract_surface.rs`; do not edit it by hand. \
         A command added or renamed in code changes this page, and the test fails until it is regenerated \
         (`UNV_UPDATE_CONTRACT=1 cargo test -p unv-cli --test contract_surface`). Run `unv <command> --help` \
         for the flags of one command.\n\n",
    );
    text.push_str(&format!(
        "Contract version {}.\n\n| Command | What it does |\n| --- | --- |\n",
        doc["contract_version"]
    ));
    for (cmd, about) in &rows {
        text.push_str(&format!("| `{cmd}` | {about} |\n"));
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../book/src/cli-reference.md");
    if std::env::var_os("UNV_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap_or_default(),
        text,
        "book/src/cli-reference.md is stale: regenerate it (see the page header)"
    );
}
