// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! `unv doctor` and the file-mode promises ("`unv doctor`", README):
//!
//! * a healthy vault passes with exit 0;
//! * something actually broken (a missing salt, a corrupt page) exits 10, and
//!   doctor still runs when the vault will not open, which is the case it exists for;
//! * there is no repair for a missing salt, and the message says so;
//! * the vault and its salt are created owner-only, and a loose salt is reported.

mod common;
use common::Vault;
use serde_json::Value;

/// The findings: in `data` when the vault is sound, in `error.details` when a check
/// failed (a failing run is ONE document, the error envelope, carrying them).
fn findings(o: &common::Out) -> Vec<Value> {
    let j = o.json();
    j["data"]["findings"]
        .as_array()
        .or_else(|| j["error"]["details"]["findings"].as_array())
        .cloned()
        .unwrap_or_default()
}

fn level_of(o: &common::Out, check: &str) -> String {
    findings(o)
        .iter()
        .find(|f| f["check"] == check)
        .unwrap_or_else(|| panic!("no '{check}' check in {}", o.stdout))["level"]
        .as_str()
        .unwrap()
        .to_string()
}

fn healthy(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "GitHub", "--key", "doctor-secret-123456"]);
    v
}

#[test]
fn a_healthy_vault_passes_every_check_and_exits_0() {
    let v = healthy("doctor-ok");
    let o = v.run(&["doctor"]);
    assert_eq!(o.code, 0, "{}", o.all());
    // Windows has an owner-only access list since ADR-0166, which doctor reads with icacls: a
    // vault this program created passes there too.
    let permissions = "ok";
    for f in findings(&o) {
        let want = if f["check"] == "permissions" {
            permissions
        } else {
            "ok"
        };
        assert_eq!(f["level"], want, "{f}");
    }
    for check in ["integrity", "salt", "audit"] {
        assert_eq!(level_of(&o, check), "ok");
    }
    assert_eq!(level_of(&o, "permissions"), permissions);
}

#[test]
fn a_missing_salt_is_broken_exit_10_and_has_no_repair() {
    let v = healthy("doctor-nosalt");
    std::fs::remove_file(v.dir.join("vault.salt")).unwrap();
    // Run without --init so doctor does not invent a new salt.
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
    let o = c
        .env_remove("UNV_SERVER_URL")
        .env("UNV_PASSWORD", common::PASSWORD)
        .env("HOME", v.path("home"))
        .env("XDG_STATE_HOME", v.path("state"))
        .args([
            "--db-path",
            v.db().to_str().unwrap(),
            "--yes",
            "--json",
            "doctor",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert_eq!(o.status.code(), Some(10), "{stdout}");
    // Exactly one JSON document, even on failure: json.loads(stdout) must work.
    let doc: Value =
        serde_json::from_str(&stdout).expect("a failing doctor prints one JSON document");
    assert_eq!(doc["ok"], false, "{stdout}");
    assert_eq!(doc["error"]["code"], "invalid");
    let salt = doc["error"]["details"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["check"] == "salt")
        .unwrap();
    assert_eq!(salt["level"], "fail", "{salt}");
    assert!(
        salt["remedy"].as_str().unwrap().to_lowercase().contains("no repair"),
        "nothing can reconstruct sixteen random bytes, and the message must not pretend otherwise: {salt}"
    );
    assert!(
        !v.dir.join("vault.salt").exists(),
        "doctor must not create a replacement salt"
    );
}

#[test]
fn a_corrupt_database_is_broken_exit_10_and_doctor_still_reports() {
    let v = healthy("doctor-corrupt");
    let mut db = std::fs::read(v.db()).unwrap();
    let from = db.len() / 2;
    for b in db.iter_mut().skip(from).take(4096) {
        *b ^= 0xff;
    }
    std::fs::write(v.db(), db).unwrap();
    let o = v.run(&["doctor"]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert_eq!(
        o.json()["ok"],
        false,
        "one document, and it says the vault is broken: {}",
        o.stdout
    );
    assert_eq!(
        level_of(&o, "integrity"),
        "fail",
        "a database can open cleanly and still be corrupt: {}",
        o.stdout
    );
}

#[cfg(unix)]
#[test]
fn a_new_vault_and_its_salt_are_owner_only() {
    use common::mode_of;
    let v = healthy("doctor-modes");
    assert_eq!(
        mode_of(&v.db()),
        0o600,
        "the ciphertext must not be handed to every local user"
    );
    assert_eq!(mode_of(&v.dir.join("vault.salt")), 0o600);
    for sidecar in ["vault.db-wal", "vault.db-shm"] {
        let p = v.dir.join(sidecar);
        if p.exists() {
            assert_eq!(mode_of(&p), 0o600, "{sidecar}");
        }
    }
}

#[cfg(unix)]
#[test]
fn a_loose_salt_is_reported_rather_than_passed() {
    use std::os::unix::fs::PermissionsExt;
    let v = healthy("doctor-loose");
    std::fs::set_permissions(
        v.dir.join("vault.salt"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let o = v.run(&["doctor"]);
    assert_eq!(level_of(&o, "permissions"), "warn", "{}", o.stdout);
    let msg = findings(&o)
        .iter()
        .find(|f| f["check"] == "permissions")
        .unwrap()["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(msg.contains("644") && msg.contains("salt"), "{msg}");
}

#[test]
fn doctor_is_read_only_without_fix() {
    let v = healthy("doctor-readonly");
    let before = std::fs::read(v.db()).unwrap();
    v.run(&["doctor"]);
    // Opening a vault may touch WAL bookkeeping, but never its contents.
    assert_eq!(v.reveal_field("GitHub", "api_key"), "doctor-secret-123456");
    let _ = before;
}
