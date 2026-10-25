// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The recipes in `book/src/contract.md`, run. A recipe that stops working, or a page that stops
//! showing one, fails here.
#![cfg(unix)]

mod common;
use common::Vault;
use std::os::unix::fs::PermissionsExt;

const PAGE: &str = include_str!("../../book/src/contract.md");

fn secret() -> String {
    format!("tok-{}", std::process::id())
}

fn seeded(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "GitHub", "--key", &secret()]);
    v
}

fn shown(recipe: &str) {
    assert!(
        PAGE.contains(recipe),
        "contract.md no longer shows: {recipe}"
    );
}

#[test]
fn reading_a_field_needs_reveal_and_otherwise_gives_a_fingerprint() {
    shown("unv --reveal get GitHub --field api_key --json");
    shown("compare data.value.fingerprint");
    let v = seeded("recipe-get");
    assert_eq!(v.reveal_field("GitHub", "api_key"), secret());
    let hidden = v.run(&["get", "GitHub", "--field", "api_key"]);
    assert_eq!(hidden.code, 0, "{}", hidden.all());
    let value = &hidden.json()["data"]["value"];
    assert_eq!(value["redacted"], true);
    assert!(value["fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(!hidden.all().contains(&secret()));
}

#[test]
fn exec_render_and_file_write_hand_the_value_to_a_program_or_a_file() {
    shown("unv exec --entry GitHub=GH_TOKEN -- ./deploy.sh");
    shown("unv render template.txt --out config.ini");
    shown("( umask 077; unv exec --entry GitHub=TOKEN -- sh -c 'printf %s \"$TOKEN\"' > ~/.config/tool/token )");
    shown("unv file write Cert --out ./server.pem");
    let v = seeded("recipe-materialise");

    let o = v.run_text(&[
        "exec",
        "--entry",
        "GitHub=GH_TOKEN",
        "--",
        "sh",
        "-c",
        "printf %s \"$GH_TOKEN\"",
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.stdout, secret());

    let dir = std::env::temp_dir().join(format!("unv-recipe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let template = dir.join("template.txt");
    std::fs::write(&template, "token=${GitHub}\n").unwrap();
    let out = dir.join("config.ini");
    let r = v.run(&[
        "render",
        template.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(r.code, 0, "{}", r.all());
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        format!("token={}\n", secret())
    );

    let token = dir.join("token");
    let w = v.sh(&format!(
        "umask 077; \"$UNV\" --db-path \"$VAULT\" --yes exec --entry GitHub=TOKEN -- sh -c 'printf %s \"$TOKEN\"' > '{}'",
        token.display()
    ));
    assert_eq!(w.code, 0, "{}", w.all());
    assert_eq!(std::fs::read_to_string(&token).unwrap(), secret());
    assert_eq!(
        std::fs::metadata(&token).unwrap().permissions().mode() & 0o777,
        0o600
    );

    // A file kept in an entry comes back byte for byte.
    let source = dir.join("source.pem");
    std::fs::write(&source, "-----BEGIN TEST-----\nbody\n").unwrap();
    v.ok(&[
        "entry",
        "add",
        "Cert",
        "--blob-file",
        source.to_str().unwrap(),
    ]);
    let pem = dir.join("server.pem");
    let f = v.run(&["file", "write", "Cert", "--out", pem.to_str().unwrap()]);
    assert_eq!(f.code, 0, "{}", f.all());
    assert_eq!(
        std::fs::read(&pem).unwrap(),
        std::fs::read(&source).unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_failure_modes_the_page_promises_have_their_exit_codes() {
    let v = seeded("recipe-codes");
    assert_eq!(v.run(&["get", "Nonexistent"]).code, 3, "not found");
    v.ok(&[
        "entry",
        "add",
        "GitHub Enterprise",
        "--key",
        "another-token-value",
    ]);
    let ambiguous = v.run(&["entry", "history", "Git"]);
    assert_eq!(ambiguous.code, 4, "ambiguous: {}", ambiguous.all());
    for phrase in ["3 not found", "4 ambiguous", "7 unavailable"] {
        shown(phrase);
    }
}

// Bug: without `--clean`, `unv exec` handed the child UNV_PASSWORD, the master password.
#[test]
fn exec_does_not_pass_the_master_password_to_the_child() {
    let v = seeded("exec-no-password");
    for args in [
        vec![
            "exec",
            "--entry",
            "GitHub=T",
            "--",
            "sh",
            "-c",
            "printf %s \"${UNV_PASSWORD-absent}\"",
        ],
        vec![
            "shield",
            "--",
            "sh",
            "-c",
            "printf %s \"${UNV_PASSWORD-absent}\"",
        ],
    ] {
        let o = v.run_text(&args);
        assert_eq!(o.code, 0, "{}", o.all());
        assert_eq!(o.stdout, "absent", "{args:?}");
    }
}
