// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The desktop commands that work on an unlocked vault, called through the mock
//! runtime. The rules held to account are the ones the UI depends on: names are
//! unique with a message a person can read, a second factor is never reported
//! as enabled before it is confirmed, a destination keeps its packages, and
//! "Open to LAN" never outlives the unlock.

// Windows resolves app_data_dir from the profile, not from XDG_DATA_HOME, so these tests would unlock the runner's (or a developer's) real vault. They run on Unix, where the harness isolates the data directory.
#![cfg(unix)]

mod common;
use common::{desk, Desk};
use serde_json::{json, Value};

fn unlocked(tag: &str) -> Desk {
    let d = desk(tag);
    d.unlock("session-master-password-1").unwrap();
    d
}

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap().to_string()
}

#[test]
fn a_user_name_is_unique_and_the_refusal_is_in_words_not_sql() {
    let d = unlocked("usernames");
    let alice = d.ok(
        "create_user",
        json!({ "username": "alice", "password": "alice-password-12345" }),
    );
    assert_eq!(alice["username"], "alice");
    assert_eq!(alice["has_password"], true);
    assert_eq!(alice["is_owner"], false);
    // Bug it pins: a duplicate name surfaced as `UNIQUE constraint failed: users.username`.
    let dup = d
        .call(
            "create_user",
            json!({ "username": "alice", "password": "other-password-12345" }),
        )
        .unwrap_err();
    assert_eq!(dup, json!("That name is already taken"));

    let bob = d.ok(
        "create_user",
        json!({ "username": "bob", "password": "bob-password-1234567" }),
    );
    let clash = d
        .call(
            "rename_user",
            json!({ "userId": id(&bob), "newUsername": "alice" }),
        )
        .unwrap_err();
    assert_eq!(clash, json!("That name is already taken"));
    d.ok(
        "rename_user",
        json!({ "userId": id(&bob), "newUsername": "robert" }),
    );
    let names: Vec<String> = d
        .ok("list_users", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["username"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"robert".to_string()) && names.contains(&"owner".to_string()),
        "{names:?}"
    );

    d.ok("delete_user", json!({ "userId": id(&bob) }));
    assert!(!d.ok("list_users", json!({})).to_string().contains("robert"));
}

#[test]
fn a_class_name_is_unique_and_permissions_are_read_back_as_written() {
    let d = unlocked("classes");
    let new = |name: &str| json!({ "name": name, "description": "d", "capManageUsers": false, "capManageClasses": false, "capDeleteProjects": false });
    let ops = d.ok("create_user_class", new("Ops"));
    assert_eq!(ops["cap_manage_users"], false);
    let dup = d.call("create_user_class", new("Ops")).unwrap_err();
    assert_eq!(dup, json!("That name is already taken"));
    let other = d.ok("create_user_class", new("Dev"));
    let mut rename = new("Ops");
    rename["classId"] = json!(id(&other));
    assert_eq!(
        d.call("update_user_class", rename).unwrap_err(),
        json!("That name is already taken")
    );

    d.ok(
        "set_class_permissions",
        json!({ "classId": id(&ops), "permissions": { "read": "project:Web", "write": "" } }),
    );
    let got = d.ok("get_class_permissions", json!({ "classId": id(&ops) }));
    assert_eq!(got, json!({ "read": "project:Web", "write": "" }));

    let user = d.ok(
        "create_user",
        json!({ "username": "carol", "password": "carol-password-12345" }),
    );
    d.ok(
        "assign_user_class",
        json!({ "userId": id(&user), "classId": id(&ops) }),
    );
    let listed = d.ok("list_users", json!({}));
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["username"] == "carol" && u["class_id"] == json!(id(&ops))),
        "{listed}"
    );
    assert!(
        d.call("delete_user_class", json!({ "classId": id(&ops) }))
            .is_err(),
        "a class with members is not deleted"
    );
    d.ok(
        "assign_user_class",
        json!({ "userId": id(&user), "classId": null }),
    );
    d.ok("delete_user_class", json!({ "classId": id(&ops) }));
}

