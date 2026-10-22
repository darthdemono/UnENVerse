// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The plugin host against real child processes: the example plugin passes the conformance suite
//! and deliberately bad plugins fail the check that matches their fault. Needs `python3`.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use unv_app::plugin_host::{conformance, Plugin, PluginError};
use vault_core::plugin::{self, Manifest};

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/plugins/ntfy-notifier")
}

fn load(dir: &Path) -> Manifest {
    Manifest::parse(&std::fs::read_to_string(dir.join("plugin.json")).unwrap()).unwrap()
}

/// A copy of the example with one fault patched in, in its own directory.
fn bad(fault: &str, patch: impl Fn(&str) -> String) -> (PathBuf, Manifest) {
    let dir = std::env::temp_dir().join(format!("unv-bad-{fault}-{}", vault_core::new_uuid()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = std::fs::read_to_string(example().join("plugin.py")).unwrap();
    let program = dir.join("plugin.py");
    std::fs::write(&program, patch(&src)).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::copy(example().join("plugin.json"), dir.join("plugin.json")).unwrap();
    let m = load(&dir);
    (dir, m)
}

fn failed(checks: &[unv_app::plugin_host::Check]) -> Vec<&'static str> {
    checks.iter().filter(|c| !c.ok).map(|c| c.name).collect()
}

#[test]
fn the_example_plugin_passes_every_conformance_check() {
    let dir = example();
    let checks = conformance(&dir, &load(&dir));
    assert_eq!(checks.len(), 6);
    assert!(failed(&checks).is_empty(), "{checks:?}");
}

#[test]
fn a_plan_that_writes_a_file_fails_the_side_effect_check() {
    let (dir, m) = bad("plan", |s| {
        s.replace(
            "def plan(params):\n",
            "def plan(params):\n    open('marker', 'w').write('x')\n",
        )
    });
    assert_eq!(failed(&conformance(&dir, &m)), ["plan_has_no_side_effects"]);
}

#[test]
fn a_repeated_key_that_runs_twice_fails_the_idempotency_check() {
    let (dir, m) = bad("idem", |s| {
        s.replace(
            "    if key in done:\n        return done[key]\n",
            "    import time\n",
        )
        .replace(
            "    done[key] = {\"status\": \"done\"}\n    return done[key]",
            "    return {\"status\": \"done\", \"at\": time.time_ns()}",
        )
    });
    assert_eq!(failed(&conformance(&dir, &m)), ["execute_is_idempotent"]);
}

#[test]
fn a_plugin_that_ignores_shutdown_is_killed_and_fails_the_deadline_check() {
    let (dir, m) = bad("hang", |s| {
        s.replace(
            "                return\n",
            "                import time\n                while True:\n                    time.sleep(1)\n",
        )
    });
    assert_eq!(failed(&conformance(&dir, &m)), ["shutdown_within_deadline"]);
}

#[test]
fn a_plugin_that_prints_its_credential_slot_fails_the_stderr_check() {
    let (dir, m) = bad("leak", |s| {
        s.replace(
            "def execute(params):\n",
            "def execute(params):\n    sys.stderr.write('token=' + os.environ.get('UNV_SLOT_URL', '') + '\\n')\n    sys.stderr.flush()\n",
        )
    });
    assert_eq!(failed(&conformance(&dir, &m)), ["stderr_holds_no_secrets"]);
}

#[test]
fn a_program_replaced_after_install_is_refused_before_it_starts() {
    let (dir, m) = bad("swap", |s| s.to_string());
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    plugin::init_schema(&conn).unwrap();
    let (found, _) = plugin::discover(dir.parent().unwrap());
    let this = found.into_iter().find(|d| d.dir == dir).unwrap();
    let installed = plugin::install(&conn, &this).unwrap();
    std::fs::write(dir.join("plugin.py"), "#!/bin/sh\nexit 0\n").unwrap();
    let err = Plugin::start(&dir, &m, Some(&installed), &[])
        .err()
        .unwrap();
    assert!(
        matches!(&err, PluginError::Start(m) if m.contains("changed since")),
        "{err:?}"
    );
}

#[test]
fn a_call_that_gets_no_answer_kills_the_plugin_and_is_lost_not_failed() {
    let (dir, m) = bad("slow", |s| {
        s.replace(
            "def execute(params):\n",
            "def execute(params):\n    import time\n    time.sleep(30)\n",
        )
    });
    let mut p = Plugin::start(&dir, &m, None, &[]).unwrap();
    let r = p.call(
        "execute",
        serde_json::json!({"op": "probe", "idempotency_key": "k"}),
        std::time::Duration::from_millis(300),
    );
    assert!(matches!(r, Err(PluginError::Lost(_))), "{r:?}");
    let after = p.call(
        "handshake",
        serde_json::json!({}),
        std::time::Duration::from_millis(300),
    );
    assert!(matches!(after, Err(PluginError::Crashed(_))), "{after:?}");
}

#[test]
fn a_plugin_error_and_a_crash_and_a_non_json_line_are_told_apart() {
    let (dir, m) = bad("modes", |s| {
        s.replace(
            "            elif method == \"reconcile\":",
            "            elif method == \"boom\":\n                raise ValueError('it broke')\n            elif method == \"garbage\":\n                print('not json', flush=True)\n                continue\n            elif method == \"die\":\n                sys.exit(3)\n            elif method == \"reconcile\":",
        )
    });
    let t = std::time::Duration::from_secs(5);
    let mut p = Plugin::start(&dir, &m, None, &[]).unwrap();
    assert!(
        matches!(p.call("boom", serde_json::json!({}), t), Err(PluginError::Remote(m)) if m.contains("it broke"))
    );
    assert!(matches!(
        p.call("garbage", serde_json::json!({}), t),
        Err(PluginError::Protocol(_))
    ));
    let mut p = Plugin::start(&dir, &m, None, &[]).unwrap();
    assert!(
        matches!(p.call("die", serde_json::json!({}), t), Err(PluginError::Crashed(m)) if m.contains("exit 3"))
    );
}
