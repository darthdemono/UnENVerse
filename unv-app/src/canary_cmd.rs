// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Canary entries (R33): decoy credentials planted where a thief looks first, and an alert when one
//! is used. It detects theft after the fact; it prevents nothing.
//!
//! `unv canary plant` makes an entry flagged `canary` whose value is a random token, and writes a
//! decoy `.env.production` whose `API_BASE_URL` points at a collector you run and whose
//! `API_TOKEN` is that token. A script that steals the file and calls the URL reaches
//! `unv canary listen`, which records `canary.hit` on the vault's audit chain with the caller's
//! address. Both ends are plain: no third-party service, nothing sent anywhere you did not name.
//!
//! A decoy is never a real credential: `unv exec` refuses a canary entry and the whole-vault
//! export leaves it out. The honest limit is in the name: a thief who never calls the URL, or who
//! reads the file offline, trips nothing.

use crate::access::Access;
use crate::error::{CliError, CliResult, Code};
use crate::out;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Whether an entry is a decoy.
pub fn is_canary(entry: &Value) -> bool {
    entry.get("canary").and_then(Value::as_bool) == Some(true)
}

/// Token to entry name for every canary entry.
pub fn tokens(vault: &Value) -> HashMap<String, String> {
    crate::data::entries(vault)
        .iter()
        .filter(|e| is_canary(e))
        .filter_map(|e| {
            let t = e.get("api_key").and_then(Value::as_str)?;
            (!t.is_empty()).then(|| (t.to_string(), crate::data::provider_of(e).to_string()))
        })
        .collect()
}

/// The token in the request line of an HTTP request for `/c/<token>`, if there is one.
pub fn token_in_request(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let line = text.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let rest = path.strip_prefix("/c/")?;
    let token: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    (!token.is_empty()).then_some(token)
}

/// The decoy file's text. Only the token and the collector URL appear; neither is a real secret.
pub fn decoy_env(collector: &str, token: &str) -> String {
    format!(
        "# generated config, do not edit\nAPI_BASE_URL={}/c/{token}\nAPI_TOKEN={token}\n",
        collector.trim_end_matches('/')
    )
}

pub fn plant(
    access: &Access,
    dir: Option<&std::path::Path>,
    label: &str,
    collector: &str,
) -> CliResult {
    let dir = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|e| CliError::from(format!("Cannot read the working directory: {e}")))?,
    };
    let file = dir.join(".env.production");
    if file.exists() {
        return Err(CliError::new(
            Code::Conflict,
            format!(
                "{} already exists; a decoy never replaces a real file",
                file.display()
            ),
        ));
    }
    let token = uuid::Uuid::new_v4().simple().to_string();
    let provider = format!("Canary {label} {}", &token[..8]);
    if out::dry_run() {
        out::ok(
            "canary.plant",
            json!({ "dry_run": true, "entry": provider, "file": file.display().to_string() }),
            || println!("would plant {provider} in {}", file.display()),
        );
        return Ok(());
    }
    let mut vault = access.load_vault_or_empty()?;
    crate::data::entries_mut(&mut vault).push(json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "provider": provider,
        "api_key": token,
        "secretType": "env_var",
        "canary": true,
        "price_type": "local",
        "categories": [],
        "projectIds": ["Universal"],
        "scopes": [],
        "created_at": vault_core::iso_now(),
    }));
    access.save(&vault)?;
    crate::envfile::write_private(&file, &decoy_env(collector, &token))?;
    out::ok(
        "canary.plant",
        json!({ "entry": provider, "file": file.display().to_string() }),
        || {
            println!(
                "Planted {provider} in {}. Run `unv canary listen` where {collector} points.",
                file.display()
            )
        },
    );
    Ok(())
}

pub fn ls(access: &Access) -> CliResult {
    let vault = access.load_vault()?;
    let mut names: Vec<String> = tokens(&vault).into_values().collect();
    names.sort();
    out::ok("canary.ls", json!({ "canaries": names }), || {
        if names.is_empty() {
            println!("No canary entries.");
        }
        for n in &names {
            println!("{n}");
        }
    });
    Ok(())
}

