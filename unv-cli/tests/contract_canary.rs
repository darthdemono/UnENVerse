// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Canary entries (R33): `plant` makes a decoy entry and file, a decoy is never released to a
//! command or exported, a second plant never replaces a real file, and a hit on the collector is
//! on the audit chain. A canary detects theft after the fact; it prevents nothing.

mod common;
use common::Vault;

fn planted(tag: &str) -> (Vault, std::path::PathBuf, String) {
    let v = Vault::new(tag);
    let dir = v.db().with_file_name("decoy-dir");
    std::fs::create_dir_all(&dir).unwrap();
    let o = v.run(&[
        "canary",
        "plant",
        dir.to_str().unwrap(),
        "--label",
        "svc",
        "--collector",
        "http://127.0.0.1:9",
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    let text = std::fs::read_to_string(dir.join(".env.production")).unwrap();
    let token = text
        .lines()
        .find_map(|l| l.strip_prefix("API_TOKEN="))
        .unwrap()
        .to_string();
    (v, dir, token)
}

#[test]
fn plant_writes_a_decoy_file_and_a_flagged_entry() {
    let (v, dir, token) = planted("canary-plant");
    let text = std::fs::read_to_string(dir.join(".env.production")).unwrap();
    assert!(
        text.contains(&format!("API_BASE_URL=http://127.0.0.1:9/c/{token}")),
        "{text}"
    );
    let ls = v.run(&["canary", "ls"]);
    assert_eq!(ls.code, 0, "{}", ls.all());
    assert!(ls.all().contains("Canary svc"), "{}", ls.all());
    // The listing names the decoys and never prints the token.
    assert!(!ls.all().contains(&token), "{}", ls.all());
}

#[test]
fn a_second_plant_never_replaces_a_file_that_is_already_there() {
    let (v, dir, _) = planted("canary-twice");
    let before = std::fs::read_to_string(dir.join(".env.production")).unwrap();
    let o = v.run(&["canary", "plant", dir.to_str().unwrap()]);
    assert_eq!(o.code, 6, "{}", o.all());
    assert_eq!(
        std::fs::read_to_string(dir.join(".env.production")).unwrap(),
        before
    );
}

#[test]
fn a_decoy_is_never_released_to_a_command_or_exported() {
    let (v, dir, token) = planted("canary-excluded");
    let name = {
        let ls = v.run(&["canary", "ls"]);
        ls.json()["data"]["canaries"][0]
            .as_str()
            .unwrap()
            .to_string()
    };
    let o = v.run(&[
        "exec",
        "--entry",
        &format!("{name}=T"),
        "--",
        "printenv",
        "T",
    ]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert!(!o.all().contains(&token), "{}", o.all());
    let out = dir.join("export.env");
    v.ok(&["entry", "add", "Real", "--key", "real-value-1234567890"]);
    let e = v.run(&["export", "--out", out.to_str().unwrap()]);
    assert_eq!(e.code, 0, "{}", e.all());
    let exported = std::fs::read_to_string(&out).unwrap();
    assert!(exported.contains("real-value-1234567890"), "{exported}");
    assert!(
        !exported.contains(&token),
        "a decoy must not be exported: {exported}"
    );
}

#[cfg(unix)]
#[test]
fn a_hit_on_the_collector_is_recorded_on_the_audit_chain() {
    let (v, _dir, token) = planted("canary-hit");
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let script = format!(
        r#"TOKEN={token}; PORT={port}
"$UNV" --db-path "$VAULT" canary listen --addr 127.0.0.1:$PORT >/dev/null 2>&1 &
P=$!
i=0
until curl -sf "http://127.0.0.1:$PORT/c/$TOKEN" >/dev/null; do
  i=$((i+1)); [ $i -gt 40 ] && break; sleep 0.25
done
curl -s -o /dev/null -w '%{{http_code}}' "http://127.0.0.1:$PORT/c/not-a-token" > "$VAULT.unknown"
kill $P 2>/dev/null; wait $P 2>/dev/null; true"#
    );
    let o = v.sh(&script);
    assert_eq!(o.code, 0, "{}", o.all());
    let unknown =
        std::fs::read_to_string(format!("{}.unknown", v.db().display())).unwrap_or_default();
    assert_eq!(
        unknown, "404",
        "an unknown token is a 404 and leaves no row"
    );
    let audit = v.run(&["audit"]);
    assert!(audit.all().contains("canary.hit"), "{}", audit.all());
    assert!(audit.all().contains("Canary svc"), "{}", audit.all());
    assert!(!audit.all().contains(&token), "{}", audit.all());
}
