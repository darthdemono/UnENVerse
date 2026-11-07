// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv guard host` (R35 slice 3): the local half of the browser paste guard.
//!
//! A browser extension cannot read the vault and must not hold a secret, so it asks this process,
//! which the browser starts itself, using the browser's native-messaging protocol over stdin and
//! stdout: each message is a 4-byte length in native byte order followed by that many bytes of
//! JSON. There is no socket and no network code; the only things that can talk to the host are the
//! browser that spawned it and, through the host manifest's allow-list, one named extension.
//!
//! A request is `{"op":"check","text":"…"}` or `{"op":"ping"}`. The answer to a check names the
//! entries found and nothing else, so a value, a hash and a fingerprint never travel back to the
//! page, and the text is matched in this process and dropped.

use crate::guard_cmd::summarise;
use serde_json::{json, Value};
use std::io::{Read, Write};

/// The most text one request may carry. A paste is a few kilobytes; this stops a page from
/// making the host buffer without limit.
pub const MAX_TEXT: usize = 1024 * 1024;
/// The browser refuses a message from a host larger than this, so answers stay far below it.
pub const MAX_FRAME: usize = MAX_TEXT + 4096;

/// Reads one length-prefixed message, or `None` at a clean end of input (the browser closed the
/// pipe). A length above `max` is an error and nothing is read.
pub fn read_frame<R: Read>(r: &mut R, max: usize) -> std::io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let n = u32::from_ne_bytes(len) as usize;
    if n > max {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("message of {n} bytes is over the {max} byte limit"),
        ));
    }
    let mut body = vec![0u8; n];
    r.read_exact(&mut body)?;
    Ok(Some(body))
}

pub fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> std::io::Result<()> {
    let n = u32::try_from(body.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "message too long"))?;
    w.write_all(&n.to_ne_bytes())?;
    w.write_all(body)?;
    w.flush()
}

/// The answer to one request. `vault` is `Err` with a short reason when the vault cannot be read
/// (locked), so the page is told "unavailable" instead of being given a false "clean".
pub fn handle(request: &[u8], vault: Result<&Value, &str>) -> Value {
    let Ok(msg) = serde_json::from_slice::<Value>(request) else {
        return json!({ "ok": false, "error": "invalid", "message": "the request is not JSON" });
    };
    match msg.get("op").and_then(Value::as_str) {
        Some("ping") => json!({ "ok": true, "host": "unv-guard/1", "unlocked": vault.is_ok() }),
        Some("check") => {
            let Some(text) = msg.get("text").and_then(Value::as_str) else {
                return json!({ "ok": false, "error": "invalid", "message": "check needs text" });
            };
            if text.len() > MAX_TEXT {
                return json!({ "ok": false, "error": "too_long", "message": "the text is too long" });
            }
            match vault {
                Ok(v) => {
                    let mut s = summarise(text, v);
                    s["ok"] = json!(true);
                    s["spans"] = json!(crate::guard_cmd::spans(text, v));
                    s
                }
                Err(why) => json!({ "ok": false, "error": "unavailable", "message": why }),
            }
        }
        _ => json!({ "ok": false, "error": "invalid", "message": "unknown op" }),
    }
}

/// Serves requests until the input closes. `vault` is asked for the current vault on every
/// request, so an entry added a minute ago is matched and a vault that locked meanwhile is
/// reported as such.
pub fn serve<R: Read, W: Write>(
    r: &mut R,
    w: &mut W,
    mut vault: impl FnMut() -> Result<Value, String>,
) -> std::io::Result<()> {
    while let Some(req) = read_frame(r, MAX_FRAME)? {
        let current = vault();
        let answer = handle(&req, current.as_ref().map_err(String::as_str));
        write_frame(w, answer.to_string().as_bytes())?;
    }
    Ok(())
}

/// Runs the host on this process's stdin and stdout until the browser closes the pipe.
pub fn run(access: &crate::access::Access) -> crate::error::CliResult {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(&mut stdin.lock(), &mut stdout.lock(), || {
        access.load_vault().map_err(|e| e.message)
    })
    .map_err(|e| crate::error::CliError::from(e.to_string()))
}

/// The name browsers know the host by. It appears in the extension and in the host manifest.
pub const HOST_NAME: &str = "io.unenverse.guard";
/// The Firefox add-on id the host manifest allows.
pub const FIREFOX_ID: &str = "guard@unenverse.io";

/// Single-quotes a path for a POSIX shell.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The wrapper a browser starts. A browser runs the manifest's `path` with no chosen arguments,
/// so the wrapper is what says `guard host`. The browser's own arguments (the calling origin) are
/// dropped on purpose: the allow-list in the manifest already decided who may call.
pub fn wrapper_script(unv: &str) -> String {
    format!("#!/bin/sh\nexec {} guard host\n", sh_quote(unv))
}

