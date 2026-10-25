// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Starter chunks of a new typed project. The desktop app builds them in
//! `src/ts/chunks/starters.ts`, the CLI in `unv-app/src/starters.rs`; the same
//! project type must start with the same chunks whichever one created it, so both
//! sides are pinned by one golden fixture, `tests/fixtures/parity/starters.json`
//! (asserted from TypeScript in `tests/starters-parity.test.ts`).
//!
//! The CLI only ports the stable types (README, `project add --help`); an
//! experimental type must be asked for with `--experimental` and then starts empty
//! here rather than with a second copy of the templates.

mod common;

use common::Vault;
use serde_json::Value;

fn golden() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/parity/starters.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A chunk as the golden file records it: no id, since ids are random.
fn without_id(chunk: &Value) -> Value {
    let mut c = chunk.clone();
    c.as_object_mut().unwrap().remove("id");
    c
}

#[test]
fn a_stable_typed_project_starts_with_the_chunks_the_app_gives_it() {
    let want = golden();
    for ptype in ["wireguard", "docker", "nginx"] {
        let v = Vault::new(&format!("starter-{ptype}"));
        v.ok(&["project", "add", "P", "--type", ptype]);
        // Revealed, because redaction would print the empty secret as a fingerprint.
        let o = v.run_with(&["--reveal"], &["project", "show", "P"]);
        assert_eq!(o.code, 0, "{}", o.all());
        let shown = o.json()["data"].clone();
        let got: Vec<Value> = shown["chunks"]
            .as_array()
            .unwrap_or_else(|| panic!("{ptype}: no chunks in {shown}"))
            .iter()
            .map(without_id)
            .collect();
        assert_eq!(
            Value::Array(got),
            want[ptype],
            "{ptype}: the CLI and the app start this project type differently"
        );
        let ids: Vec<&str> = shown["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "{ptype}: chunk ids must be unique");
    }
}

#[test]
fn a_generic_project_starts_empty() {
    let v = Vault::new("starter-generic");
    v.ok(&["project", "add", "P"]);
    let shown = v.ok(&["project", "show", "P"]);
    assert!(
        shown["chunks"].as_array().is_none_or(|c| c.is_empty()),
        "{shown}"
    );
}

#[test]
fn an_experimental_type_needs_the_flag_and_then_starts_empty_here() {
    let v = Vault::new("starter-experimental");
    let refused = v.run(&["project", "add", "K", "--type", "kubernetes"]);
    assert_ne!(refused.code, 0, "{}", refused.all());
    v.ok(&[
        "project",
        "add",
        "K",
        "--type",
        "kubernetes",
        "--experimental",
    ]);
    let shown = v.ok(&["project", "show", "K"]);
    assert_eq!(shown["project_type"], "kubernetes", "{shown}");
    assert!(
        shown["chunks"].as_array().is_none_or(|c| c.is_empty()),
        "the CLI does not carry a second copy of experimental templates: {shown}"
    );
}