#[test]
fn a_second_factor_is_not_enabled_until_a_code_confirms_it() {
    let d = unlocked("totp");
    let user = d.ok(
        "create_user",
        json!({ "username": "dave", "password": "dave-password-123456" }),
    );
    let uid = id(&user);
    assert_eq!(
        d.ok("totp_status", json!({ "userId": uid }))["enrolled"],
        false
    );
    let enrolled = d.ok("totp_enroll", json!({ "userId": uid }));
    assert_eq!(
        (enrolled["enrolled"].clone(), enrolled["enabled"].clone()),
        (json!(true), json!(false)),
        "{enrolled}"
    );
    assert!(
        enrolled["uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/"),
        "{enrolled}"
    );
    let wrong = d.ok("totp_confirm", json!({ "userId": uid, "code": "000000" }));
    assert_eq!(wrong, json!(false), "a wrong code does not enable it");
    assert_eq!(
        d.ok("totp_status", json!({ "userId": uid }))["enabled"],
        false
    );
    d.ok("totp_disable", json!({ "userId": uid }));
    assert_eq!(
        d.ok("totp_status", json!({ "userId": uid }))["enrolled"],
        false
    );
}

#[test]
fn tokens_are_listed_without_their_value_and_can_be_revoked() {
    let d = unlocked("tokens");
    let user = d.ok(
        "create_user",
        json!({ "username": "erin", "password": "erin-password-123456" }),
    );
    let uid = id(&user);
    let made = d.ok(
        "create_user_token",
        json!({ "userId": uid, "description": "ci" }),
    );
    let value = made["token"].as_str().unwrap().to_string();
    let listed = d.ok("list_user_tokens", json!({ "userId": uid }));
    assert_eq!(listed[0]["description"], "ci");
    assert!(
        !listed.to_string().contains(&value),
        "a token is shown once, at creation"
    );
    d.ok("revoke_user_token", json!({ "tokenId": made["token_id"] }));
    assert!(d
        .ok("list_user_tokens", json!({ "userId": uid }))
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn passwords_permissions_and_strict_write_apply_to_the_user() {
    let d = unlocked("userprefs");
    let user = d.ok(
        "create_user",
        json!({ "username": "fay", "password": "fay-password-1234567" }),
    );
    let uid = id(&user);
    d.ok(
        "set_user_password",
        json!({ "userId": uid, "password": "fay-new-password-123" }),
    );
    d.ok(
        "set_user_permissions",
        json!({ "userId": uid, "permissions": { "read": "project:Web", "write": "" } }),
    );
    assert_eq!(
        d.ok("get_user_permissions", json!({ "userId": uid })),
        json!({ "read": "project:Web", "write": "" })
    );
    d.ok(
        "set_strict_write",
        json!({ "subjectKind": "user", "subjectId": uid, "strict": true }),
    );
    let listed = d.ok("list_users", json!({}));
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["username"] == "fay" && u["strict_write"] == true),
        "{listed}"
    );
    assert!(d
        .call(
            "set_strict_write",
            json!({ "subjectKind": "planet", "subjectId": uid, "strict": true })
        )
        .is_err());
    d.ok(
        "set_user_password",
        json!({ "userId": uid, "password": null }),
    );
    assert!(
        d.ok("list_users", json!({}))
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["username"] == "fay" && u["has_password"] == false),
        "a null password removes the login"
    );
}

#[test]
fn history_timeline_doctor_and_audit_read_the_unlocked_vault() {
    let d = unlocked("reads");
    let doc = json!({ "api_keys": [{ "id": "k1", "provider": "GitHub", "api_key": "ghp_secret_value_123456" }], "projects": [], "user_categories": [] });
    d.ok("save_vault", json!({ "data": doc, "expectVersion": null }));
    assert!(d.ok("history_call", json!({ "op": "stats", "args": {} }))["snapshots"].is_number());
    assert!(d
        .call("history_call", json!({ "op": "no-such-op", "args": {} }))
        .is_err());
    assert!(d
        .ok("entry_timeline", json!({ "entryId": "nope" }))
        .as_array()
        .unwrap()
        .is_empty());
    let timeline = d.ok("entry_timeline", json!({ "entryId": "k1" }));
    assert!(
        !timeline.to_string().contains("ghp_secret_value_123456"),
        "secret-free: {timeline}"
    );
    let checks = d.ok("doctor_file", json!({}));
    assert!(
        checks
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["check"] == "integrity" && c["level"] == "ok"),
        "{checks}"
    );
    assert!(!checks.to_string().contains("ghp_secret_value_123456"));
}

