//! What the README promises about secret values and the CLI ("The CLI",
//! sections 1 and 2), checked by running the real binary:
//!
//! * every stdout path masks a stored value as `sha256:` plus twelve hex digits
//!   unless `--reveal` is given;
//! * equal values have equal fingerprints, and an empty value is `empty`,
//!   never a hash;
//! * a vault-wide export to stdout is refused with exit 9, not masked;
//! * a whole `env_file` chunk is masked whatever its per-field flags say, while
//!   a `${ref}` stays readable in the structured view;
//! * the ways to get a real value out write it to a file, never to stdout, and
//!   that file is owner-only (the help text of each `--out` says 0600).

mod common;
use common::Vault;

const KEY: &str = "ghp_secretvalue_123456";

fn is_fingerprint(s: &str) -> bool {
    match s.strip_prefix("sha256:") {
        Some(hex) => {
            hex.len() == 12
                && hex
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        }
        None => false,
    }
}

#[test]
fn stored_values_are_masked_as_a_fingerprint_on_every_default_output() {
    let v = Vault::new("mask");
    let added = v.run(&["entry", "add", "GitHub", "--key", KEY]);
    assert_eq!(added.code, 0, "{}", added.all());

    for args in [
        vec!["get", "GitHub"],
        vec!["list"],
        vec!["entry", "get", "GitHub"],
        vec!["entry", "ls"],
        vec!["get", "GitHub", "--field", "api_key"],
    ] {
        let o = v.run(&args);
        assert_eq!(o.code, 0, "{args:?}: {}", o.all());
        assert!(
            !o.all().contains(KEY),
            "{args:?} printed the real value:\n{}",
            o.all()
        );
    }
    // The text view too, not only the JSON one.
    let text = v.run_text(&["get", "GitHub"]);
    assert_eq!(text.code, 0, "{}", text.all());
    assert!(!text.all().contains(KEY), "{}", text.all());

    // The documented shape: redacted, a 12-hex fingerprint, and the length.
    let data = v.ok(&["get", "GitHub"]);
    let key = &data["entries"][0]["api_key"];
    assert_eq!(key["redacted"], true);
    assert!(
        is_fingerprint(key["fingerprint"].as_str().unwrap()),
        "{key}"
    );
    assert_eq!(key["length"], KEY.len());

    // `entry add` reports a fingerprint and not the value it just stored.
    let fp = added.json()["data"]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(is_fingerprint(&fp), "{fp}");
    assert_eq!(fp, key["fingerprint"].as_str().unwrap());
}

#[test]
fn reveal_opts_back_in_and_only_then() {
    let v = Vault::new("reveal");
    v.ok(&["entry", "add", "GitHub", "--key", KEY]);
    assert_eq!(v.reveal_field("GitHub", "api_key"), KEY);
    let plain = v.run(&["get", "GitHub", "--field", "api_key"]);
    assert!(!plain.all().contains(KEY));
}

