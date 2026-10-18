//! Commands that sit beside the main flows, each held to what its help says:
//! single-use recovery codes, `file write`, `project show`, `chunk rm`,
//! `template show`, `entry verify`, `entry timeline`, `totp uri`/`rm` and `ops`.

mod common;
use common::Vault;

const CODES: &str = "alpha-1111\nbeta-2222\ngamma-3333\ndelta-4444";

fn with_codes(tag: &str) -> Vault {
    let v = Vault::new(tag);
    v.ok(&[
        "entry",
        "add",
        "Backup",
        "--type",
        "recovery_codes",
        "--var",
        &format!("codes={CODES}"),
    ]);
    v
}

// ---- codes

#[test]
fn codes_status_counts_without_printing_any_code() {
    let v = with_codes("codesstatus");
    let o = v.run(&["codes", "status", "Backup"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let d = &o.json()["data"];
    assert_eq!(
        (
            d["total"].as_u64().unwrap(),
            d["remaining"].as_u64().unwrap(),
            d["low"].as_bool().unwrap()
        ),
        (4, 4, false),
        "{d}"
    );
    assert!(
        !o.all().contains("alpha-1111"),
        "status prints no codes: {}",
        o.all()
    );
}

#[test]
fn reading_the_next_code_does_not_consume_it_and_needs_reveal() {
    let v = with_codes("codesnext");
    let refused = v.run(&["codes", "next", "Backup"]);
    assert_eq!(refused.code, 9, "{}", refused.all());
    assert!(!refused.all().contains("alpha-1111"));
    let a = v.run_with(&["--reveal"], &["codes", "next", "Backup"]);
    let b = v.run_with(&["--reveal"], &["codes", "next", "Backup"]);
    assert_eq!(a.json()["data"]["code"], "alpha-1111", "{}", a.all());
    assert_eq!(
        b.json()["data"]["code"],
        "alpha-1111",
        "reading never consumes: {}",
        b.all()
    );
}

#[test]
fn use_marks_the_first_unused_or_the_named_code_and_the_count_drops() {
    let v = with_codes("codesuse");
    assert_eq!(v.ok(&["codes", "use", "Backup"])["remaining"], 3);
    let next = v.run_with(&["--reveal"], &["codes", "next", "Backup"]);
    assert_eq!(
        next.json()["data"]["code"],
        "beta-2222",
        "the used one is skipped: {}",
        next.all()
    );
    assert_eq!(
        v.ok(&["codes", "use", "Backup", "delta-4444"])["remaining"],
        2
    );
    let low = v.ok(&["codes", "status", "Backup"]);
    assert_eq!(low["low"], true, "two or fewer left is low: {low}");
    let unknown = v.run(&["codes", "use", "Backup", "nope-0000"]);
    assert_ne!(unknown.code, 0, "{}", unknown.all());
    assert_eq!(
        v.ok(&["codes", "status", "Backup"])["remaining"],
        2,
        "a refused use changes nothing"
    );
}

#[test]
fn when_every_code_is_used_next_is_not_found_and_other_entry_types_are_refused() {
    let v = with_codes("codesout");
    for _ in 0..4 {
        v.ok(&["codes", "use", "Backup"]);
    }
    assert_eq!(
        v.run_with(&["--reveal"], &["codes", "next", "Backup"]).code,
        3
    );
    assert_ne!(
        v.run(&["codes", "use", "Backup"]).code,
        0,
        "nothing left to use"
    );
    v.ok(&["entry", "add", "Plain", "--key", "plain-secret-value-123"]);
    assert_eq!(
        v.run(&["codes", "status", "Plain"]).code,
        10,
        "only recovery_codes entries"
    );
}

// ---- file write

#[test]
fn file_write_writes_the_file_owner_only_and_prints_the_env_line_not_the_contents() {
    let v = Vault::new("filewrite");
    let blob = v.path("cred.json");
    std::fs::write(&blob, "{\"private\":\"blob-contents-xyz\"}").unwrap();
    let mount = v.path("mnt/cred.json");
    v.ok(&[
        "entry",
        "add",
        "GcpKey",
        "--key",
        "placeholder-value-12345",
        "--blob-file",
        blob.to_str().unwrap(),
        "--mount-path",
        mount.to_str().unwrap(),
    ]);
    let o = v.run(&["file", "write", "GcpKey"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        !o.all().contains("blob-contents-xyz"),
        "the line names the file, never its contents: {}",
        o.all()
    );
    let line = o.json()["data"]["env_line"].as_str().unwrap().to_string();
    assert!(line.ends_with(&format!("={}", mount.display())), "{line}");
    assert_eq!(
        std::fs::read_to_string(&mount).unwrap(),
        "{\"private\":\"blob-contents-xyz\"}"
    );
    #[cfg(unix)]
    assert_eq!(common::mode_of(&mount), 0o600, "the file is the credential");

    let other = v.path("elsewhere/c.json");
    assert_eq!(
        v.run(&["file", "write", "GcpKey", "--out", other.to_str().unwrap()])
            .code,
        0
    );
    assert!(
        other.exists(),
        "--out overrides the mount path and creates its directory"
    );
}

#[test]
fn file_write_refuses_an_entry_with_no_file_and_one_with_nowhere_to_go() {
    let v = Vault::new("filewritebad");
    v.ok(&["entry", "add", "Plain", "--key", "plain-secret-value-123"]);
    assert_eq!(
        v.run(&["file", "write", "Plain"]).code,
        10,
        "no file contents"
    );
    let blob = v.path("c.txt");
    std::fs::write(&blob, "contents").unwrap();
    v.ok(&[
        "entry",
        "add",
        "NoMount",
        "--key",
        "placeholder-value-12345",
        "--blob-file",
        blob.to_str().unwrap(),
    ]);
    assert_eq!(
        v.run(&["file", "write", "NoMount"]).code,
        10,
        "no mount path and no --out"
    );
    assert_eq!(v.run(&["file", "write", "Nope"]).code, 3);
}

// ---- project show / chunk rm

#[test]
fn project_show_prints_the_project_with_its_chunks_and_chunk_rm_deletes_one() {
    let v = Vault::new("projshow");
    v.ok(&["project", "add", "Web"]);
    v.ok(&[
        "project", "chunk", "add", "Web", "api", "--type", "env_file",
    ]);
    v.ok(&[
        "project", "chunk", "add", "Web", "app", "--type", "env_file",
    ]);
    let shown = v.ok(&["project", "show", "Web"]);
    assert_eq!(shown["name"], "Web", "{shown}");
    assert_eq!(shown["chunks"].as_array().unwrap().len(), 2, "{shown}");

    assert_eq!(
        v.ok(&["project", "chunk", "rm", "Web", "api"])["deleted"],
        true
    );
    let left = v.ok(&["project", "chunk", "ls", "Web"]);
    assert_eq!(left["count"], 1, "{left}");
    assert_eq!(
        v.run(&["project", "chunk", "rm", "Web", "api"]).code,
        3,
        "twice is not_found"
    );
    assert_eq!(v.run(&["project", "show", "Nope"]).code, 3);
}

// ---- templates

#[test]
fn template_show_names_what_a_preset_fills_and_an_unknown_one_is_not_found() {
    let v = Vault::new("template");
    let ls = v.ok(&["template", "ls"]);
    let first = ls["templates"][0]["id"]
        .as_str()
        .expect("at least one preset")
        .to_string();
    let shown = v.run(&["template", "show", &first]);
    assert_eq!(shown.code, 0, "{}", shown.all());
    assert!(shown.all().contains(&first), "{}", shown.all());
    assert_eq!(v.run(&["template", "show", "no-such-preset"]).code, 3);
}

// ---- entry verify / timeline

#[test]
fn verify_stamps_a_session_and_timeline_shows_lifecycle_without_secrets() {
    let v = Vault::new("verify");
    v.ok(&["entry", "add", "Site", "--key", "timeline-secret-value-12"]);
    let t = v.run(&["entry", "timeline", "Site"]);
    assert_eq!(t.code, 0, "{}", t.all());
    assert!(
        !t.all().contains("timeline-secret-value-12"),
        "secret-free: {}",
        t.all()
    );
    let ok = v.run(&["entry", "verify", "Site"]);
    assert_eq!(ok.code, 0, "{}", ok.all());
    assert_eq!(v.run(&["entry", "verify", "Nope"]).code, 3);
    assert_eq!(v.run(&["entry", "timeline", "Nope"]).code, 3);
}

// ---- totp uri / rm

const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

#[test]
fn totp_uri_is_refused_to_the_terminal_and_written_owner_only_and_rm_forgets_the_seed() {
    let v = Vault::new("totpuri");
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "gh-secret-value-123456",
        "--totp",
        SEED,
    ]);
    let refused = v.run(&["totp", "uri", "GitHub"]);
    assert_eq!(refused.code, 9, "{}", refused.all());
    assert!(!refused.all().contains(SEED));
    let out = v.path("uri.txt");
    let o = v.run(&["totp", "uri", "GitHub", "--out", out.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.starts_with("otpauth://totp/") && text.contains(&format!("secret={SEED}")),
        "{text}"
    );
    #[cfg(unix)]
    assert_eq!(common::mode_of(&out), 0o600);

    assert_eq!(v.run(&["totp", "rm", "GitHub"]).code, 0);
    assert_ne!(
        v.run(&["totp", "uri", "GitHub", "--out", out.to_str().unwrap()])
            .code,
        0,
        "no seed left to write"
    );
    assert_eq!(v.run(&["totp", "rm", "Nope"]).code, 3);
}

// ---- ops

#[test]
fn ops_lists_runs_shows_one_and_cancelling_an_unknown_one_is_not_found() {
    let v = Vault::new("ops");
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
    v.ok(&["backup", "recovery-key"]);
    assert!(v.ok(&["ops", "ls"])["runs"].as_array().unwrap().is_empty());
    let ran = v.ok(&["ops", "run", "--dest", "main"]);
    let id = ran["run"]["id"].as_str().unwrap().to_string();
    assert_eq!(ran["run"]["state"], "succeeded", "{ran}");
    let ls = v.ok(&["ops", "ls"]);
    assert_eq!(ls["runs"].as_array().unwrap().len(), 1, "{ls}");
    let shown = v.ok(&["ops", "show", &id]);
    assert_eq!(shown["run"]["id"], id.as_str(), "{shown}");
    assert_eq!(v.run(&["ops", "show", "nope"]).code, 3);
    // Bug it pins: an unknown run id surfaced as the generic code 1, not not_found.
    assert_eq!(v.run(&["ops", "cancel", "nope"]).code, 3);
    let done = v.run(&["ops", "cancel", &id]);
    assert_ne!(
        done.code,
        0,
        "a finished run cannot be cancelled: {}",
        done.all()
    );
}
