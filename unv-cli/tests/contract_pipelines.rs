// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv` is built to sit in a pipeline ("an orchestrator, a CI job or an
//! agent"). A reader that stops reading early (`| head -1`, `| jq -e`, a
//! script that has what it needs) is normal, not a failure: the command must
//! end quietly instead of panicking with a Rust backtrace hint and exit 101.

mod common;
use common::Vault;
use std::io::Read;
use std::process::{Command, Stdio};

#[test]
fn a_reader_that_closes_the_pipe_early_ends_the_command_quietly() {
    let v = Vault::new("pipe");
    // `describe` prints a large document, so a one-byte reader certainly leaves
    // the writer with more to send than the pipe will take.
    let mut child = Command::new(env!("CARGO_BIN_EXE_unv"))
        .env_remove("UNV_SERVER_URL")
        .env("HOME", v.path("home"))
        .env("XDG_DATA_HOME", v.path("data"))
        .env("XDG_CONFIG_HOME", v.path("config"))
        .env("XDG_STATE_HOME", v.path("state"))
        .arg("describe")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdout = child.stdout.take().unwrap();
    let mut first = [0u8; 1];
    stdout.read_exact(&mut first).unwrap();
    assert_eq!(first[0], b'{', "describe prints a JSON document");
    drop(stdout); // the reader goes away mid-output

    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let status = child.wait().unwrap();

    assert!(
        !stderr.contains("panicked") && !stderr.contains("Broken pipe"),
        "a closed pipe is not an error to report: {stderr}"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "a reader that has what it needs must not turn the command into a failure (stderr: {stderr})"
    );
}

/// A failing command in envelope mode prints exactly ONE JSON document. Commands that
/// compute a result and then fail (verification, collisions) used to print a success
/// envelope first and the error after it: two values in a row, which no JSON parser
/// reads, on the run a caller most needs to understand.
#[test]
fn a_failing_verification_prints_one_json_document() {
    let v = Vault::new("onedoc");
    v.ok(&["entry", "add", "One", "--key", "first-secret-value-1234"]);
    v.ok(&["entry", "add", "Two", "--key", "second-secret-value-123"]);
    v.ok(&["entry", "add", "Three", "--key", "third-secret-value-1234"]);

    let salt = vault_core::read_or_create_salt(&v.dir.join("vault.salt")).unwrap();
    let key = vault_core::derive_key(common::PASSWORD, &salt).unwrap();
    {
        let conn = vault_core::open_db(&v.db(), &key).unwrap();
        conn.execute(
            "UPDATE vault_audit SET entry_provider = 'Forged' WHERE id = 2",
            [],
        )
        .unwrap();
    }

    for args in [vec!["audit", "--verify"], vec!["doctor"]] {
        let o = v.run(&args);
        assert_ne!(o.code, 0, "{args:?}: {}", o.all());
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&o.stdout);
        assert!(
            parsed.is_ok(),
            "`unv {}` must print ONE JSON document on failure: {}",
            args.join(" "),
            o.stdout
        );
        assert_eq!(parsed.unwrap()["ok"], false, "{args:?}");
    }
}
