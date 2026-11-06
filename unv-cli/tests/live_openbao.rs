// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv import-service openbao` against a real OpenBao dev server (R11). The mock-server tests in
//! `contract_service_sources.rs` are built from the documented responses; this one is the check
//! that the documented responses are the real ones.
//!
//! It runs only when `UNV_LIVE_OPENBAO_ADDR` and `UNV_LIVE_OPENBAO_TOKEN` are set, which the
//! `Live services` workflow does after seeding a secret called `live-db` with two versions. A
//! plain `cargo test` skips it, so no developer needs a container to run the suite.

mod common;
use common::Vault;

fn live() -> Option<(String, String)> {
    let addr = std::env::var("UNV_LIVE_OPENBAO_ADDR").ok()?;
    let token = std::env::var("UNV_LIVE_OPENBAO_TOKEN").ok()?;
    Some((addr, token))
}

#[test]
fn a_real_openbao_imports_the_latest_value_and_keeps_the_first_as_history() {
    let Some((addr, token)) = live() else {
        eprintln!("skipped: UNV_LIVE_OPENBAO_ADDR / UNV_LIVE_OPENBAO_TOKEN not set");
        return;
    };
    let v = Vault::new("live-bao");
    let env = [("BAO_TOKEN", token.as_str())];
    let args = [
        "import-service",
        "openbao",
        "--addr",
        addr.as_str(),
        "--apply",
    ];
    let o = v.run_env(&env, &args);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        !o.all().contains(token.as_str()),
        "the token must not be echoed"
    );
    assert_eq!(
        v.reveal_field("live-db", "password"),
        "live-second-password-0002"
    );
    let shown = v.ok(&["get", "live-db"])["entries"][0].clone();
    assert_eq!(
        shown["version_history"].as_array().map(Vec::len),
        Some(1),
        "the first version is kept as history: {shown}"
    );
    // A second run finds nothing to change.
    let again = v.run_env(&env, &args);
    assert_eq!(again.code, 0, "{}", again.all());
    assert_eq!(
        again.json()["data"]["counts"]["imported"],
        0,
        "{}",
        again.stdout
    );
}
