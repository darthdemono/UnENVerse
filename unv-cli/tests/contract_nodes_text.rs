// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The human (non-JSON) side of node administration on a hub, as an operator at
//! a terminal reads it:
//!
//! * the node commands refuse a local vault and say a hub is needed;
//! * an empty list says so; an enrolled node is listed with its state and shown
//!   with its key fingerprint and projects;
//! * revoking asks first (a script without `--yes` is refused), `--dry-run`
//!   changes nothing, and a real revoke says so and takes the node off the list;
//! * a policy change and an approver device are confirmed in words that say what
//!   the policy now means;
//! * an enrollment token is never printed unless asked for, and a bad lifetime is
//!   refused as invalid input before anything is minted.

mod common;
#[path = "common/server.rs"]
mod server;

use common::Vault;
use server::Server;

fn hub(tag: &str) -> (Vault, Server) {
    let v = Vault::new(tag);
    v.ok(&["project", "add", "Edge"]);
    let s = Server::start(&v, true);
    (v, s)
}

fn enroll_node(v: &Vault, s: &Server, name: &str) {
    let file = v.path(&format!("{name}.token"));
    let o = v.on(
        s,
        &[
            "node",
            "token",
            "new",
            name,
            "--project",
            "edge",
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    let state = v.path(&format!("{name}-state"));
    std::fs::create_dir_all(&state).unwrap();
    let e = std::process::Command::new(env!("CARGO_BIN_EXE_unv"))
        .env_remove("UNV_SERVER_URL")
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
            file.to_str().unwrap(),
            "--state-dir",
            state.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(e.status.success(), "{}", String::from_utf8_lossy(&e.stdout));
}

#[test]
fn node_commands_against_a_local_vault_name_the_missing_hub() {
    let v = Vault::new("ntext-local");
    for args in [vec!["node", "ls"], vec!["node", "show", "x"]] {
        let o = v.run_text(&args);
        assert_eq!(o.code, 7, "{args:?}: {}", o.all());
        assert!(
            o.all().contains("Nodes are managed on a hub"),
            "{}",
            o.all()
        );
    }
}

#[test]
fn an_enrolled_node_is_listed_and_shown_and_a_revoke_is_confirmed_in_words() {
    let (v, s) = hub("ntext-life");
    assert!(v
        .on_text(&s, &["node", "ls"])
        .stdout
        .contains("No nodes enrolled."));
    enroll_node(&v, &s, "prod-1");

    let list = v.on_text(&s, &["node", "ls"]);
    assert_eq!(list.code, 0, "{}", list.all());
    let row = list.stdout.lines().find(|l| l.contains("prod-1")).unwrap();
    assert!(row.contains("active") && row.contains("never"), "{row}");

    let shown = v.on_text(&s, &["node", "show", "prod-1"]);
    assert_eq!(shown.code, 0, "{}", shown.all());
    assert!(shown.stdout.contains("key fingerprint"), "{}", shown.stdout);
    assert!(shown.stdout.contains("projects"), "{}", shown.stdout);
    assert_eq!(v.on_text(&s, &["node", "show", "nope"]).code, 3);

    // A script that forgot --yes is refused, and --dry-run only says what it would do.
    let no_yes = v.run_no_yes(&["--server", &s.url, "node", "revoke", "prod-1"]);
    assert_ne!(no_yes.code, 0, "{}", no_yes.all());
    let dry = v.on_text(&s, &["--dry-run", "node", "revoke", "prod-1"]);
    assert!(
        dry.stdout.contains("Would revoke 'prod-1'."),
        "{}",
        dry.all()
    );
    assert!(v.on_text(&s, &["node", "ls"]).stdout.contains("prod-1"));

    let done = v.on_text(&s, &["node", "revoke", "prod-1"]);
    assert!(done.stdout.contains("Revoked 'prod-1'."), "{}", done.all());
    assert_eq!(
        v.on_text(&s, &["node", "revoke", "prod-1"]).code,
        3,
        "a revoked node is no longer an active node"
    );
}

#[test]
fn a_policy_change_says_what_the_policy_now_means() {
    let (v, s) = hub("ntext-policy");
    enroll_node(&v, &s, "prod-1");
    let none = v.on_text(&s, &["node", "policy", "prod-1", "--approval", "none"]);
    assert!(
        none.stdout.contains("pushes go out as soon as"),
        "{}",
        none.all()
    );
    let required = v.on_text(&s, &["node", "policy", "prod-1", "--approval", "required"]);
    assert!(
        required.stdout.contains("approve those exact bytes"),
        "{}",
        required.all()
    );
    v.on_text(&s, &["node", "approver", "register", "--label", "laptop"]);
    let device = v.on_text(&s, &["node", "policy", "prod-1", "--approval", "device"]);
    assert!(
        device
            .stdout
            .contains("sign an approval on your own device"),
        "{}",
        device.all()
    );
}

#[test]
fn approver_devices_are_shown_registered_listed_and_removed_in_words() {
    let (v, s) = hub("ntext-approver");
    let shown = v.run_text(&["node", "approver", "show"]);
    assert_eq!(shown.code, 0, "{}", shown.all());
    assert!(shown.stdout.contains("Approver public key:"));
    assert!(shown.stdout.contains("Fingerprint:"));

    assert!(v
        .on_text(&s, &["node", "approver", "ls"])
        .stdout
        .contains("No approver devices are registered."));
    let reg = v.on_text(&s, &["node", "approver", "register", "--label", "laptop"]);
    assert!(reg.stdout.contains("Registered 'laptop'"), "{}", reg.all());
    let ls = v.on_text(&s, &["node", "approver", "ls"]);
    // What the listing prints must be usable as it stands in `rm` (a prefix is enough):
    // it once ended in an ellipsis that `rm` answered with "No such approver".
    let fp = ls.stdout.split_whitespace().next().unwrap().to_string();
    assert!(fp.len() >= 8 && !fp.contains('…'), "{fp}");
    assert!(ls.stdout.contains("laptop"), "{}", ls.stdout);
    let no_yes = v.run_no_yes(&["--server", &s.url, "node", "approver", "rm", &fp]);
    assert_ne!(no_yes.code, 0, "removal without --yes is refused");
    let rm = v.on_text(&s, &["node", "approver", "rm", &fp]);
    assert!(rm.stdout.contains("Removed."), "{}", rm.all());
}

#[test]
fn an_enrollment_token_is_not_printed_unless_asked_and_a_bad_lifetime_mints_nothing() {
    let (v, s) = hub("ntext-token");
    let refused = v.on_text(&s, &["node", "token", "new", "edge-1", "--project", "edge"]);
    assert_ne!(refused.code, 0);
    assert!(refused.all().contains("--out FILE"), "{}", refused.all());

    for bad in ["soon", "5x", ""] {
        let o = v.on_text(
            &s,
            &[
                "--reveal",
                "node",
                "token",
                "new",
                "edge-1",
                "--project",
                "edge",
                "--ttl",
                bad,
            ],
        );
        assert_eq!(o.code, 10, "ttl {bad:?}: {}", o.all());
    }

    for ttl in ["90s", "15m", "2h", "1d"] {
        let name = format!("n-{ttl}");
        let o = v.on_text(
            &s,
            &[
                "--reveal",
                "node",
                "token",
                "new",
                &name,
                "--project",
                "edge",
                "--ttl",
                ttl,
            ],
        );
        assert_eq!(o.code, 0, "ttl {ttl}: {}", o.all());
        assert!(!o.stdout.trim().is_empty());
    }

    let file = v.path("out.token");
    let o = v.on_text(
        &s,
        &[
            "node",
            "token",
            "new",
            "to-file",
            "--project",
            "edge",
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert!(o.stdout.contains("written to"), "{}", o.all());
    assert!(!o
        .stdout
        .contains(std::fs::read_to_string(&file).unwrap().trim()));
}
