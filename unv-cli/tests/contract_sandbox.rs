// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv exec --sandbox` (R34). The sandbox itself is bubblewrap and needs user namespaces, which
//! not every machine allows, so the behaviour tests run only where a probe shows it works. What is
//! asserted everywhere is the part that must never depend on the machine: where the sandbox cannot
//! be enforced, nothing runs and the command exits 7.

mod common;
use common::Vault;

const SECRET: &str = "sandboxed-secret-value-9981";

/// True when `bwrap` can start a sandbox here.
fn sandbox_works() -> bool {
    std::process::Command::new("bwrap")
        .args([
            "--unshare-user",
            "--unshare-net",
            "--ro-bind",
            "/",
            "/",
            "--",
            "true",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn vault(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "Tok", "--key", SECRET]);
    v
}

#[test]
fn without_a_working_sandbox_nothing_runs_and_the_exit_is_7() {
    if sandbox_works() {
        eprintln!("skipped: a sandbox works on this machine");
        return;
    }
    let v = vault("sandbox-refuse");
    let marker = v.db().with_file_name("ran.txt");
    let script = format!("echo ran > {}", marker.display());
    let o = v.run(&[
        "exec",
        "--entry",
        "Tok=TOK",
        "--sandbox",
        "--",
        "sh",
        "-c",
        &script,
    ]);
    assert_eq!(o.code, 7, "{}", o.all());
    assert!(!marker.exists(), "the command must not have run");
    assert!(o.all().contains("unsandboxed"), "{}", o.all());
}

#[test]
fn a_sandboxed_command_gets_the_value_but_not_the_network_or_the_env_file() {
    if !sandbox_works() {
        eprintln!("skipped: no working bubblewrap here");
        return;
    }
    let v = vault("sandbox-run");
    // The working directory for the child is the vault's directory.
    let dir = v.db().parent().unwrap().to_path_buf();
    std::fs::write(dir.join(".env"), "PLAIN=top-secret-in-a-dotenv\n").unwrap();
    let o = v.run_in(
        &dir,
        &[
            "exec",
            "--entry",
            "Tok=TOK",
            "--sandbox",
            "--",
            "sh",
            "-c",
            "printf '%s|' \"$TOK\"; cat .env | wc -c",
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(o.stdout.starts_with(&format!("{SECRET}|")), "{}", o.stdout);
    // The .env file is shadowed by an empty one, so it reads as zero bytes.
    assert!(o.stdout.trim_end().ends_with('0'), "{}", o.stdout);
}

#[test]
fn the_sandbox_flags_are_listed_and_the_net_flag_needs_the_sandbox() {
    let v = Vault::new("sandbox-flags");
    let o = v.run(&["exec", "--sandbox-net", "--entry", "X", "--", "true"]);
    assert_eq!(o.code, 2, "clap rejects --sandbox-net alone: {}", o.all());
}