#[test]
fn a_migration_plan_is_redacted_and_apply_saves_what_it_planned() {
    let d = unlocked("migrate");
    let args = json!({ "format": "env", "sourceLabel": "app.env", "text": "DB_PASSWORD=hunter2-real-pass\nMODE=live\n", "template": false });
    let plan = d.ok("migrate_plan", args.clone());
    assert!(
        !plan.to_string().contains("hunter2-real-pass"),
        "a plan shows fingerprints, never values: {plan}"
    );
    assert!(plan["counts"]["imported"].as_u64().unwrap() >= 1, "{plan}");
    assert!(
        d.ok("load_vault", json!({})).is_null()
            || d.ok("load_vault", json!({}))["data"]["api_keys"]
                .as_array()
                .is_none_or(|a| a.is_empty()),
        "planning writes nothing"
    );
    let applied = d.ok("migrate_apply", args);
    assert!(
        applied["counts"]["imported"].as_u64().unwrap() >= 1,
        "{applied}"
    );
    assert!(!applied.to_string().contains("hunter2-real-pass"));
    let after = d.ok("load_vault", json!({}));
    assert!(
        after["data"]["api_keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["provider"] == "DB_PASSWORD"),
        "{after}"
    );
}

#[test]
fn the_recovery_key_is_shown_once_and_destinations_packages_drills_and_schedules_work() {
    let d = unlocked("recovery");
    d.ok("save_vault", json!({ "data": { "api_keys": [{ "id": "k1", "provider": "Seed", "api_key": "seed-secret-123456" }], "projects": [], "user_categories": [] }, "expectVersion": null }));
    let key = d.ok("backup_recovery_key", json!({}));
    assert!(
        key[0].as_str().unwrap().starts_with("age1")
            && key[1].as_str().unwrap().starts_with("AGE-SECRET-KEY-"),
        "recipient and identity"
    );

    let dest = d.dir.join("dest");
    std::fs::create_dir_all(&dest).unwrap();
    let added = d.ok(
        "backup_destination_add",
        json!({ "id": "main", "path": dest }),
    );
    assert_eq!(added["id"], "main");
    assert!(
        d.call(
            "backup_destination_add",
            json!({ "id": "main", "path": dest })
        )
        .is_err(),
        "an id is unique"
    );
    assert_eq!(
        d.ok("backup_destination_list", json!({}))
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let receipt = d.ok("backup_package", json!({ "dest": "main" }));
    assert_eq!(receipt["destination_id"], "main", "{receipt}");
    let drill = d.ok("backup_drill", json!({ "dest": "main" }));
    assert_eq!(drill["state"], "succeeded", "{drill}");
    assert!(d
        .ok("ops_list", json!({}))
        .as_array()
        .is_some_and(|a| !a.is_empty()));
    let run_id = id(&drill);
    assert_eq!(
        d.ok("ops_show", json!({ "id": run_id }))["id"],
        json!(run_id)
    );
    assert_eq!(d.ok("ops_show", json!({ "id": "nope" })), Value::Null);
    assert!(d
        .call("ops_cancel", json!({ "id": "nope" }))
        .unwrap_err()
        .as_str()
        .unwrap()
        .contains("No such operation"));

    let interval = d.ok("backup_schedule_add", json!({ "dest": "main", "interval": 3600, "daily": null, "timeZone": null, "maxLateness": null }));
    assert_eq!(interval["target"], "main");
    assert!(d.call("backup_schedule_add", json!({ "dest": "nope", "interval": 60, "daily": null, "timeZone": null, "maxLateness": null })).is_err());
    d.ok(
        "backup_schedule_drill",
        json!({ "dest": "main", "day": null, "at": null, "timeZone": null, "maxLateness": null }),
    );
    assert_eq!(
        d.ok("backup_schedule_list", json!({}))
            .as_array()
            .unwrap()
            .len(),
        2
    );

    d.ok("backup_destination_remove", json!({ "id": "main" }));
    assert_eq!(
        std::fs::read_dir(&dest)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "unvpkg"))
            .count(),
        1,
        "removing a destination keeps its packages"
    );
}

