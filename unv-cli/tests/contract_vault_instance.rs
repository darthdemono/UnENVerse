// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Vault identity (R19): a vault has an id and an instance; a restored copy starts in standby, will
//! not act, says so in `doctor`, and becomes live only through `unv vault activate`.

mod common;
use common::Vault;

fn role(v: &Vault) -> String {
    let o = v.run(&["vault", "status"]);
    assert_eq!(o.code, 0, "{}", o.all());
    o.json()["data"]["identity"]["role"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_new_vault_is_active_at_epoch_one_and_keeps_its_ids() {
    let v = Vault::new("id-new");
    let a = v.run(&["vault", "status"]).json()["data"]["identity"].clone();
    assert_eq!(a["role"], "active");
    assert_eq!(a["epoch"], 1);
    let b = v.run(&["vault", "status"]).json()["data"]["identity"].clone();
    assert_eq!(a, b);
    assert!(a["vault_id"].as_str().unwrap().len() >= 32);
}

#[test]
fn a_restore_marker_makes_the_next_open_a_standby_that_refuses_work_until_activated() {
    let v = Vault::new("id-standby");
    v.ok(&["entry", "add", "CF", "--key", "a-key-value-for-the-test"]);
    let before = v.run(&["vault", "status"]).json()["data"]["identity"].clone();

    // What `unv recover restore` leaves beside the restored database.
    std::fs::write(v.db().with_file_name("vault.restored"), "").unwrap();
    let after = v.run(&["vault", "status"]).json()["data"]["identity"].clone();
    assert_eq!(after["role"], "standby");
    assert_eq!(after["vault_id"], before["vault_id"], "still that vault");
    assert_ne!(
        after["instance_id"], before["instance_id"],
        "a new instance"
    );
    assert!(
        !v.db().with_file_name("vault.restored").exists(),
        "the marker is spent"
    );

    // It reads fine, says so in doctor, and will not rotate.
    assert_eq!(v.run(&["get", "CF"]).code, 0);
    let doctor = v.run(&["doctor"]).all();
    assert!(doctor.contains("Standby instance"), "{doctor}");
    let rot = v.run(&[
        "rotate",
        "run",
        "CF",
        "--token-id",
        "x",
        "--base-url",
        "http://127.0.0.1:9",
    ]);
    assert_ne!(rot.code, 0);
    assert!(rot.all().contains("standby"), "{}", rot.all());

    // Activation needs a yes, then raises the epoch above anything before.
    let refused = v.run_no_yes(&["vault", "activate"]);
    assert_ne!(refused.code, 0);
    assert!(refused.all().contains("--yes"), "{}", refused.all());
    assert_eq!(role(&v), "standby");
    let on = v.run(&["vault", "activate"]);
    assert_eq!(on.code, 0, "{}", on.all());
    assert_eq!(on.json()["data"]["after"]["epoch"], 2);
    assert_eq!(role(&v), "active");
    let doctor = v.run(&["doctor"]).all();
    assert!(doctor.contains("Active instance"), "{doctor}");
}

#[test]
fn a_dry_run_activation_changes_nothing() {
    let v = Vault::new("id-dry");
    std::fs::write(v.db().with_file_name("vault.restored"), "").unwrap();
    assert_eq!(role(&v), "standby");
    let o = v.run_with(&["--dry-run"], &["vault", "activate"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(role(&v), "standby");
}

#[test]
fn a_vault_in_a_sync_folder_is_named_by_status_and_doctor() {
    let v = Vault::new("id-sync");
    // `Vault::new` puts the database in a temp directory; a Syncthing marker above it is enough.
    std::fs::create_dir_all(v.db().parent().unwrap().join(".stfolder")).unwrap();
    let status = v.run(&["vault", "status"]);
    assert_eq!(status.json()["data"]["sync_folder"], "Syncthing");
    let doctor = v.run(&["doctor"]).all();
    assert!(doctor.contains("Syncthing"), "{doctor}");
}

#[test]
fn a_standby_instance_runs_no_operations_until_it_is_activated() {
    let v = Vault::new("id-ops");
    v.ok(&["entry", "add", "Seed", "--key", "seed-secret-123456"]);
    let dest = v.path("dest");
    std::fs::create_dir_all(&dest).unwrap();
    v.ok(&[
        "backup",
        "dest",
        "add",
        "local",
        "main",
        dest.to_str().unwrap(),
    ]);
    v.ok(&["backup", "recovery-key"]);

    std::fs::write(v.db().with_file_name("vault.restored"), "").unwrap();
    let blocked = v.run(&["ops", "run", "--dest", "main"]);
    assert_ne!(blocked.code, 0, "{}", blocked.all());
    assert!(blocked.all().contains("standby"), "{}", blocked.all());

    assert_eq!(v.run(&["vault", "activate"]).code, 0);
    let ran = v.run(&["ops", "run", "--dest", "main"]);
    assert_eq!(ran.code, 0, "{}", ran.all());
}