/// Where a browser looks for host manifests, relative to the home directory, on `os`
/// (`std::env::consts::OS`). `None` where this is not supported.
pub fn manifest_dir(browser: &str, os: &str) -> Option<std::path::PathBuf> {
    let rel = match (os, browser) {
        ("linux", "chrome") => ".config/google-chrome/NativeMessagingHosts",
        ("linux", "chromium") => ".config/chromium/NativeMessagingHosts",
        ("linux", "firefox") => ".mozilla/native-messaging-hosts",
        ("macos", "chrome") => "Library/Application Support/Google/Chrome/NativeMessagingHosts",
        ("macos", "chromium") => "Library/Application Support/Chromium/NativeMessagingHosts",
        ("macos", "firefox") => "Library/Application Support/Mozilla/NativeMessagingHosts",
        _ => return None,
    };
    Some(std::path::PathBuf::from(rel))
}

/// The host manifest for `browser`. Chrome-family browsers allow one extension by origin, so they
/// need its id; Firefox allows the fixed add-on id.
pub fn host_manifest(
    browser: &str,
    wrapper: &str,
    extension_id: Option<&str>,
) -> Result<Value, String> {
    let mut m = json!({
        "name": HOST_NAME,
        "description": "UnENVerse paste guard: checks pasted text against the vault, locally",
        "path": wrapper,
        "type": "stdio",
    });
    if browser == "firefox" {
        m["allowed_extensions"] = json!([FIREFOX_ID]);
    } else {
        let id = extension_id
            .filter(|i| !i.is_empty() && i.chars().all(|c| c.is_ascii_lowercase()))
            .ok_or("Chrome and Chromium need --extension-id: the 32 lowercase letters shown on chrome://extensions")?;
        m["allowed_origins"] = json!([format!("chrome-extension://{id}/")]);
    }
    Ok(m)
}

