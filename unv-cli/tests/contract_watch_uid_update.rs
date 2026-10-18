//! `unv watch`, `unv uid prune`, `unv catalogue update` and `unv update install`:
//!
//! * watch syncs a `.env` change into the vault while it runs, and a missing file
//!   is not_found;
//! * prune deletes registry rows before a date (a dry run deletes nothing) and
//!   after it an ID is no longer known;
//! * an unverifiable catalogue or release is refused and changes nothing.

mod common;
#[path = "common/server.rs"]
mod server;

use common::Vault;
use server::Server;
use std::io::Write;

#[test]
fn watch_of_a_missing_file_is_not_found() {
    let v = Vault::new("watchmissing");
    let o = v.run(&["watch", v.path("nope.env").to_str().unwrap()]);
    assert_eq!(o.code, 3, "{}", o.all());
}

#[test]
fn watch_syncs_a_change_into_the_vault_while_it_runs() {
    let v = Vault::new("watchsync");
    let file = v.path("app.env");
    std::fs::write(&file, "SEEDED=1\n").unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_unv"))
        .env_remove("UNV_SERVER_URL")
        .env("UNV_PASSWORD", common::PASSWORD)
        .env("HOME", v.path("home"))
        .env("XDG_DATA_HOME", v.path("data"))
        .env("XDG_CONFIG_HOME", v.path("config"))
        .env("XDG_STATE_HOME", v.path("state"))
        .args([
            "--db-path",
            v.db().to_str().unwrap(),
            "--init",
            "--yes",
            "watch",
            file.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut seen = false;
    for i in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        writeln!(f, "WATCHED_VAR=watched-value-{i}-abcdef").unwrap();
        drop(f);
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let names: Vec<String> = v.run(&["list"]).json()["data"]["entries"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| e["provider"].as_str().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();
        if names.iter().any(|n| n.contains("WATCHED")) {
            seen = true;
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(seen, "an edit to the watched file must reach the vault");
}

#[test]
fn prune_deletes_old_registry_rows_and_a_dry_run_deletes_nothing() {
    let v = Vault::new("uidprune");
    v.ok(&["project", "add", "Web"]);
    let s = Server::start_with(&v, false, true);
    assert_eq!(
        v.on(&s, &["uid", "register", "order-1", "--namespace", "orders"])
            .code,
        0
    );

    let dry = v.run_with(
        &["--server", &s.url, "--dry-run"],
        &["uid", "prune", "2999-01-01"],
    );
    assert_eq!(dry.code, 0, "{}", dry.all());
    assert_eq!(
        v.on(&s, &["uid", "lookup", "order-1"]).json()["data"]["issued"],
        true,
        "a dry run deletes nothing"
    );

    assert_eq!(v.on(&s, &["uid", "prune", "2000-01-01"]).code, 0);
    assert_eq!(
        v.on(&s, &["uid", "lookup", "order-1"]).json()["data"]["issued"],
        true,
        "rows after the date stay"
    );

    let pruned = v.on(&s, &["uid", "prune", "2999-01-01"]);
    assert_eq!(pruned.code, 0, "{}", pruned.all());
    assert_eq!(
        v.on(&s, &["uid", "lookup", "order-1"]).json()["data"]["issued"],
        false,
        "pruned means forgotten"
    );
    assert_eq!(v.on(&s, &["uid", "prune", "not-a-date"]).code, 10);
}

#[test]
fn a_catalogue_that_cannot_be_verified_is_refused_and_not_cached() {
    let v = Vault::new("catupdate");
    let junk = v.path("junk.json");
    std::fs::write(&junk, "{\"providers\":[]}").unwrap();
    let o = v.run(&["catalogue", "update", "--file", junk.to_str().unwrap()]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert_eq!(
        v.ok(&["catalogue", "show"])["source"],
        "bundled",
        "nothing unverified is ever used"
    );
}

#[test]
fn an_unsigned_build_refuses_to_install_a_release() {
    let v = Vault::new("updinstall");
    let m = v.path("manifest.json");
    std::fs::write(&m, "{}").unwrap();
    let a = v.path("unv.tar.gz");
    std::fs::write(&a, "bytes").unwrap();
    let o = v.run(&[
        "update",
        "install",
        "--manifest",
        m.to_str().unwrap(),
        "--archive",
        a.to_str().unwrap(),
    ]);
    assert_eq!(
        o.code,
        5,
        "no pinned key means nothing can be verified: {}",
        o.all()
    );
}
