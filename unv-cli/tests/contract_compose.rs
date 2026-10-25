// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The Compose secrets guide (`book/src/compose-secrets.md`), run: the recipe writes the value as
//! an owner-only file with no trailing newline, and Compose accepts the stack that mounts it.
#![cfg(unix)]

mod common;
use common::Vault;
use std::os::unix::fs::PermissionsExt;

const PAGE: &str = include_str!("../../book/src/compose-secrets.md");

const COMPOSE: &str = "services:
  db:
    image: postgres:16
    environment:
      POSTGRES_PASSWORD_FILE: /run/secrets/db_password
    secrets:
      - db_password

secrets:
  db_password:
    file: ./secrets/db_password
";

#[test]
fn the_guide_writes_an_owner_only_secret_file_without_a_newline_and_compose_accepts_the_stack() {
    for shown in [
        "( umask 077; unv exec --entry \"Demo Postgres=DB_PASSWORD\" -- sh -c 'printf %s \"$DB_PASSWORD\"' > secrets/db_password )",
        "docker compose config -q",
        COMPOSE,
        "unv render env.template --out .env",
    ] {
        assert!(PAGE.contains(shown), "the guide no longer shows: {shown}");
    }
    let v = Vault::new("compose");
    let value = format!("demo-compose-value-{}", std::process::id());
    v.ok(&["entry", "add", "Demo Postgres", "--key", &value]);
    std::fs::create_dir_all(v.path("secrets")).unwrap();
    std::fs::write(v.path("compose.yaml"), COMPOSE).unwrap();
    let o = v.sh(&format!(
        "cd '{}' && export UNV_DB_PATH=\"$VAULT\" && ( umask 077; \"$UNV\" --yes exec --entry \"Demo Postgres=DB_PASSWORD\" -- sh -c 'printf %s \"$DB_PASSWORD\"' > secrets/db_password )",
        v.dir.display()
    ));
    assert_eq!(o.code, 0, "{}", o.all());
    let file = v.path("secrets/db_password");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), value);
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );

    // Compose is a developer tool, not a dependency: the validation runs where it is installed.
    let compose = std::process::Command::new("docker")
        .args(["compose", "version"])
        .output();
    if compose.is_ok_and(|c| c.status.success()) {
        let r = std::process::Command::new("docker")
            .args(["compose", "-f"])
            .arg(v.path("compose.yaml"))
            .args(["config", "-q"])
            .output()
            .unwrap();
        assert!(
            r.status.success(),
            "docker compose config -q: {}",
            String::from_utf8_lossy(&r.stderr)
        );
    }
}
