// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The managed host's side of nodes ("Setting up a node", README): `unv node
//! install --plan` shows every step without running one, a target that only
//! observes gets read access and no reload unit, and `unv node check` reports
//! what the agent would watch.

// `unv node install` sets up a systemd service and refuses on other platforms.
#![cfg(target_os = "linux")]

mod common;
use common::Vault;

fn plan(v: &Vault, target: &str, extra: &[&str]) -> common::Out {
    let token = v.path("token");
    std::fs::write(&token, "enrollment-token-12345678901234567890\n").unwrap();
    let mut a = vec![
        "node",
        "install",
        "--hub",
        "http://10.10.0.2:8743",
        "--token-file",
        token.to_str().unwrap(),
        "--target",
        target,
        "--plan",
    ];
    a.extend_from_slice(extra);
    // The numbered steps go to stderr and the closing line to stdout; read them as one.
    let o = v.run_text(&a);
    common::Out {
        code: o.code,
        stdout: o.all(),
        stderr: String::new(),
    }
}

const APPLY: &str = "id=wg0,path=/etc/wireguard/wg0.conf,project=wg/vps,exporter=wireguard,apply=true,reload=wg-quick@wg0,require_approval=true";
const OBSERVE: &str = "id=wg0,path=/etc/wireguard/wg0.conf,project=wg/vps,exporter=wireguard";

#[test]
fn plan_lists_every_step_and_says_nothing_was_changed() {
    let v = Vault::new("plan");
    let o = plan(&v, APPLY, &["--relay", "10.10.0.2:8743"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(o.stdout.contains("[1/"), "steps are numbered: {}", o.stdout);
    assert!(
        o.stdout.contains("useradd --system"),
        "a dedicated unprivileged account: {}",
        o.stdout
    );
    assert!(
        o.stdout.contains("/etc/unv-node/node.toml (mode 600)"),
        "the config is private: {}",
        o.stdout
    );
    assert!(o.stdout.contains("unv-node.service"), "{}", o.stdout);
    assert!(
        o.stdout.contains("Plan only; nothing was changed."),
        "{}",
        o.stdout
    );
    assert!(
        !std::path::Path::new("/etc/unv-node/node.toml").exists(),
        "a plan must not install anything"
    );
}

#[test]
fn an_applying_target_gets_write_access_to_exactly_its_file_and_a_reload_unit_for_one_service() {
    let v = Vault::new("planapply");
    let o = plan(&v, APPLY, &[]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        o.stdout
            .contains("setfacl -m u:unv-node:rw /etc/wireguard/wg0.conf"),
        "{}",
        o.stdout
    );
    assert!(
        o.stdout.contains("unv-reload-wg0.path") && o.stdout.contains("unv-reload-wg0.service"),
        "the agent never gains the right to reload anything else: {}",
        o.stdout
    );
}

#[test]
fn a_target_without_apply_only_observes() {
    let v = Vault::new("planobserve");
    let o = plan(&v, OBSERVE, &[]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        o.stdout
            .contains("setfacl -m u:unv-node:r /etc/wireguard/wg0.conf"),
        "read only: {}",
        o.stdout
    );
    assert!(
        !o.stdout.contains("u:unv-node:rw"),
        "an observing target is never writable: {}",
        o.stdout
    );
    assert!(
        !o.stdout.contains("unv-reload-"),
        "and has no reload unit: {}",
        o.stdout
    );
}

#[test]
fn a_relay_adds_the_loopback_relay_service_and_without_one_there_is_none() {
    let v = Vault::new("planrelay");
    let with = plan(&v, OBSERVE, &["--relay", "10.10.0.2:8743"]);
    assert!(
        with.stdout.contains("unv-hub-relay.service"),
        "{}",
        with.stdout
    );
    let without = plan(&v, OBSERVE, &[]);
    assert!(
        !without.stdout.contains("unv-hub-relay"),
        "{}",
        without.stdout
    );
}

#[test]
fn a_malformed_target_is_refused_before_any_plan_is_printed() {
    let v = Vault::new("planbad");
    for target in ["id=a,project=p", "id=a,path=/etc/a.conf", "nonsense"] {
        let o = plan(&v, target, &[]);
        assert_ne!(o.code, 0, "{target}: {}", o.all());
        assert!(
            !o.stdout.contains("[1/"),
            "no half plan for a bad target: {}",
            o.stdout
        );
    }
}

fn config(v: &Vault, body: &str) -> String {
    let p = v.path("node.toml");
    std::fs::write(&p, body).unwrap();
    p.to_str().unwrap().to_string()
}

#[test]
fn check_reports_each_target_and_whether_its_file_is_there() {
    let v = Vault::new("check");
    let file = v.path("app.env");
    let cfg = config(
        &v,
        &format!(
            "[[target]]\nid = \"edge\"\npath = \"{}\"\nproject = \"edge\"\nexporter = \"env\"\nmode = \"push\"\napply = false\n",
            file.display()
        ),
    );
    let absent = v.run(&["node", "check", "--config", &cfg]);
    assert_eq!(absent.code, 0, "{}", absent.all());
    let t = &absent.json()["data"]["targets"][0];
    assert_eq!(
        (
            t["id"].as_str().unwrap(),
            t["apply"].as_bool().unwrap(),
            t["present"].as_bool().unwrap()
        ),
        ("edge", false, false)
    );
    assert!(t["sha256"].is_null(), "{t}");

    std::fs::write(&file, "TOKEN=abc\n").unwrap();
    let present = v.run(&["node", "check", "--config", &cfg]);
    let t = &present.json()["data"]["targets"][0];
    assert_eq!(t["present"], true, "{t}");
    assert_eq!(
        t["sha256"].as_str().unwrap().len(),
        64,
        "the hash the hub compares: {t}"
    );
}

#[test]
fn check_of_a_missing_config_is_not_found() {
    let v = Vault::new("checkmissing");
    let o = v.run(&[
        "node",
        "check",
        "--config",
        v.path("nope.toml").to_str().unwrap(),
    ]);
    assert_eq!(o.code, 3, "{}", o.all());
}
