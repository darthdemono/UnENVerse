// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! `unv use` and `unv describe` ("Commands", "`unv describe`", README):
//!
//! * `use` pins a project and an environment to a directory in `.unv.json`;
//!   the file names things and never holds a value, an explicit flag wins over
//!   it, and `UNV_PROJECT` wins over the file;
//! * `describe` is the whole command tree, exit codes, envelope and redaction
//!   rules as one JSON document generated from the real definition, so it
//!   cannot describe a flag that does not exist.

mod common;
use common::Vault;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn list_in(v: &Vault, cwd: &Path, extra_env: &[(&str, &str)], args: &[&str]) -> Vec<String> {
    let mut c = Command::new(env!("CARGO_BIN_EXE_unv"));
    for var in [
        "UNV_SERVER_URL",
        "UNV_ENV_FILE",
        "UNV_PROJECT",
        "UNV_TOKEN",
        "UNV_USER",
    ] {
        c.env_remove(var);
    }
    c.current_dir(cwd)
        .env("UNV_PASSWORD", common::PASSWORD)
        .env("HOME", v.path("home"))
        .env("XDG_DATA_HOME", v.path("data"))
        .env("XDG_CONFIG_HOME", v.path("config"))
        .env("XDG_STATE_HOME", v.path("state"))
        .stdin(std::process::Stdio::null())
        .args(["--db-path", v.db().to_str().unwrap(), "--yes", "--json"]);
    for (k, val) in extra_env {
        c.env(k, val);
    }
    let o = c.args(args).output().unwrap();
    let stdout = String::from_utf8_lossy(&o.stdout);
    let doc: Value = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    let mut names: Vec<String> = doc["data"]["entries"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| e["provider"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn use_cmd(v: &Vault, cwd: &Path, args: &[&str]) -> common::Out {
    let mut c = Command::new(env!("CARGO_BIN_EXE_unv"));
    c.env_remove("UNV_PROJECT")
        .env_remove("UNV_SERVER_URL")
        .current_dir(cwd)
        .env("UNV_PASSWORD", common::PASSWORD)
        .env("HOME", v.path("home"))
        .env("XDG_DATA_HOME", v.path("data"))
        .env("XDG_CONFIG_HOME", v.path("config"))
        .env("XDG_STATE_HOME", v.path("state"))
        .stdin(std::process::Stdio::null())
        .args([
            "--db-path",
            v.db().to_str().unwrap(),
            "--yes",
            "--json",
            "use",
        ])
        .args(args);
    let o = c.output().unwrap();
    common::Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn seeded(tag: &str) -> (Vault, std::path::PathBuf) {
    let v = Vault::new(tag);
    v.ok(&["project", "add", "Web"]);
    v.ok(&["project", "add", "Api"]);
    v.ok(&[
        "entry",
        "add",
        "WebStaging",
        "--key",
        "web-staging-secret-1",
        "--projects",
        "web",
        "--env",
        "staging",
    ]);
    v.ok(&[
        "entry",
        "add",
        "WebProd",
        "--key",
        "web-prod-secret-22",
        "--projects",
        "web",
        "--env",
        "production",
    ]);
    v.ok(&[
        "entry",
        "add",
        "ApiStaging",
        "--key",
        "api-staging-secret-1",
        "--projects",
        "api",
        "--env",
        "staging",
    ]);
    let work = v.path("work");
    std::fs::create_dir_all(&work).unwrap();
    (v, work)
}

#[test]
fn use_pins_a_project_and_environment_that_later_commands_inherit() {
    let (v, work) = seeded("use");
    assert_eq!(use_cmd(&v, &work, &["Web", "--env", "staging"]).code, 0);
    assert_eq!(
        list_in(&v, &work, &[], &["list"]),
        vec!["WebStaging"],
        "both pins narrow the list"
    );
    // Elsewhere nothing is pinned.
    let elsewhere = v.path("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    assert_eq!(list_in(&v, &elsewhere, &[], &["list"]).len(), 3);
}

#[test]
fn the_context_file_names_things_and_never_holds_a_value() {
    let (v, work) = seeded("usefile");
    use_cmd(&v, &work, &["Web", "--env", "staging"]);
    let raw = std::fs::read_to_string(work.join(".unv.json")).unwrap();
    let doc: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(doc["project"], "Web");
    assert_eq!(doc["environment"], "staging");
    for secret in [
        "web-staging-secret-1",
        "web-prod-secret-22",
        common::PASSWORD,
    ] {
        assert!(!raw.contains(secret), "it is safe to commit: {raw}");
    }
}

#[test]
fn an_explicit_flag_beats_the_file_and_the_environment_variable_beats_it_too() {
    let (v, work) = seeded("useprecedence");
    use_cmd(&v, &work, &["Web", "--env", "staging"]);
    assert_eq!(
        list_in(&v, &work, &[], &["list", "--project", "api"]),
        vec!["ApiStaging"],
        "an explicit flag still wins"
    );
    assert_eq!(
        list_in(&v, &work, &[("UNV_PROJECT", "api")], &["list"]),
        vec!["ApiStaging"],
        "UNV_PROJECT wins over the file"
    );
    assert_eq!(
        list_in(&v, &work, &[], &["list", "--env", "production"]),
        vec!["WebProd"]
    );
}

#[test]
fn use_show_says_where_the_context_came_from_and_clear_removes_it() {
    let (v, work) = seeded("useshow");
    use_cmd(&v, &work, &["Web"]);
    let shown = use_cmd(&v, &work, &["--show"]);
    assert_eq!(shown.code, 0, "{}", shown.all());
    assert_eq!(shown.json()["data"]["project"], "Web");
    assert!(
        shown.json()["data"]["file"]
            .as_str()
            .unwrap()
            .ends_with(".unv.json"),
        "{}",
        shown.stdout
    );

    let cleared = use_cmd(&v, &work, &["--clear"]);
    assert_eq!(cleared.code, 0, "{}", cleared.all());
    assert!(!work.join(".unv.json").exists());
    assert_eq!(
        list_in(&v, &work, &[], &["list"]).len(),
        3,
        "nothing is pinned any more"
    );
}

#[test]
fn a_context_naming_a_project_that_does_not_exist_is_refused() {
    let (v, work) = seeded("usemissing");
    let o = use_cmd(&v, &work, &["NoSuchProject"]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(
        !work.join(".unv.json").exists(),
        "a refused pin must not leave a file behind"
    );
}

// ---- describe

#[test]
fn describe_is_generated_from_the_real_command_tree() {
    let v = Vault::new("describe");
    let o = v.run(&["describe"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let doc = o.json();
    // The document has the tree, and every command in it can be asked for --help.
    let root = if doc["command"].is_object() {
        &doc["command"]
    } else {
        &doc
    };
    let top: Vec<String> = root["subcommands"]
        .as_array()
        .expect("the command tree")
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    for must in [
        "entry", "project", "get", "list", "exec", "render", "backup", "user", "node", "describe",
    ] {
        assert!(
            top.contains(&must.to_string()),
            "{must} missing from {top:?}"
        );
    }
    for name in top.iter().filter(|n| *n != "help").take(40) {
        let h = v.run_text(&[name.as_str(), "--help"]);
        assert_eq!(
            h.code,
            0,
            "describe lists '{name}', so it must exist: {}",
            h.all()
        );
    }
}

#[test]
fn describe_publishes_the_exit_codes_the_cli_actually_uses() {
    let v = Vault::new("describecodes");
    let text = v.run(&["describe"]).stdout;
    for code in [
        "not_found",
        "ambiguous",
        "denied",
        "conflict",
        "unavailable",
        "needs_confirmation",
        "redacted",
        "invalid",
    ] {
        assert!(
            text.contains(code),
            "describe must name the '{code}' error: truncated"
        );
    }
}

#[test]
fn completions_print_a_script_for_the_shell_asked_for() {
    let v = Vault::new("completions");
    let bash = v.run_text(&["completions", "bash"]);
    assert_eq!(bash.code, 0, "{}", bash.all());
    assert!(
        bash.stdout.contains("unv") && bash.stdout.contains("complete"),
        "{}",
        &bash.stdout[..bash.stdout.len().min(200)]
    );
    let bad = v.run_text(&["completions", "not-a-shell"]);
    assert_eq!(bad.code, 2, "{}", bad.all());
}

#[test]
fn every_subcommand_builds_without_clap_rejecting_its_definition() {
    // clap checks the whole definition in debug builds. `completions` walks every
    // subcommand, so a flag defined twice (here: a destination's own --user beside
    // the global --user) shows up as a panic there and in that command's --help.
    let v = Vault::new("clapbuild");
    let desc = v.run(&["describe"]).json();
    let root = if desc["command"].is_object() {
        desc["command"].clone()
    } else {
        desc.clone()
    };
    fn paths(c: &Value, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
        for s in c["subcommands"].as_array().cloned().unwrap_or_default() {
            let name = s["name"].as_str().unwrap().to_string();
            if name == "help" {
                continue;
            }
            prefix.push(name);
            out.push(prefix.clone());
            paths(&s, prefix, out);
            prefix.pop();
        }
    }
    let mut all = Vec::new();
    paths(&root, &mut Vec::new(), &mut all);
    assert!(all.len() > 100, "the tree is large: {}", all.len());
    for path in all {
        let mut args: Vec<&str> = path.iter().map(String::as_str).collect();
        args.push("--help");
        let o = v.run_text(&args);
        assert_eq!(
            o.code,
            0,
            "`unv {} --help` must work: {}",
            path.join(" "),
            o.all()
        );
    }
}

#[test]
fn the_sftp_destination_takes_its_login_as_username_not_user() {
    let v = Vault::new("sftpflag");
    let help = v.run_text(&["backup", "dest", "add", "sftp", "--help"]);
    assert_eq!(help.code, 0, "{}", help.all());
    assert!(help.stdout.contains("--username"), "{}", help.stdout);
}
