//! Recovery destinations and schedules ("Backups", README):
//!
//! * a local destination is registered under an id, listed, and refused when the
//!   id is taken (a conflict) or the path is not usable;
//! * a schedule is an interval, a daily time in an IANA zone, or a monthly drill;
//!   a bad time, an unknown zone or an unknown destination is refused with the
//!   code the README gives it, and a daily time is converted to real UTC;
//! * a package can be written on demand, and removing a destination does not
//!   delete the packages it already holds.

mod common;
use common::Vault;
use serde_json::Value;

fn with_dest(tag: &str) -> Vault {
    let v = Vault::new(tag);
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
    v
}

fn schedules(v: &Vault) -> Vec<Value> {
    v.ok(&["backup", "schedule", "ls"])["schedules"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn a_destination_is_registered_listed_and_its_id_is_unique() {
    let v = with_dest("dest");
    let ls = v.ok(&["backup", "dest", "ls"]);
    let d = &ls["destinations"][0];
    assert_eq!(
        (d["id"].as_str().unwrap(), d["transport"].as_str().unwrap()),
        ("main", "local"),
        "{ls}"
    );

    let dup = v.run(&[
        "backup",
        "dest",
        "add",
        "local",
        "main",
        v.path("dest").to_str().unwrap(),
    ]);
    assert_eq!(dup.code, 6, "'already exists' is a conflict: {}", dup.all());
    assert_eq!(dup.error_code(), "conflict");
    assert_eq!(
        v.ok(&["backup", "dest", "ls"])["destinations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn removing_a_destination_leaves_the_packages_it_holds() {
    let v = with_dest("destrm");
    v.ok(&["backup", "recovery-key"]);
    v.ok(&["backup", "package", "--dest", "main"]);
    let held = |v: &Vault| {
        std::fs::read_dir(v.path("dest"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "unvpkg"))
            .count()
    };
    assert_eq!(held(&v), 1);
    let rm = v.run(&["backup", "dest", "rm", "main"]);
    assert_eq!(rm.code, 0, "{}", rm.all());
    assert!(v.ok(&["backup", "dest", "ls"])["destinations"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        held(&v),
        1,
        "unregistering a destination never deletes a backup"
    );
    assert_eq!(
        v.run(&["backup", "dest", "rm", "main"]).code,
        3,
        "removing it twice is not_found"
    );
}

#[test]
fn an_interval_schedule_is_due_one_interval_from_now() {
    let v = with_dest("schedinterval");
    let s = v.ok(&[
        "backup",
        "schedule",
        "add",
        "--dest",
        "main",
        "--interval",
        "3600",
    ]);
    let sched = &s["schedule"];
    assert_eq!(sched["target"], "main");
    assert_eq!(sched["enabled"], true);
    assert!(
        sched["rule"].as_str().unwrap().contains("\"seconds\":3600"),
        "{sched}"
    );
    assert_eq!(schedules(&v).len(), 1);
}

#[test]
fn a_daily_schedule_is_converted_from_its_time_zone_to_utc() {
    let v = with_dest("scheddaily");
    let s = v.ok(&[
        "backup",
        "schedule",
        "add",
        "--dest",
        "main",
        "--daily",
        "03:30",
        "--time-zone",
        "UTC",
    ]);
    let due = s["schedule"]["next_due_at"].as_str().unwrap();
    assert!(
        due.contains("T03:30:00"),
        "03:30 UTC is due at 03:30 UTC: {due}"
    );

    let shifted = v.ok(&[
        "backup",
        "schedule",
        "add",
        "--dest",
        "main",
        "--daily",
        "03:30",
        "--time-zone",
        "Asia/Tokyo",
    ]);
    let due = shifted["schedule"]["next_due_at"].as_str().unwrap();
    assert!(
        due.contains("T18:30:00"),
        "03:30 in Tokyo (UTC+9, no daylight saving) is 18:30 UTC the day before: {due}"
    );
}

#[test]
fn a_bad_time_an_unknown_zone_and_an_unknown_destination_are_refused_with_their_codes() {
    let v = with_dest("schedbad");
    let bad_time = v.run(&[
        "backup", "schedule", "add", "--dest", "main", "--daily", "25:99",
    ]);
    assert_eq!(bad_time.code, 10, "{}", bad_time.all());
    let bad_zone = v.run(&[
        "backup",
        "schedule",
        "add",
        "--dest",
        "main",
        "--daily",
        "03:30",
        "--time-zone",
        "Not/AZone",
    ]);
    assert_eq!(bad_zone.code, 10, "{}", bad_zone.all());
    let no_dest = v.run(&[
        "backup",
        "schedule",
        "add",
        "--dest",
        "nope",
        "--interval",
        "60",
    ]);
    assert_eq!(no_dest.code, 3, "{}", no_dest.all());
    assert!(schedules(&v).is_empty(), "a refused schedule is not stored");
}

#[test]
fn interval_and_daily_cannot_be_combined() {
    let v = with_dest("schedboth");
    let o = v.run(&[
        "backup",
        "schedule",
        "add",
        "--dest",
        "main",
        "--interval",
        "60",
        "--daily",
        "03:30",
    ]);
    assert_eq!(o.code, 2, "clap refuses the combination: {}", o.all());
}

#[test]
fn a_monthly_drill_can_be_scheduled_for_an_existing_destination() {
    let v = with_dest("scheddrill");
    let s = v.run(&["backup", "schedule", "drill", "--dest", "main"]);
    assert_eq!(s.code, 0, "{}", s.all());
    assert_eq!(schedules(&v).len(), 1);
    let missing = v.run(&["backup", "schedule", "drill", "--dest", "nope"]);
    assert_eq!(missing.code, 3, "{}", missing.all());
}

#[test]
fn a_package_needs_a_recovery_identity_first_and_writes_nothing_without_one() {
    let v = with_dest("packagenokey");
    let o = v.run(&["backup", "package", "--dest", "main"]);
    assert_ne!(
        o.code,
        0,
        "an unencrypted package must never be written: {}",
        o.all()
    );
    let written: Vec<_> = std::fs::read_dir(v.path("dest"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "unvpkg"))
        .collect();
    assert!(
        written.is_empty(),
        "no package without a key to encrypt it to"
    );
}

#[test]
fn a_package_is_written_on_demand_with_a_receipt_naming_its_destination() {
    let v = with_dest("package");
    v.ok(&["backup", "recovery-key"]);
    let made = v.ok(&["backup", "package", "--dest", "main"]);
    assert_eq!(made["receipt"]["destination_id"], "main", "{made}");
    assert_eq!(made["receipt"]["manifest"]["entry_count"], 1, "{made}");
}
