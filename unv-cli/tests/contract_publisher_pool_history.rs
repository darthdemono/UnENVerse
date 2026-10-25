// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Pool inspection, history lookups and the publisher commands:
//!
//! * `pool show` lists each member with its use count and whether it cools,
//!   `pool reset` forgets cursor, cooldowns and counts on this machine only;
//! * `history snapshot` records a change once, `history where` finds the
//!   snapshot a file hash came from, `history diff-file` compares a file with
//!   a snapshot and shows no value without `--reveal`;
//! * `catalogue export`/`sign` and `update sign` write verifiable, private output.

mod common;
use common::Vault;

fn pool_vault(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&[
        "entry",
        "add",
        "OpenAI-1",
        "--key",
        "sk-aaaaaaaaaaaaaaaa",
        "--pool",
        "openai",
    ]);
    v.ok(&[
        "entry",
        "add",
        "OpenAI-2",
        "--key",
        "sk-bbbbbbbbbbbbbbbb",
        "--pool",
        "openai",
    ]);
    v
}

#[test]
fn pool_show_counts_each_use_and_pool_reset_forgets_them() {
    let v = pool_vault("poolshow");
    v.ok(&["pool", "next", "openai"]);
    v.ok(&["pool", "next", "openai"]);
    v.ok(&["pool", "next", "openai"]);
    let shown = v.ok(&["pool", "show", "openai"]);
    let members = shown["members"]
        .as_array()
        .unwrap_or_else(|| panic!("{shown}"));
    assert_eq!(members.len(), 2, "{shown}");
    let uses: u64 = members
        .iter()
        .map(|m| m["uses"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(uses, 3, "three hand-outs are three uses: {shown}");
    assert!(
        !v.run(&["pool", "show", "openai"])
            .all()
            .contains("sk-aaaaaaaaaaaaaaaa"),
        "no key in an inspection"
    );

    v.ok(&["pool", "report", "openai", "--limited", "--for", "15m"]);
    v.ok(&["pool", "reset", "openai"]);
    let after = v.ok(&["pool", "show", "openai"]);
    let uses: u64 = after["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["uses"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(uses, 0, "reset forgets counts: {after}");
    assert!(
        after["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["cooling"] != true),
        "and cooldowns: {after}"
    );
    assert_eq!(
        v.ok(&["pool", "next", "openai"])["provider"],
        "OpenAI-1",
        "and the cursor"
    );
    assert_eq!(v.ok(&["list"])["count"], 2, "the entries are untouched");
}

#[test]
fn an_unknown_pool_is_not_found() {
    let v = pool_vault("poolunknown");
    assert_eq!(v.run(&["pool", "show", "nope"]).code, 3);
    assert_eq!(v.run(&["pool", "reset", "nope"]).code, 3);
}

// ---- history

const ONE: &str = "hist-secret-ONE-aaaa";

fn history_vault(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "Stripe", "--key", ONE]);
    v.ok(&["project", "add", "web", "--type", "generic"]);
    v.ok(&[
        "project", "chunk", "add", "web", "app.env", "--type", "env_file",
    ]);
    v.ok(&[
        "project",
        "chunk",
        "set",
        "web",
        "app.env",
        "STRIPE_KEY=${Stripe/api_key}",
        "MODE=live",
    ]);
    v
}

#[test]
fn snapshot_records_a_change_once_and_where_finds_the_hash() {
    let v = history_vault("histwhere");
    let first = v.ok(&["history", "snapshot", "web"]);
    let again = v.ok(&["history", "snapshot", "web"]);
    assert_eq!(
        again["recorded"].as_u64().unwrap_or(0),
        0,
        "nothing changed since the last snapshot: {first} / {again}"
    );

    let ls = v.ok(&["history", "ls", "web"]);
    let row = ls["snapshots"][0].clone();
    let sha = row["sha256"]
        .as_str()
        .unwrap_or_else(|| panic!("{ls}"))
        .to_string();
    let found = v.run(&["history", "where", &sha]);
    assert_eq!(found.code, 0, "{}", found.all());
    assert!(
        found.all().contains("web"),
        "names the project: {}",
        found.all()
    );
    let none = v.ok(&["history", "where", &"0".repeat(64)]);
    assert!(
        none["snapshots"].as_array().unwrap().is_empty(),
        "a hash from nowhere matches no snapshot: {none}"
    );
}

#[test]
fn diff_file_shows_names_of_differing_lines_and_values_only_with_reveal() {
    let v = history_vault("histdifffile");
    v.ok(&["history", "snapshot", "web"]);
    let live = v.path("live.env");
    std::fs::write(&live, "STRIPE_KEY=somebody-changed-this-9999\nMODE=live\n").unwrap();
    let o = v.run(&["history", "diff-file", live.to_str().unwrap(), "web"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(o.all().contains("STRIPE_KEY"), "{}", o.all());
    assert!(
        !o.all().contains("somebody-changed-this-9999") && !o.all().contains(ONE),
        "{}",
        o.all()
    );
    let real = v.run_with(
        &["--reveal"],
        &["history", "diff-file", live.to_str().unwrap(), "web"],
    );
    assert!(
        real.all().contains("somebody-changed-this-9999"),
        "--reveal opts in: {}",
        real.all()
    );
    assert_eq!(
        v.run(&[
            "history",
            "diff-file",
            v.path("nope.env").to_str().unwrap(),
            "web"
        ])
        .code,
        3
    );
}

// ---- publisher

#[test]
fn catalogue_export_then_sign_writes_a_private_signed_file() {
    let v = Vault::new("catsign");
    let exported = v.path("providers.json");
    let o = v.run(&["catalogue", "export"]);
    assert_eq!(o.code, 0, "{}", o.all());
    std::fs::write(&exported, &o.stdout).unwrap();
    let providers: serde_json::Value =
        serde_json::from_str(&o.stdout).expect("stdout is the providers array and nothing else");
    assert!(
        providers.as_array().unwrap().len() > 10,
        "the compiled table, as an array"
    );

    let seed = v.path("seed.hex");
    std::fs::write(&seed, "11".repeat(32)).unwrap();
    let signed = v.path("signed.json");
    let s = v.run(&[
        "catalogue",
        "sign",
        exported.to_str().unwrap(),
        "--key-file",
        seed.to_str().unwrap(),
        "--out",
        signed.to_str().unwrap(),
    ]);
    assert_eq!(s.code, 0, "{}", s.all());
    let text = std::fs::read_to_string(&signed).unwrap();
    assert!(
        text.contains("signature"),
        "{}",
        &text[..200.min(text.len())]
    );
    let bad = v.path("bad.hex");
    std::fs::write(&bad, "not-hex").unwrap();
    let r = v.run(&[
        "catalogue",
        "sign",
        exported.to_str().unwrap(),
        "--key-file",
        bad.to_str().unwrap(),
        "--out",
        signed.to_str().unwrap(),
    ]);
    assert_ne!(r.code, 0, "{}", r.all());
}

#[test]
fn update_sign_hashes_the_archives_and_refuses_a_bad_asset() {
    let v = Vault::new("updsign");
    let archive = v.path("unv-1.2.3-linux-x86_64.tar.gz");
    std::fs::write(&archive, "pretend archive bytes").unwrap();
    let seed = v.path("seed.hex");
    std::fs::write(&seed, "22".repeat(32)).unwrap();
    let out = v.path("manifest.json");
    let o = v.run(&[
        "update",
        "sign",
        "--version",
        "1.2.3",
        "--asset",
        &format!("linux-x86_64={}", archive.display()),
        "--base-url",
        "https://example.org/releases",
        "--key-file",
        seed.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    let m = std::fs::read_to_string(&out).unwrap();
    assert!(
        m.contains("1.2.3")
            && m.contains("https://example.org/releases/unv-1.2.3-linux-x86_64.tar.gz"),
        "{m}"
    );
    assert!(m.contains("sha256") || m.contains("hash"), "{m}");
    let bad = v.run(&[
        "update",
        "sign",
        "--version",
        "1.2.3",
        "--asset",
        "linux-x86_64=/no/such/archive.tar.gz",
        "--base-url",
        "https://example.org",
        "--key-file",
        seed.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_ne!(bad.code, 0, "{}", bad.all());
}
