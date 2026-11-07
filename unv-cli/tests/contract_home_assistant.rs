// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The Home Assistant recipe in `book/src/home-assistant.md` (R29): a `secrets.yaml` template with
//! `${Entry}` references renders to the real file, `--strict` refuses an unresolved reference
//! instead of writing an empty secret, and stdout never shows a value.

mod common;
use common::Vault;

const DB: &str = "ha-postgres-password-5521";

fn vault(tag: &str) -> (Vault, std::path::PathBuf) {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "HA Postgres", "--key", DB]);
    let tpl = v.db().with_file_name("secrets.yaml.tpl");
    (v, tpl)
}

#[test]
fn the_template_renders_to_a_file_with_the_real_value() {
    let (v, tpl) = vault("ha-render");
    std::fs::write(
        &tpl,
        "recorder_db_url: postgresql://homeassistant:${HA Postgres}@db/homeassistant\n",
    )
    .unwrap();
    let out = v.db().with_file_name("secrets.yaml");
    let o = v.run(&[
        "render",
        tpl.to_str().unwrap(),
        "--strict",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    let text = std::fs::read_to_string(&out).unwrap();
    assert_eq!(
        text,
        format!("recorder_db_url: postgresql://homeassistant:{DB}@db/homeassistant\n")
    );
    assert!(
        !o.all().contains(DB),
        "stdout must not show the value: {}",
        o.all()
    );
}

#[test]
fn strict_refuses_a_reference_that_does_not_resolve_and_writes_no_file() {
    let (v, tpl) = vault("ha-strict");
    std::fs::write(&tpl, "long_lived_token: ${HA Missing Entry}\n").unwrap();
    let out = v.db().with_file_name("secrets-missing.yaml");
    let o = v.run(&[
        "render",
        tpl.to_str().unwrap(),
        "--strict",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(
        !out.exists(),
        "an unresolved reference must not become a file"
    );
}
