// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv support bundle` and `unv demo-vault` (R25): the bundle may be sent to a stranger, so it
//! must hold no name, value, path or host; the demo vault must be usable and honest.

mod common;
use common::Vault;

const NAMES: [&str; 4] = [
    "Zq9DistinctProject",
    "Qx7DistinctEntry",
    "distinct-secret-value-4411",
    "node-host.distinct.example",
];

#[test]
fn the_bundle_holds_counts_and_versions_and_none_of_the_names_values_or_paths() {
    let v = Vault::new("support-bundle");
    v.ok(&["project", "add", NAMES[0], "--type", "nginx"]);
    v.ok(&[
        "entry",
        "add",
        NAMES[1],
        "--key",
        NAMES[2],
        "--url",
        &format!("https://{}", NAMES[3]),
    ]);
    let o = v.run(&["support", "bundle"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let text = o.all();
    for n in NAMES {
        assert!(!text.contains(n), "the bundle leaked {n}");
    }
    let dir = v.dir.display().to_string();
    assert!(!text.contains(&dir), "the bundle leaked a path");
    let b = &o.json()["data"]["bundle"];
    assert_eq!(b["entries"], 1);
    assert_eq!(b["project_types"]["nginx"], 1);
    assert_eq!(b["bundle_version"], 1);
    assert!(b["sqlcipher_version"].is_string());
    assert!(b["doctor"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["check"].is_string() && c["level"].is_string()));
}

#[test]
fn the_bundle_can_be_written_to_a_file_and_read_back() {
    let v = Vault::new("support-file");
    v.ok(&[
        "entry",
        "add",
        "Anything",
        "--key",
        "some-value-for-the-test",
    ]);
    let file = v.path("bundle.json");
    let o = v.run(&["support", "bundle", "--out", file.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(on_disk["entries"], 1);
}

fn dir_pw(v: &Vault) -> std::path::PathBuf {
    v.path("demo/demo-password.txt")
}

/// `unv` run directly, with a clean environment, so the test is the same on every platform.
fn unv(v: &Vault, password: Option<&str>, args: &[&str]) -> (i32, String) {
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
    for k in [
        "UNV_SERVER_URL",
        "UNV_PASSWORD",
        "UNV_PASSWORD_FILE",
        "UNV_PASSWORD_COMMAND",
        "UNV_TOKEN",
        "UNV_USER",
        "UNV_PROJECT",
        "UNV_ENV_FILE",
    ] {
        c.env_remove(k);
    }
    c.env("HOME", v.path("home"))
        .env("XDG_DATA_HOME", v.path("data"))
        .env("XDG_CONFIG_HOME", v.path("config"))
        .env("XDG_STATE_HOME", v.path("state"))
        .env("UNV_SESSION_FILE", v.path("state/sessions.json"))
        .env("UNV_POOL_FILE", v.path("state/pools.json"))
        .stdin(std::process::Stdio::null());
    if let Some(p) = password {
        c.env("UNV_PASSWORD", p);
    }
    let o = c.args(args).output().unwrap();
    (
        o.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
    )
}

#[test]
fn a_demo_vault_opens_with_its_printed_password_passes_doctor_and_refuses_to_overwrite() {
    let v = Vault::new("demo-vault");
    let dir = v.path("demo");
    let dir = dir.to_str().unwrap();
    let db = v.path("demo/vault.db");
    let db = db.to_str().unwrap();

    let (code, text) = unv(&v, None, &["--json", "demo-vault", "--out", dir]);
    assert_eq!(code, 0, "{text}");
    assert!(
        !text.contains("demo-password-"),
        "the password must not be printed: {text}"
    );
    let password = std::fs::read_to_string(dir_pw(&v)).unwrap();

    let (code, text) = unv(&v, Some(&password), &["--db-path", db, "--json", "list"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("Demo Service A"), "{text}");

    let (code, text) = unv(&v, Some(&password), &["--db-path", db, "--json", "doctor"]);
    assert_eq!(code, 0, "{text}");

    let (code, _) = unv(&v, None, &["demo-vault", "--out", dir]);
    assert_ne!(code, 0, "an existing vault must not be overwritten");
}
