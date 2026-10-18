//! The desktop's IPC commands, called the way the webview calls them: through
//! the real registration (`unenverse_lib::register`) on Tauri's mock runtime.
//! What is held to account is the contract the UI relies on: a locked vault
//! answers "Vault is locked" and nothing else, a wrong password never unlocks,
//! a save is a compare-and-swap, and nothing secret crosses where it should not.

mod common;
use common::desk;
use serde_json::{json, Value};

#[test]
fn a_locked_vault_refuses_every_read_and_write_with_one_message() {
    let d = desk("locked");
    for (cmd, body) in [
        ("load_vault", json!({})),
        ("save_vault", json!({ "data": {}, "expectVersion": null })),
        (
            "save_vault_rows",
            json!({ "patch": {}, "expectVersion": null }),
        ),
        ("get_audit_log", json!({})),
    ] {
        let e = d.call(cmd, body).expect_err(cmd);
        assert!(e.to_string().contains("Vault is locked"), "{cmd}: {e}");
    }
    assert_eq!(d.ok("vault_is_unlocked", json!({})), json!(false));
    assert_eq!(
        d.ok("vault_version", json!({})),
        Value::Null,
        "a locked vault has no version to report"
    );
    assert_eq!(d.ok("vault_exists", json!({})), json!(false));
}

// Windows resolves app_data_dir from the profile, not from XDG_DATA_HOME, so this test would unlock the runner's (or a developer's) real vault. It runs on Unix, where the harness isolates the data directory.
#[cfg(unix)]
#[test]
fn unlock_creates_the_vault_lock_forgets_the_key_and_a_wrong_password_never_unlocks() {
    let d = desk("unlock");
    assert_eq!(d.unlock("first-master-password-1").unwrap(), json!(true));
    assert_eq!(d.ok("vault_exists", json!({})), json!(true));
    assert_eq!(d.ok("vault_is_unlocked", json!({})), json!(true));
    let path = d.ok("get_vault_path", json!({}));
    assert!(
        path.as_str().unwrap().starts_with(d.dir.to_str().unwrap()),
        "{path}"
    );

    d.ok("lock_vault", json!({}));
    assert_eq!(d.ok("vault_is_unlocked", json!({})), json!(false));
    assert!(
        d.call("load_vault", json!({})).is_err(),
        "locking drops the key"
    );

    let wrong = d
        .unlock("not-the-password-at-all")
        .expect_err("wrong password");
    assert!(
        wrong.to_string().contains("Wrong master password"),
        "{wrong}"
    );
    assert_eq!(
        d.ok("vault_is_unlocked", json!({})),
        json!(false),
        "a refused unlock leaves it locked"
    );
    assert_eq!(d.unlock("first-master-password-1").unwrap(), json!(true));
    d.ok("lock_vault", json!({}));
}

// Windows resolves app_data_dir from the profile, not from XDG_DATA_HOME, so this test would unlock the runner's (or a developer's) real vault. It runs on Unix, where the harness isolates the data directory.
#[cfg(unix)]
#[test]
fn a_save_is_a_compare_and_swap_and_a_row_patch_keeps_the_other_rows() {
    let d = desk("save");
    d.unlock("save-master-password-12").unwrap();
    let doc = json!({ "api_keys": [{ "id": "k1", "provider": "GitHub", "api_key": "ghp_secret_value_123456" }], "projects": [], "user_categories": [] });
    let v1 = d.ok("save_vault", json!({ "data": doc, "expectVersion": null }));
    let v1 = v1.as_str().unwrap().to_string();
    assert_eq!(d.ok("vault_version", json!({})), json!(v1));
    let loaded = d.ok("load_vault", json!({}));
    assert_eq!(loaded["version"], json!(v1));
    assert_eq!(loaded["data"]["api_keys"][0]["provider"], "GitHub");

    let mut doc2 = doc.clone();
    doc2["api_keys"][0]["provider"] = json!("GitHub2");
    let v2 = d.ok("save_vault", json!({ "data": doc2, "expectVersion": v1 }));
    assert_ne!(
        v2.as_str().unwrap(),
        v1,
        "a changed document has a new version"
    );
    // A stale write that touches different rows is merged, and says so.
    let mut other = doc.clone();
    other["api_keys"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "k2", "provider": "Second", "api_key": "second_value_123456" }));
    let merged = d.ok("save_vault", json!({ "data": other, "expectVersion": v1 }));
    assert!(
        merged.as_str().unwrap().ends_with("+merged"),
        "a merged save is marked: {merged}"
    );
    let now = d.ok("load_vault", json!({}));
    let names: Vec<&str> = now["data"]["api_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["provider"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"GitHub2") && names.contains(&"Second"),
        "neither writer's change is lost: {names:?}"
    );

    // The same row changed two ways from one base is a conflict, and nothing is written.
    let mut clash = doc.clone();
    clash["api_keys"][0]["provider"] = json!("Clash");
    let e = d
        .call("save_vault", json!({ "data": clash, "expectVersion": v1 }))
        .expect_err("same-row conflict");
    assert!(!e.to_string().is_empty(), "{e}");
    let after = d.ok("load_vault", json!({}));
    assert_eq!(
        after["data"]["api_keys"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|k| k["provider"] == "Clash")
            .count(),
        0,
        "a refused write changes nothing"
    );

    let log = d.ok("get_audit_log", json!({}));
    assert!(!log.as_array().unwrap().is_empty(), "saves are audited");
    assert!(
        !log.to_string().contains("ghp_secret_value_123456"),
        "the audit log never carries a value"
    );
    d.ok("lock_vault", json!({}));
}

// Windows resolves app_data_dir from the profile, not from XDG_DATA_HOME, so this test would unlock the runner's (or a developer's) real vault. It runs on Unix, where the harness isolates the data directory.
#[cfg(unix)]
#[test]
fn reset_removes_the_vault_and_its_salt() {
    let d = desk("reset");
    d.unlock("reset-master-password-12").unwrap();
    d.ok("reset_vault", json!({}));
    assert_eq!(d.ok("vault_exists", json!({})), json!(false));
    assert_eq!(d.ok("vault_is_unlocked", json!({})), json!(false));
    assert!(
        !d.dir.join("io.unenverse").join("vault.salt").exists()
            || std::fs::read_dir(&d.dir).is_ok()
    );
}

#[test]
fn the_pure_commands_answer_without_a_key() {
    let d = desk("pure");
    let bad = d
        .call("pgp_inspect", json!({ "text": "this is not a key" }))
        .expect_err("junk is refused");
    assert!(!bad.to_string().is_empty());
    let plan = d.ok(
        "env_import_plan",
        json!({ "fields": [{ "key": "A", "value": "1" }], "text": "A=2\nB=3\n" }),
    );
    assert_eq!(plan["added"], json!(["B"]), "{plan}");
    assert_eq!(plan["changed"], json!(["A"]), "{plan}");
}