/// `unv guard install-host`: writes the wrapper and the manifest where the browser looks.
pub fn install(browser: &str, extension_id: Option<&str>) -> crate::error::CliResult {
    use crate::error::CliError;
    let os = std::env::consts::OS;
    let rel = manifest_dir(browser, os).ok_or_else(|| {
        CliError::invalid(format!(
            "installing the host for {browser} is not supported on {os}"
        ))
    })?;
    let home = dirs::home_dir().ok_or_else(|| CliError::from("Cannot find the home directory"))?;
    // nosemgrep: rust.lang.security.current-exe.current-exe (writes the path of the program the user is running into the user's own wrapper script; nothing is trusted or compared)
    let unv = std::env::current_exe()
        .map_err(|e| CliError::from(format!("Cannot find this program: {e}")))?;
    let data = dirs::data_dir()
        .ok_or_else(|| CliError::from("Cannot find the data directory"))?
        .join("io.unenverse");
    let wrapper = data.join("guard-host.sh");
    let manifest = host_manifest(browser, &wrapper.display().to_string(), extension_id)
        .map_err(CliError::invalid)?;
    let dir = home.join(rel);
    if crate::out::dry_run() {
        crate::out::ok(
            "guard.install_host",
            json!({ "dry_run": true, "wrapper": wrapper.display().to_string(), "manifest": dir.join(format!("{HOST_NAME}.json")).display().to_string() }),
            || {
                println!(
                    "would write {} and the manifest in {}",
                    wrapper.display(),
                    dir.display()
                )
            },
        );
        return Ok(());
    }
    std::fs::create_dir_all(&data).map_err(|e| CliError::from(e.to_string()))?;
    std::fs::write(&wrapper, wrapper_script(&unv.display().to_string()))
        .map_err(|e| CliError::from(format!("Cannot write {}: {e}", wrapper.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| CliError::from(e.to_string()))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| CliError::from(e.to_string()))?;
    let path = dir.join(format!("{HOST_NAME}.json"));
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    )
    .map_err(|e| CliError::from(format!("Cannot write {}: {e}", path.display())))?;
    crate::out::ok(
        "guard.install_host",
        json!({ "wrapper": wrapper.display().to_string(), "manifest": path.display().to_string() }),
        || {
            println!(
                "Installed {}. Restart the browser, then use the extension.",
                path.display()
            )
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn vault() -> Value {
        json!({ "api_keys": [
            { "id": "1", "provider": "GitHub", "api_key": "plain-token-with-no-known-prefix-9" }
        ] })
    }

    fn frame(v: &Value) -> Vec<u8> {
        let b = v.to_string().into_bytes();
        let mut out = (b.len() as u32).to_ne_bytes().to_vec();
        out.extend(b);
        out
    }

    fn answers(out: &[u8]) -> Vec<Value> {
        let mut c = Cursor::new(out.to_vec());
        let mut v = Vec::new();
        while let Some(b) = read_frame(&mut c, MAX_FRAME).unwrap() {
            v.push(serde_json::from_slice(&b).unwrap());
        }
        v
    }

    #[test]
    fn a_check_names_the_entry_and_never_the_value() {
        let mut input =
            frame(&json!({ "op": "check", "text": "x plain-token-with-no-known-prefix-9 y" }));
        input.extend(frame(&json!({ "op": "check", "text": "nothing here" })));
        let mut out = Vec::new();
        serve(&mut Cursor::new(input), &mut out, || Ok(vault())).unwrap();
        let a = answers(&out);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0]["ok"], true);
        assert_eq!(a[0]["blocking"], 1);
        assert_eq!(a[0]["hits"][0]["entry"], "GitHub");
        assert_eq!(a[0]["spans"][0]["start"], 2);
        assert_eq!(a[0]["spans"][0]["ref"], "${GitHub}");
        assert!(!String::from_utf8_lossy(&out).contains("plain-token-with-no-known-prefix-9"));
        assert_eq!(a[1]["clean"], true);
    }

    #[test]
    fn a_locked_vault_is_unavailable_never_clean() {
        let mut out = Vec::new();
        serve(
            &mut Cursor::new(frame(&json!({ "op": "check", "text": "anything" }))),
            &mut out,
            || Err("locked".into()),
        )
        .unwrap();
        let a = answers(&out);
        assert_eq!(a[0]["ok"], false);
        assert_eq!(a[0]["error"], "unavailable");
        assert!(a[0].get("clean").is_none());
    }

    #[test]
    fn ping_reports_whether_the_vault_is_readable() {
        let v = vault();
        assert_eq!(handle(br#"{"op":"ping"}"#, Ok(&v))["unlocked"], true);
        assert_eq!(
            handle(br#"{"op":"ping"}"#, Err("locked"))["unlocked"],
            false
        );
    }

    #[test]
    fn bad_requests_get_a_clear_refusal() {
        let v = vault();
        for bad in [
            &b"not json"[..],
            br#"{"op":"dump"}"#,
            br#"{"op":"check"}"#,
            br#"[]"#,
        ] {
            let r = handle(bad, Ok(&v));
            assert_eq!(r["ok"], false, "{}", String::from_utf8_lossy(bad));
            assert_eq!(r["error"], "invalid");
        }
        let long = json!({ "op": "check", "text": "a".repeat(MAX_TEXT + 1) }).to_string();
        assert_eq!(handle(long.as_bytes(), Ok(&v))["error"], "too_long");
    }

    #[test]
    fn frames_over_the_limit_are_refused_without_reading_the_body_and_eof_is_clean() {
        let mut huge = ((MAX_FRAME + 1) as u32).to_ne_bytes().to_vec();
        huge.push(b'x');
        assert!(read_frame(&mut Cursor::new(huge), MAX_FRAME).is_err());
        assert!(read_frame(&mut Cursor::new(Vec::new()), MAX_FRAME)
            .unwrap()
            .is_none());
        // A short body behind an honest length is an error, not a partial message.
        let mut short = 10u32.to_ne_bytes().to_vec();
        short.extend_from_slice(b"abc");
        assert!(read_frame(&mut Cursor::new(short), MAX_FRAME).is_err());
    }

    #[test]
    fn a_vault_added_between_requests_is_matched_on_the_next_one() {
        let mut input = frame(&json!({ "op": "check", "text": "added-later-value-12345" }));
        input.extend(frame(
            &json!({ "op": "check", "text": "added-later-value-12345" }),
        ));
        let mut calls = 0;
        let mut out = Vec::new();
        serve(&mut Cursor::new(input), &mut out, || {
            calls += 1;
            Ok(if calls == 1 {
                json!({ "api_keys": [] })
            } else {
                json!({ "api_keys": [{ "id": "2", "provider": "New", "api_key": "added-later-value-12345" }] })
            })
        })
        .unwrap();
        let a = answers(&out);
        assert_eq!(a[0]["clean"], true);
        assert_eq!(a[1]["blocking"], 1);
    }

    #[test]
    fn the_wrapper_names_guard_host_and_quotes_the_path() {
        assert_eq!(
            wrapper_script("/opt/my tools/unv"),
            "#!/bin/sh\nexec '/opt/my tools/unv' guard host\n"
        );
        assert_eq!(
            wrapper_script("/it's/unv"),
            "#!/bin/sh\nexec '/it'\\''s/unv' guard host\n"
        );
    }

    #[test]
    fn each_browser_has_its_own_manifest_folder_and_unknown_ones_have_none() {
        assert!(manifest_dir("firefox", "linux")
            .unwrap()
            .starts_with(".mozilla"));
        assert!(manifest_dir("chrome", "macos")
            .unwrap()
            .starts_with("Library"));
        assert!(manifest_dir("chrome", "windows").is_none());
        assert!(manifest_dir("opera", "linux").is_none());
    }

    #[test]
    fn firefox_allows_the_fixed_add_on_and_chrome_needs_an_id() {
        let ff = host_manifest("firefox", "/w", None).unwrap();
        assert_eq!(ff["allowed_extensions"][0], FIREFOX_ID);
        assert!(ff.get("allowed_origins").is_none());
        assert!(host_manifest("chrome", "/w", None).is_err());
        assert!(host_manifest("chrome", "/w", Some("NOT-an-id")).is_err());
        let ch = host_manifest("chrome", "/w", Some("abcdefghijklmnopabcdefghijklmnop")).unwrap();
        assert_eq!(
            ch["allowed_origins"][0],
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/"
        );
        assert_eq!(ch["type"], "stdio");
        assert_eq!(ch["name"], HOST_NAME);
    }
}
