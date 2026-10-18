//! A node's life as the hub owner sees it ("Nodes", README), driven through the
//! real `unv`, a real `unv-server --nodes` and a real agent beat (`unv node run
//! --once`):
//!
//! * a push to a node whose policy is `required` is held, writes nothing, and
//!   waits in `node approvals`; approving it makes the next beat write exactly
//!   those bytes; rejecting it leaves the file alone; a decided request cannot
//!   be decided again;
//! * a pull target's file reaches the owner only when asked for, is written
//!   0600 and is never printed;
//! * `node accept` records a pull target's current file as accepted.

mod common;
#[path = "common/server.rs"]
mod server;

use common::Vault;
use server::Server;
use std::path::PathBuf;

struct World {
    v: Vault,
    s: Server,
    state: PathBuf,
}

fn world(tag: &str) -> World {
    let v = Vault::new(tag);
    v.ok(&["project", "add", "Edge"]);
    v.ok(&[
        "project", "chunk", "add", "Edge", "app", "--type", "env_file",
    ]);
    v.ok(&[
        "project",
        "chunk",
        "set",
        "Edge",
        "app",
        "TOKEN=first-value-12345",
    ]);
    let s = Server::start(&v, true);
    let state = v.path("node-state");
    std::fs::create_dir_all(&state).unwrap();
    let token = v.path("t.token");
    let o = v.on(
        &s,
        &[
            "node",
            "token",
            "new",
            "prod-1",
            "--project",
            "Edge",
            "--out",
            token.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_unv"))
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
            token.to_str().unwrap(),
        ])
        .args(["--state-dir", state.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    World { v, s, state }
}

fn write_config(w: &World, body: &str) -> PathBuf {
    let p = w.v.path("node.toml");
    std::fs::write(&p, body).unwrap();
    p
}

/// One agent beat; returns the parsed `data` of `node run --once`.
fn beat(w: &World, cfg: &std::path::Path) -> serde_json::Value {
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_unv"))
        .env_remove("UNV_SERVER_URL")
        .env("HOME", w.v.path("home"))
        .env("XDG_STATE_HOME", w.v.path("state"))
        .stdin(std::process::Stdio::null())
        .args([
            "--json",
            "node",
            "run",
            "--once",
            "--config",
            cfg.to_str().unwrap(),
        ])
        .args(["--state-dir", w.state.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice::<serde_json::Value>(&o.stdout).unwrap()["data"].clone()
}

fn push_config(w: &World, out: &std::path::Path) -> PathBuf {
    write_config(
        w,
        &format!(
            "[[target]]\nid=\"env-main\"\npath='{}'\nproject=\"Edge\"\nexporter=\"env\"\napply=true\nrequire_approval=true\n",
            out.display()
        ),
    )
}

fn policy_required(w: &World) {
    let o = w.v.on(
        &w.s,
        &["node", "policy", "prod-1", "--approval", "required"],
    );
    assert_eq!(o.code, 0, "{}", o.all());
}

fn pending(w: &World) -> Vec<serde_json::Value> {
    w.v.on(&w.s, &["node", "approvals"]).json()["data"]["approvals"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn a_held_push_writes_nothing_until_a_human_approves_exactly_those_bytes() {
    let w = world("hubapprove");
    policy_required(&w);
    let out = w.v.path("app.env");
    let cfg = push_config(&w, &out);

    let held = beat(&w, &cfg);
    assert_eq!(held["held_for_approval"], 1, "{held}");
    assert!(!out.exists(), "nothing is written before anyone says yes");
    let rows = pending(&w);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["node"], "prod-1");
    assert_eq!(rows[0]["target"], "env-main");
    assert_eq!(rows[0]["status"], "pending");

    let id = rows[0]["id"].as_str().unwrap().to_string();
    let ok = w.v.on(&w.s, &["node", "approve", &id[..8]]);
    assert_eq!(ok.code, 0, "an id prefix of 8 is enough: {}", ok.all());
    assert!(
        pending(&w).is_empty(),
        "a decided request leaves the waiting list"
    );

    let again = w.v.on(&w.s, &["node", "approve", &id]);
    assert_eq!(
        again.code,
        6,
        "a decided request is a conflict: {}",
        again.all()
    );
    let reject_after = w.v.on(&w.s, &["node", "reject", &id]);
    assert_eq!(reject_after.code, 6, "{}", reject_after.all());

    let wrote = beat(&w, &cfg);
    assert_eq!(wrote["actions"], 1, "{wrote}");
    assert!(std::fs::read_to_string(&out)
        .unwrap()
        .contains("TOKEN=first-value-12345"));
}

#[test]
fn a_rejected_push_never_reaches_the_file() {
    let w = world("hubreject");
    policy_required(&w);
    let out = w.v.path("app.env");
    let cfg = push_config(&w, &out);
    beat(&w, &cfg);
    let id = pending(&w)[0]["id"].as_str().unwrap().to_string();

    let r = w.v.on(&w.s, &["node", "reject", &id]);
    assert_eq!(r.code, 0, "{}", r.all());
    let all = w.v.on(&w.s, &["node", "approvals", "--all"]).json()["data"]["approvals"].clone();
    assert_eq!(
        all[0]["status"], "rejected",
        "--all shows the decided ones: {all}"
    );
    beat(&w, &cfg);
    assert!(
        !out.exists(),
        "the same bytes are not asked about again, and are never written"
    );
    assert!(
        pending(&w).is_empty(),
        "a rejected push is not asked about again: {:?}",
        pending(&w)
    );
}

#[test]
fn an_unknown_request_is_not_found_and_a_short_prefix_is_not_a_match() {
    let w = world("hubunknown");
    let o = w.v.on(&w.s, &["node", "approve", "deadbeefdeadbeef"]);
    assert_eq!(o.code, 3, "{}", o.all());
    let short = w.v.on(&w.s, &["node", "reject", "ab"]);
    assert_eq!(short.code, 3, "{}", short.all());
}

#[test]
fn a_pull_target_reaches_the_owner_only_when_asked_and_is_written_owner_only() {
    let w = world("hubpull");
    let file = w.v.path("wg0.conf");
    std::fs::write(&file, "[Interface]\nPrivateKey = pull-secret-abcdef\n").unwrap();
    let cfg = write_config(
        &w,
        &format!("[[target]]\nid=\"wg\"\npath='{}'\nproject=\"Edge\"\nexporter=\"wireguard\"\nmode=\"pull\"\n", file.display()),
    );
    beat(&w, &cfg);

    let dest = w.v.path("pulled.conf");
    let outcome = std::thread::scope(|sc| {
        let pull = sc.spawn(|| {
            w.v.on(
                &w.s,
                &[
                    "node",
                    "pull",
                    "prod-1",
                    "wg",
                    "--out",
                    dest.to_str().unwrap(),
                ],
            )
        });
        // The node picks the request up on its next beat.
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(400));
            beat(&w, &cfg);
            if pull.is_finished() {
                break;
            }
        }
        pull.join().unwrap()
    });
    assert_eq!(outcome.code, 0, "{}", outcome.all());
    assert!(
        !outcome.all().contains("pull-secret-abcdef"),
        "the file is never printed: {}",
        outcome.all()
    );
    assert_eq!(
        std::fs::read_to_string(&dest).unwrap(),
        std::fs::read_to_string(&file).unwrap()
    );
    #[cfg(unix)]
    assert_eq!(common::mode_of(&dest), 0o600);

    let accepted = w.v.on(&w.s, &["node", "accept", "prod-1", "wg"]);
    assert_eq!(accepted.code, 0, "{}", accepted.all());
    assert_eq!(
        accepted.json()["data"]["sha256"].as_str().unwrap().len(),
        64
    );
}

#[test]
fn pull_of_an_unknown_node_is_not_found_and_writes_nothing() {
    let w = world("hubpullbad");
    let dest = w.v.path("x.conf");
    let o = w.v.on(
        &w.s,
        &[
            "node",
            "pull",
            "nope",
            "wg",
            "--out",
            dest.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 3, "{}", o.all());
    assert!(!dest.exists());
}