#[cfg(not(unix))]
pub fn listen(_access: &Access, _addr: &str) -> CliResult {
    Err(CliError::invalid(
        "the canary listener is not implemented on this platform yet",
    ))
}

/// Serves `GET /c/<token>` and writes one `canary.hit` audit row per token every ten seconds at
/// most. Runs until stopped. Unknown tokens get a 404 and leave no row.
#[cfg(unix)]
pub fn listen(access: &Access, addr: &str) -> CliResult {
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    let Access::Local(key) = access else {
        return Err(CliError::invalid(
            "the canary listener writes to the local vault's audit chain; it cannot run in remote mode",
        ));
    };
    let key = *key;
    let db = crate::access::default_db_path();
    let listener = std::net::TcpListener::bind(addr)
        .map_err(|e| CliError::from(format!("Cannot listen on {addr}: {e}")))?;
    eprintln!("Listening on {addr} for canary hits (Ctrl-C to stop)");
    let last: Arc<Mutex<HashMap<String, Instant>>> = Arc::default();
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let (db, last) = (db.clone(), Arc::clone(&last));
        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let peer = stream
                .peer_addr()
                .map_or_else(|_| "unknown".to_string(), |p| p.ip().to_string());
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let known = token_in_request(&buf[..n]).and_then(|t| {
                let conn = vault_core::open_db(&db, &key).ok()?;
                let vault = vault_core::load_vault(&conn).ok()??;
                let name = tokens(&vault).get(&t).cloned()?;
                Some((t, name, conn))
            });
            let reply = match known {
                Some((token, name, conn)) => {
                    let fresh = {
                        let mut seen = last.lock().unwrap_or_else(|e| e.into_inner());
                        let due = seen
                            .get(&token)
                            .is_none_or(|t| t.elapsed() >= Duration::from_secs(10));
                        if due {
                            seen.insert(token, Instant::now());
                        }
                        due
                    };
                    if fresh {
                        let _ = vault_core::record_event(
                            &conn,
                            "canary.hit",
                            &name,
                            Some(&format!("from {peer}")),
                            None,
                        );
                    }
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                }
                None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            };
            let _ = stream.write_all(reply.as_bytes());
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_is_read_from_the_request_line_only() {
        assert_eq!(
            token_in_request(b"GET /c/abc123def?x=1 HTTP/1.1\r\nHost: h\r\n\r\n").as_deref(),
            Some("abc123def")
        );
        assert_eq!(
            token_in_request(b"POST /c/tok HTTP/1.1\r\n").as_deref(),
            Some("tok")
        );
        for bad in [
            &b""[..],
            b"GET / HTTP/1.1",
            b"GET /c/ HTTP/1.1",
            b"GET /d/tok HTTP/1.1",
            b"\xff\xfe",
        ] {
            assert!(token_in_request(bad).is_none(), "{bad:?}");
        }
        // A token in a header or the body is not a hit.
        assert!(token_in_request(b"GET /x HTTP/1.1\r\nX: /c/tok\r\n").is_none());
    }

    #[test]
    fn only_flagged_entries_with_a_value_are_tokens() {
        let v = json!({ "api_keys": [
            { "provider": "Canary a", "api_key": "tok1", "canary": true },
            { "provider": "Real", "api_key": "tok2" },
            { "provider": "Canary empty", "api_key": "", "canary": true },
            { "provider": "Flag off", "api_key": "tok3", "canary": false }
        ]});
        let t = tokens(&v);
        assert_eq!(t.len(), 1);
        assert_eq!(t["tok1"], "Canary a");
        assert!(is_canary(&v["api_keys"][0]) && !is_canary(&v["api_keys"][3]));
    }

    #[test]
    fn the_decoy_file_holds_the_token_and_a_url_and_nothing_else() {
        let t = decoy_env("http://127.0.0.1:8789/", "abc123");
        assert_eq!(
            t,
            "# generated config, do not edit\nAPI_BASE_URL=http://127.0.0.1:8789/c/abc123\nAPI_TOKEN=abc123\n"
        );
    }
}
