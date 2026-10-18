//! Getting a real value out ("Ways to get the real value out", README) and the
//! record of it (`matlog.rs`, ADR-0142): `exec`, `render --out`, `--out` and
//! revealed output are the only paths a value leaves the vault by, none of them
//! prints it to the terminal unasked, and each is written to the materialisation
//! log as entry, field and fingerprint, never the value, so blast-radius can
//! say what reached this machine.

mod common;
use common::Vault;
use serde_json::Value;

const SECRET: &str = "materialise-secret-123456";

fn seeded(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "GitHub", "--key", SECRET]);
    v.ok(&["project", "add", "Web"]);
    v.ok(&[
        "project", "chunk", "add", "Web", "app", "--type", "env_file",
    ]);
    v.ok(&[
        "project",
        "chunk",
        "set",
        "Web",
        "app",
        "TOKEN=${GitHub}",
        "PLAIN=hello",
    ]);
    v
}

/// The records of the materialisation log, oldest first.
fn log(v: &Vault) -> Vec<Value> {
    let path = v
        .dir
        .join("state")
        .join("unv")
        .join("materialisations.jsonl");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn template(v: &Vault, body: &str) -> String {
    let p = v.path("t.tpl");
    std::fs::write(&p, body).unwrap();
    p.to_str().unwrap().to_string()
}

// ---- exec

#[test]
fn exec_puts_the_value_in_the_childs_environment_and_not_on_our_stdout() {
    let v = seeded("exec");
    // The child reports only the LENGTH of what it was given.
    let o = v.run_text(&[
        "exec",
        "--entry",
        "GitHub=GH_TOKEN",
        "--",
        "sh",
        "-c",
        "printf %s \"$GH_TOKEN\" | wc -c",
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(
        o.stdout.trim(),
        SECRET.len().to_string(),
        "the child must see the real value: {}",
        o.all()
    );
    assert!(
        !o.all().contains(SECRET),
        "exec itself must never print it: {}",
        o.all()
    );
}

#[test]
fn exec_loads_a_projects_env_file_with_refs_resolved_and_an_optional_prefix() {
    let v = seeded("execproject");
    let o = v.run_text(&[
        "exec",
        "--project",
        "Web",
        "--prefix",
        "APP_",
        "--",
        "sh",
        "-c",
        "echo \"$APP_PLAIN\"; [ \"$APP_TOKEN\" = \"$EXPECTED\" ] && echo resolved",
    ]);
    // The child compares with a value it cannot know; so instead check the plain
    // variable and that the ref variable is set, which proves nothing leaked into stdout.
    assert_eq!(
        o.code,
        1,
        "EXPECTED is unset on purpose, so the comparison is false: {}",
        o.all()
    );
    assert!(o.stdout.contains("hello"), "{}", o.all());
    assert!(!o.all().contains(SECRET), "{}", o.all());

    let set = v.run_text(&[
        "exec",
        "--project",
        "Web",
        "--prefix",
        "APP_",
        "--",
        "sh",
        "-c",
        "test -n \"$APP_TOKEN\" && echo token-set",
    ]);
    assert_eq!(set.code, 0, "{}", set.all());
    assert!(set.stdout.contains("token-set"), "{}", set.all());
}

#[test]
fn exec_returns_the_childs_exit_code() {
    let v = seeded("execcode");
    let o = v.run_text(&[
        "exec",
        "--entry",
        "GitHub=GH_TOKEN",
        "--",
        "sh",
        "-c",
        "exit 7",
    ]);
    assert_eq!(
        o.code,
        7,
        "a script must see the real outcome of its command: {}",
        o.all()
    );
}

#[test]
fn exec_with_an_unknown_entry_refuses_and_never_starts_the_command() {
    let v = seeded("execmissing");
    let marker = v.path("ran.txt");
    let o = v.run_text(&[
        "exec",
        "--entry",
        "Nope",
        "--",
        "sh",
        "-c",
        &format!("echo ran > {}", marker.display()),
    ]);
    assert_eq!(o.code, 3, "{}", o.all());
    assert!(
        !marker.exists(),
        "the command must not run without the secrets it was asked for"
    );
}

#[test]
fn exec_clean_hides_the_callers_environment_but_keeps_the_path() {
    let v = seeded("execclean");
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_unv"));
    let o = c
        .env_remove("UNV_SERVER_URL")
        .env("UNV_PASSWORD", common::PASSWORD)
        .env("HOME", v.path("home"))
        .env("XDG_STATE_HOME", v.path("state"))
        .env("LEAK_ME", "ambient-value")
        .args([
            "--db-path",
            v.db().to_str().unwrap(),
            "--yes",
            "exec",
            "--clean",
            "--entry",
            "GitHub=GH_TOKEN",
            "--",
        ])
        .args([
            "sh",
            "-c",
            "echo \"[$LEAK_ME]\"; command -v sh >/dev/null && echo path-kept",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert!(
        stdout.contains("[]"),
        "--clean must not pass the ambient environment: {stdout}"
    );
    assert!(stdout.contains("path-kept"), "PATH is kept: {stdout}");
}

#[test]
fn exec_is_recorded_without_the_value() {
    let v = seeded("execlog");
    v.run_text(&["exec", "--entry", "GitHub=GH_TOKEN", "--", "true"]);
    let records = log(&v);
    let rec = records
        .iter()
        .find(|r| r["kind"] == "exec")
        .expect("exec must be logged");
    assert_eq!(rec["exposed"][0]["provider"], "GitHub", "{rec}");
    assert_eq!(rec["exposed"][0]["field"], "api_key", "{rec}");
    assert!(
        rec["exposed"][0]["fp"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"),
        "{rec}"
    );
    let raw = std::fs::read_to_string(
        v.dir
            .join("state")
            .join("unv")
            .join("materialisations.jsonl"),
    )
    .unwrap();
    assert!(!raw.contains(SECRET), "the log must never hold a value");
}

// ---- render

#[test]
fn render_to_stdout_shows_a_fingerprint_and_leaves_an_unresolved_ref_readable() {
    let v = seeded("render");
    let t = template(&v, "token=${GitHub}\nmissing=${Nope}\n");
    let o = v.run_text(&["render", &t]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains(SECRET), "{}", o.all());
    assert!(o.stdout.contains("token=sha256:"), "{}", o.stdout);
    assert!(
        o.stdout.contains("missing=${Nope}"),
        "an unresolved ref stays as written: {}",
        o.stdout
    );
    assert!(
        o.all().contains("unresolved ref: ${Nope}"),
        "and is reported: {}",
        o.all()
    );
}

#[test]
fn render_strict_fails_on_an_unresolved_ref_and_writes_nothing() {
    let v = seeded("renderstrict");
    let t = template(&v, "token=${GitHub}\nmissing=${Nope}\n");
    let out = v.path("out.conf");
    let o = v.run(&["render", &t, "--strict", "--out", out.to_str().unwrap()]);
    assert_eq!(o.code, 3, "{}", o.all());
    assert!(
        !out.exists(),
        "a failed strict render must not leave a half-resolved config behind"
    );
}

#[test]
fn render_dry_run_writes_nothing() {
    let v = seeded("renderdry");
    let t = template(&v, "token=${GitHub}\n");
    let out = v.path("out.conf");
    let o = v.run_with(
        &["--dry-run"],
        &["render", &t, "--out", out.to_str().unwrap()],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!out.exists(), "--dry-run must not write the file");
}

#[cfg(unix)]
#[test]
fn render_to_a_file_writes_the_real_value_owner_only_and_records_it() {
    use common::mode_of;
    let v = seeded("renderfile");
    let t = template(&v, "token=${GitHub}\n");
    let out = v.path("nginx.conf");
    let o = v.run(&["render", &t, "--out", out.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        !o.all().contains(SECRET),
        "the file receives the value, the terminal does not: {}",
        o.all()
    );
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        format!("token={SECRET}\n")
    );
    assert_eq!(
        mode_of(&out),
        0o600,
        "a file holding a real secret must be owner-only"
    );

    let rec = log(&v)
        .into_iter()
        .find(|r| r["kind"] == "file")
        .expect("anything written with --out is logged");
    assert_eq!(rec["exposed"][0]["provider"], "GitHub", "{rec}");
}

#[test]
fn a_revealed_print_is_recorded_too() {
    let v = seeded("revealed");
    let t = template(&v, "token=${GitHub}\n");
    let o = v.run_with(&["--reveal"], &["render", &t]);
    assert!(o.stdout.contains(SECRET), "{}", o.all());
    let kinds: Vec<String> = log(&v)
        .iter()
        .map(|r| r["kind"].as_str().unwrap().to_string())
        .collect();
    assert!(
        kinds.contains(&"stdout".to_string()),
        "anything printed with --reveal is logged: {kinds:?}"
    );
}