#[test]
fn equal_secrets_share_a_fingerprint_and_different_ones_do_not() {
    let v = Vault::new("fp");
    v.ok(&["entry", "add", "One", "--key", KEY]);
    v.ok(&["entry", "add", "Two", "--key", KEY]);
    v.ok(&["entry", "add", "Three", "--key", "a-different-secret-value"]);
    let fp = |name: &str| {
        v.ok(&["get", name, "--field", "api_key"])["value"]["fingerprint"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(
        fp("One"),
        fp("Two"),
        "equal values must be detectable as equal"
    );
    assert_ne!(fp("One"), fp("Three"));
}

#[test]
fn an_empty_value_is_reported_as_empty_and_never_as_a_hash() {
    let v = Vault::new("empty");
    let added = v.run(&["entry", "add", "Blank", "--key", ""]);
    assert_eq!(added.code, 0, "{}", added.all());
    assert_eq!(
        added.json()["data"]["fingerprint"],
        "empty",
        "'unset' and 'set to something' must not look alike"
    );
}

#[test]
fn a_vault_wide_text_export_to_stdout_is_refused_not_masked() {
    let v = Vault::new("export");
    v.ok(&["entry", "add", "GitHub", "--key", KEY]);
    for args in [
        vec!["export"],
        vec!["export", "--format", "dotenv"],
        vec!["export", "--format", "yaml"],
        vec!["export", "--format", "k8s"],
        vec!["export", "--format", "tfvars"],
    ] {
        let o = v.run(&args);
        assert_eq!(o.code, 9, "{args:?}: {}", o.all());
        assert_eq!(o.error_code(), "redacted");
        assert!(!o.all().contains(KEY), "{args:?}: {}", o.all());
        assert!(
            !o.stdout.contains("sha256:"),
            "a masked .env looks deployable and is not: {}",
            o.stdout
        );
    }
}

#[test]
fn the_structured_json_export_is_masked_and_never_deployable() {
    // The one stdout form that is not refused: a masked JSON document whose
    // structure is worth reading. It cannot be mistaken for a deployable file.
    let v = Vault::new("exportjson");
    v.ok(&["entry", "add", "GitHub", "--key", KEY]);
    let o = v.run(&["export", "--format", "json"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains(KEY), "{}", o.all());
    assert!(o.stdout.contains("sha256:"), "{}", o.stdout);
}

#[test]
fn a_whole_env_file_chunk_is_masked_whatever_the_field_flags_say() {
    let v = Vault::new("chunk");
    v.ok(&["project", "add", "Web"]);
    v.ok(&[
        "project", "chunk", "add", "Web", "app", "--type", "env_file",
    ]);
    // `chunk set` writes field_type `var` by default: no secret flag at all.
    v.ok(&[
        "project",
        "chunk",
        "set",
        "Web",
        "app",
        "DB_PASSWORD=hunter2-real-pass",
    ]);

    // `--format json` prints the project document itself, not the envelope.
    let out = v.run(&["project", "export", "Web", "--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let data = out.json();
    let field = &data["chunks"][0]["fields"][0];
    assert_eq!(field["field_type"], "var", "the premise: no secret flag");
    assert_eq!(field["value"]["redacted"], true, "{field}");
    assert!(!out.all().contains("hunter2-real-pass"));

    let text = v.run(&["project", "export", "Web", "--format", "env"]);
    assert_eq!(text.code, 0, "{}", text.all());
    assert!(!text.all().contains("hunter2-real-pass"), "{}", text.all());
}

#[test]
fn a_reference_stays_readable_in_the_structured_view() {
    let v = Vault::new("ref");
    v.ok(&["entry", "add", "GitHub", "--key", KEY]);
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
        "TOKEN=${GitHub/api_key}",
    ]);

    let out = v.run(&["project", "export", "Web", "--format", "json"]);
    assert_eq!(out.code, 0, "{}", out.all());
    let data = out.json();
    let value = &data["chunks"][0]["fields"][0]["value"];
    assert_eq!(
        value, "${GitHub/api_key}",
        "a ${{ref}} is a pointer, not a secret: an agent must be able to read it"
    );
    assert!(!out.all().contains(KEY));
}

#[cfg(unix)]
mod files {
    use super::*;
    use common::mode_of;
    use std::os::unix::fs::PermissionsExt;

    /// Each `--out` below is documented as written 0600 and holds a real value.
    fn cases(v: &Vault) -> Vec<(&'static str, std::path::PathBuf, Vec<String>)> {
        let p = |n: &str| v.path(n);
        let s = |p: &std::path::Path| p.to_str().unwrap().to_string();
        vec![
            (
                "get --profile basic --out",
                p("profile.env"),
                vec![
                    "get".into(),
                    "GitHub".into(),
                    "--profile".into(),
                    "basic".into(),
                    "--out".into(),
                    s(&p("profile.env")),
                ],
            ),
            (
                "curl --out",
                p("curl.txt"),
                vec![
                    "curl".into(),
                    "GitHub".into(),
                    "--out".into(),
                    s(&p("curl.txt")),
                    "--".into(),
                    "https://api.example.test/user".into(),
                ],
            ),
            (
                "export --out",
                p("export.env"),
                vec!["export".into(), "--out".into(), s(&p("export.env"))],
            ),
        ]
    }

    #[test]
    fn a_file_that_holds_a_real_secret_is_written_owner_only() {
        let v = Vault::new("modes");
        v.ok(&["entry", "add", "GitHub", "--key", KEY]);
        for (name, path, args) in cases(&v) {
            let argv: Vec<&str> = args.iter().map(String::as_str).collect();
            let o = v.run_text(&argv);
            assert_eq!(o.code, 0, "{name}: {}", o.all());
            assert!(
                !o.all().contains(KEY),
                "{name} leaked to the terminal: {}",
                o.all()
            );
            let body = std::fs::read_to_string(&path).unwrap();
            assert!(
                body.contains(KEY),
                "{name}: the file must hold the real value"
            );
            assert_eq!(
                mode_of(&path),
                0o600,
                "{name}: a file with a real secret must be owner-only"
            );
        }
    }

    #[test]
    fn overwriting_a_world_readable_file_tightens_it() {
        let v = Vault::new("tighten");
        v.ok(&["entry", "add", "GitHub", "--key", KEY]);
        let target = v.path("loose.env");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        let o = v.run_text(&["export", "--out", target.to_str().unwrap()]);
        assert_eq!(o.code, 0, "{}", o.all());
        assert!(std::fs::read_to_string(&target).unwrap().contains(KEY));
        assert_eq!(mode_of(&target), 0o600);
    }
}
