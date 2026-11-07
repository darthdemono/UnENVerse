// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv guard host` (R35 slice 3): the real binary speaks the browser's native-messaging framing on
//! stdin and stdout. Lengths below 128 keep every frame byte ASCII, so the harness (which carries
//! text) can exchange them; the matching logic itself is covered by the unit tests in
//! `guard_host.rs`.

mod common;
use common::Vault;
use serde_json::Value;

fn frame(json: &str) -> String {
    let n = json.len();
    assert!(n < 128, "keep frames ASCII for the text harness");
    let mut s = String::new();
    s.push(char::from(n as u8));
    s.push_str("\0\0\0");
    s.push_str(json);
    s
}

/// Splits stdout into the JSON bodies of its frames.
fn bodies(out: &str) -> Vec<Value> {
    let b = out.as_bytes();
    let mut v = Vec::new();
    let mut i = 0;
    while i + 4 <= b.len() {
        let n = u32::from_ne_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
        i += 4;
        v.push(serde_json::from_slice(&b[i..i + n]).expect("a JSON body"));
        i += n;
    }
    v
}

#[test]
fn the_host_answers_ping_and_check_in_order_and_exits_when_the_input_closes() {
    let v = Vault::new("guard-host");
    v.ok(&[
        "entry",
        "add",
        "Billing",
        "--key",
        "host-test-value-with-no-prefix-3377",
    ]);
    let input = format!(
        "{}{}",
        frame(r#"{"op":"ping"}"#),
        frame(r#"{"op":"check","text":"nothing here"}"#)
    );
    let o = v.run_stdin(&["guard", "host"], &input);
    assert_eq!(o.code, 0, "{}", o.all());
    let a = bodies(&o.stdout);
    assert_eq!(a.len(), 2, "{:?}", o.stdout);
    assert_eq!(a[0]["host"], "unv-guard/1");
    assert_eq!(a[0]["unlocked"], true);
    assert_eq!(a[1]["ok"], true);
    assert_eq!(a[1]["clean"], true);
}

#[test]
fn the_host_never_echoes_the_text_it_was_sent() {
    let v = Vault::new("guard-host-echo");
    v.ok(&[
        "entry",
        "add",
        "Billing",
        "--key",
        "host-test-value-with-no-prefix-3377",
    ]);
    let o = v.run_stdin(
        &["guard", "host"],
        &frame(r#"{"op":"check","text":"a harmless sentence"}"#),
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!o.stdout.contains("harmless sentence"), "{:?}", o.stdout);
}

#[cfg(unix)]
#[test]
fn install_host_for_chrome_refuses_without_an_extension_id_and_writes_nothing() {
    let v = Vault::new("guard-install");
    let o = v.run(&["guard", "install-host", "--browser", "chrome"]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert!(o.all().contains("extension-id"), "{}", o.all());
}

#[cfg(unix)]
#[test]
fn a_dry_run_of_install_host_names_the_files_and_creates_none() {
    let v = Vault::new("guard-install-dry");
    let home = v.db().with_file_name("fake-home");
    let o = v.run_with(
        &["--dry-run"],
        &["guard", "install-host", "--browser", "firefox"],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(o.all().contains("guard-host.sh"), "{}", o.all());
    assert!(!home.exists());
}

#[cfg(windows)]
#[test]
fn install_host_says_windows_is_not_supported_instead_of_writing_anything() {
    let v = Vault::new("guard-install-windows");
    let o = v.run(&["guard", "install-host", "--browser", "firefox"]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert!(o.all().contains("not supported on windows"), "{}", o.all());
}
