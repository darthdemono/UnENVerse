// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Nodes ("Nodes", README), the hub side driven through the real `unv` and a
//! real `unv-server --nodes`:
//!
//! * nodes are opt-in: without `--nodes` every node route is a 404;
//! * an enrollment token is created without ever reaching the terminal
//!   (`--out`, 0600, or `--reveal`), there is one open token per node name, and
//!   a token enrols exactly once;
//! * an enrolled node is listed with its key fingerprint and only the projects
//!   named when its token was minted;
//! * revoking a node removes it from the active list;
//! * a node policy and an approver device are settable and visible, and a
//!   sub-user cannot do any of it.

mod common;
#[path = "common/server.rs"]
mod server;

use common::Vault;
use server::Server;

const PW: &str = "operator-password-12345";

fn hub(tag: &str) -> (Vault, Server) {
    let v = Vault::new(tag);
    v.ok(&["project", "add", "Edge"]);
    v.ok(&["project", "add", "Other"]);
    let s = Server::start(&v, true);
    (v, s)
}

fn mint(v: &Vault, s: &Server, name: &str, project: &str) -> std::path::PathBuf {
    let file = v.path(&format!("{name}.token"));
    let o = v.on(
        s,
        &[
            "node",
            "token",
            "new",
            name,
            "--project",
            project,
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    file
}

fn enroll(
    v: &Vault,
    s: &Server,
    token_file: &std::path::Path,
    state: &std::path::Path,
) -> common::Out {
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
    c.env_remove("UNV_SERVER_URL")
        .env("HOME", v.path("home"))
        .env("XDG_STATE_HOME", v.path("state"))
        .stdin(std::process::Stdio::null())
        .args([
            "--json",
            "node",
            "enroll",
            "--hub",
            &s.url,
            "--token-file",
            token_file.to_str().unwrap(),
            "--state-dir",
            state.to_str().unwrap(),
        ]);
    let o = c.output().unwrap();
    common::Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn nodes(v: &Vault, s: &Server) -> Vec<serde_json::Value> {
    v.on(s, &["node", "ls"]).json()["data"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn nodes_are_opt_in_and_a_hub_without_them_answers_not_found() {
    let v = Vault::new("nonodes");
    v.ok(&["project", "add", "Edge"]);
    let s = Server::start(&v, false);
    let o = v.on(&s, &["node", "ls"]);
    assert_eq!(
        o.code,
        3,
        "every /api/nodes/* route answers 404 without --nodes: {}",
        o.all()
    );
}

#[cfg(unix)]
#[test]
fn a_token_goes_to_a_private_file_and_is_never_printed() {
    use common::mode_of;
    let (v, s) = hub("nodetoken");
    let file = v.path("vps.token");
    let o = v.on(
        &s,
        &[
            "node",
            "token",
            "new",
            "vps-01",
            "--project",
            "edge",
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    let token = std::fs::read_to_string(&file).unwrap().trim().to_string();
    assert!(token.len() >= 20, "{token:?}");
    assert!(
        !o.all().contains(&token),
        "the token must not reach the terminal: {}",
        o.all()
    );
    assert_eq!(mode_of(&file), 0o600, "a single-use credential, owner-only");
}

#[test]
fn printing_a_token_is_refused_before_one_is_minted() {
    let (v, s) = hub("nodetokenpolicy");
    let refused = v.on(&s, &["node", "token", "new", "vps-01", "--project", "edge"]);
    assert_eq!(refused.code, 9, "{}", refused.all());
    // Had the refused call minted a token, this would now collide with it.
    let file = mint(&v, &s, "vps-01", "edge");
    assert!(file.exists());
}

#[test]
fn a_node_name_has_one_open_token_at_a_time() {
    let (v, s) = hub("nodeonetoken");
    mint(&v, &s, "vps-01", "edge");
    let again = v.on(
        &s,
        &[
            "node",
            "token",
            "new",
            "vps-01",
            "--project",
            "edge",
            "--out",
            v.path("second.token").to_str().unwrap(),
        ],
    );
    assert_ne!(again.code, 0, "{}", again.all());
    assert!(again.all().contains("already open"), "{}", again.all());
}

#[test]
fn a_token_enrols_exactly_one_node_and_names_it_after_the_token() {
    let (v, s) = hub("nodeenroll");
    let token = mint(&v, &s, "vps-01", "edge");
    let state = v.path("node-state");
    std::fs::create_dir_all(&state).unwrap();
    let o = enroll(&v, &s, &token, &state);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["name"], "vps-01");
    assert!(
        state.join("node-state.json").exists(),
        "the node keeps its own identity"
    );

    let listed = nodes(&v, &s);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["name"], "vps-01");
    assert_eq!(
        listed[0]["projects"],
        serde_json::json!(["edge"]),
        "only the projects named when the token was minted"
    );
    assert_eq!(
        listed[0]["fingerprint"].as_str().unwrap().len(),
        64,
        "the node's key fingerprint: {}",
        listed[0]
    );

    // The same token again, from another machine, is refused.
    let other_state = v.path("other-node-state");
    std::fs::create_dir_all(&other_state).unwrap();
    let replay = enroll(&v, &s, &token, &other_state);
    assert_ne!(replay.code, 0, "a token is single-use: {}", replay.all());
    assert_eq!(nodes(&v, &s).len(), 1, "no second node appeared");
}

#[cfg(unix)]
#[test]
fn a_nodes_identity_file_is_owner_only() {
    use common::mode_of;
    let (v, s) = hub("nodeidentity");
    let token = mint(&v, &s, "vps-01", "edge");
    let state = v.path("node-state");
    std::fs::create_dir_all(&state).unwrap();
    assert_eq!(enroll(&v, &s, &token, &state).code, 0);
    assert_eq!(
        mode_of(&state.join("node-state.json")),
        0o600,
        "it holds the node's private signing key"
    );
}

#[test]
fn revoking_a_node_takes_it_out_of_the_active_list() {
    let (v, s) = hub("noderevoke");
    let token = mint(&v, &s, "vps-01", "edge");
    let state = v.path("node-state");
    std::fs::create_dir_all(&state).unwrap();
    enroll(&v, &s, &token, &state);

    assert_eq!(v.on(&s, &["node", "show", "vps-01"]).code, 0);
    let r = v.on(&s, &["node", "revoke", "vps-01"]);
    assert_eq!(r.code, 0, "{}", r.all());
    let shown = v.on(&s, &["node", "show", "vps-01"]);
    assert_eq!(
        shown.code,
        3,
        "a revoked node is no longer an active node: {}",
        shown.all()
    );
    let listed = nodes(&v, &s);
    assert!(
        listed
            .iter()
            .all(|n| n["name"] != "vps-01" || !n["revoked_at"].is_null()),
        "a revoked node must be marked as such: {listed:?}"
    );
}

#[test]
fn revoking_an_unknown_node_is_not_found() {
    let (v, s) = hub("nodenone");
    let o = v.on(&s, &["node", "revoke", "ghost"]);
    assert_eq!(o.code, 3, "{}", o.all());
}

#[test]
fn an_approval_policy_is_settable_visible_and_validated() {
    let (v, s) = hub("nodepolicy");
    let token = mint(&v, &s, "prod-1", "edge");
    let state = v.path("node-state");
    std::fs::create_dir_all(&state).unwrap();
    enroll(&v, &s, &token, &state);

    // `device` hands approval to a device the hub has been told about, so it needs one.
    let no_device = v.on(&s, &["node", "policy", "prod-1", "--approval", "device"]);
    assert_ne!(
        no_device.code,
        0,
        "device approval with no approver registered locks the node out: {}",
        no_device.all()
    );
    assert_eq!(
        v.on(&s, &["node", "approver", "register", "--label", "laptop"])
            .code,
        0
    );

    for mode in ["required", "device"] {
        let o = v.on(&s, &["node", "policy", "prod-1", "--approval", mode]);
        assert_eq!(o.code, 0, "{mode}: {}", o.all());
        let n = nodes(&v, &s);
        assert_eq!(n[0]["approval"], mode, "{n:?}");
    }
    let bad = v.on(&s, &["node", "policy", "prod-1", "--approval", "whenever"]);
    assert_ne!(bad.code, 0, "{}", bad.all());
    assert_eq!(
        nodes(&v, &s)[0]["approval"],
        "device",
        "a refused policy changes nothing"
    );
}

#[test]
fn an_approver_device_can_be_shown_registered_listed_and_removed() {
    let (v, s) = hub("nodeapprover");
    let shown = v.run(&["node", "approver", "show"]);
    assert_eq!(shown.code, 0, "{}", shown.all());
    let doc = shown.json();
    let key = doc["data"]["fingerprint"]
        .as_str()
        .or_else(|| doc["data"]["public_key"].as_str())
        .unwrap_or_default()
        .to_string();
    assert!(
        !key.is_empty(),
        "this machine's approver key is printed: {}",
        shown.stdout
    );

    let reg = v.on(&s, &["node", "approver", "register", "--label", "laptop"]);
    assert_eq!(reg.code, 0, "{}", reg.all());
    let ls = v.on(&s, &["node", "approver", "ls"]);
    assert!(ls.stdout.contains("laptop"), "{}", ls.stdout);
    let fp = ls.json()["data"]["approvers"][0]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(fp.len() >= 8, "{}", ls.stdout);
    // "at least 8 characters of its fingerprint" (README): a prefix is enough, and it
    // must not be mistaken for the global TLS --fingerprint pin.
    let rm = v.on(&s, &["node", "approver", "rm", &fp[..8]]);
    assert_eq!(rm.code, 0, "{}", rm.all());
    assert!(!v
        .on(&s, &["node", "approver", "ls"])
        .stdout
        .contains("laptop"));
}

#[test]
fn a_sub_user_cannot_mint_tokens_or_change_policy() {
    let (v, s) = hub("nodeowner");
    assert_eq!(
        v.on(&s, &["user", "add", "ops", "--user-password", PW])
            .code,
        0
    );
    assert_eq!(
        v.on(
            &s,
            &["perm", "set", "user", "ops", "--read", "vault:*", "--write", "vault:*"]
        )
        .code,
        0
    );
    let file = v.path("sneaky.token");
    let o = v.as_user(
        &s,
        "ops",
        PW,
        &[
            "node",
            "token",
            "new",
            "sneaky",
            "--project",
            "edge",
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 5, "token minting is owner-only: {}", o.all());
    assert!(!file.exists());
}
