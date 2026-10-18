// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! A mock-runtime desktop for the IPC tests.
#![allow(dead_code)]

use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{ipc::CallbackFn, ipc::InvokeBody, App, WebviewWindow};

// The origin Tauri serves the app from; the command ACL only admits calls from it.
// Windows serves it over http://tauri.localhost, everywhere else tauri://localhost.
#[cfg(windows)]
const APP_URL: &str = "http://tauri.localhost";
#[cfg(not(windows))]
const APP_URL: &str = "tauri://localhost";

/// The commands read `XDG_DATA_HOME` through the path resolver, so tests that
/// set it take turns.
static ENV: Mutex<()> = Mutex::new(());

pub struct Desk {
    _guard: std::sync::MutexGuard<'static, ()>,
    pub dir: std::path::PathBuf,
    _app: App<tauri::test::MockRuntime>,
    window: WebviewWindow<tauri::test::MockRuntime>,
}

pub fn desk(tag: &str) -> Desk {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    // One data directory for the whole test binary, emptied for each test: the
    // library fixes its vault paths on first use (`unv_app::access::set_paths` is a
    // `OnceLock`, as it is in the app, which has one data directory), so a directory
    // per test would leave the second test pointing at the first one's vault.
    let _ = tag;
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let dir = DIR
        .get_or_init(|| std::env::temp_dir().join(format!("unv-ipc-{}", std::process::id())))
        .clone();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", &dir);
    std::env::set_var("HOME", &dir);
    let app = unenverse_lib::register(mock_builder())
        .build(mock_context(noop_assets()))
        .expect("app");
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("window");
    Desk {
        _guard: guard,
        dir,
        _app: app,
        window,
    }
}

/// Lock the vault before the next test gets the shared data directory. Most tests
/// finish unlocked, which leaves the operations executor (and any LAN server)
/// running with the old key; the next test then wipes the directory and unlocks
/// while that executor can still write into it, and the unlock fails with "Wrong
/// master password" against a database the old test recreated. Locking stops and
/// joins them. It runs before the field guard is released.
impl Drop for Desk {
    fn drop(&mut self) {
        let _ = self.call("lock_vault", json!({}));
    }
}

impl Desk {
    pub fn call(&self, cmd: &str, body: Value) -> Result<Value, Value> {
        let res = get_ipc_response(
            &self.window,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: APP_URL.parse().unwrap(),
                body: InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        );
        res.map(|b| b.deserialize::<Value>().unwrap())
    }
    pub fn ok(&self, cmd: &str, body: Value) -> Value {
        self.call(cmd, body)
            .unwrap_or_else(|e| panic!("{cmd}: {e}"))
    }
    pub fn unlock(&self, pw: &str) -> Result<Value, Value> {
        self.call("unlock_vault", json!({ "password": pw }))
    }
}