#[test]
fn an_archive_round_trips_with_its_password_and_a_wrong_one_restores_nothing() {
    let d = unlocked("archive");
    d.ok("save_vault", json!({ "data": { "api_keys": [{ "id": "k1", "provider": "Kept", "api_key": "kept-secret-123456" }], "projects": [], "user_categories": [] }, "expectVersion": null }));
    // The archive copies the database file, so the pane says to lock first: an open
    // database can hold its latest pages in the write-ahead log.
    d.ok("lock_vault", json!({}));
    let text = d.ok(
        "backup_archive_build",
        json!({ "password": "archive-password-123" }),
    );
    d.unlock("session-master-password-1").unwrap();
    assert!(
        !text.as_str().unwrap().contains("kept-secret-123456"),
        "an archive is ciphertext"
    );
    d.ok("save_vault", json!({ "data": { "api_keys": [], "projects": [], "user_categories": [] }, "expectVersion": null }));
    assert!(d
        .call(
            "backup_archive_restore",
            json!({ "text": text, "password": "wrong-password-123456" })
        )
        .is_err());
    d.ok(
        "backup_archive_restore",
        json!({ "text": text, "password": "archive-password-123" }),
    );
    d.unlock("session-master-password-1").unwrap();
    let back = d.ok("load_vault", json!({}));
    assert!(
        back["data"]["api_keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["provider"] == "Kept"),
        "{back}"
    );
}

#[test]
fn open_to_lan_needs_a_user_serves_the_vault_and_does_not_outlive_the_lock() {
    let d = unlocked("lan");
    let refused = d
        .call("lan_start", json!({ "port": 38744, "tls": false }))
        .unwrap_err();
    assert!(
        refused
            .as_str()
            .unwrap()
            .contains("No user account exists yet"),
        "{refused}"
    );
    d.ok(
        "create_user",
        json!({ "username": "peer", "password": "peer-password-123456" }),
    );
    let st = d.ok("lan_start", json!({ "port": 38744, "tls": false }));
    assert_eq!(st["running"], true, "{st}");
    let port = st["port"].as_u64().unwrap() as u16;
    let answers = (0..40).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
    });
    assert!(answers, "the port answers");
    assert!(
        d.call("lan_start", json!({ "port": 38744, "tls": false }))
            .is_err(),
        "a second start is refused"
    );
    assert_eq!(d.ok("lan_status", json!({}))["running"], true);

    d.ok("lock_vault", json!({}));
    assert_eq!(
        d.ok("lan_status", json!({}))["running"],
        false,
        "locking stops the server that holds a copy of the key"
    );
    assert!(
        d.call("lan_start", json!({ "port": 38745, "tls": false }))
            .is_err(),
        "and a locked vault cannot be opened to the LAN"
    );
}

#[test]
fn a_remote_request_reaches_a_plain_server_and_reports_status_etag_and_body() {
    let d = desk("remote");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nETag: \"v7\"\r\nx-vault-merged: 1\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi");
        }
    });
    // Bug it pins: an unpinned request built a reqwest client with no rustls crypto
    // provider installed, and panicked instead of answering.
    let r = d.ok("remote_request", json!({ "url": format!("http://127.0.0.1:{port}/api/vault"), "method": "GET", "headersJson": "{}", "body": null, "fingerprint": null }));
    assert_eq!(
        (r["status"].clone(), r["body"].clone(), r["merged"].clone()),
        (json!(200), json!("hi"), json!(true)),
        "{r}"
    );
    assert_eq!(r["etag"], "\"v7\"");

    let down = d.call("remote_request", json!({ "url": "http://127.0.0.1:1/x", "method": "GET", "headersJson": "{}", "body": null, "fingerprint": null }));
    assert!(
        down.is_err(),
        "an unreachable server is an error the UI can show"
    );
    assert!(d.call("remote_request", json!({ "url": "http://127.0.0.1:1/x", "method": "NOT A METHOD", "headersJson": "{}", "body": null, "fingerprint": null })).is_err());
    assert!(d
        .call(
            "probe_cert_fingerprint",
            json!({ "url": "https://127.0.0.1:1" })
        )
        .is_err());
    assert!(d
        .call(
            "backup_sftp_host_key",
            json!({ "host": "127.0.0.1", "port": 1 })
        )
        .unwrap_err()
        .as_str()
        .unwrap()
        .contains("failed"));
}
