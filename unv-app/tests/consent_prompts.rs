// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The two commands that ask before they change who may act: enabling a plugin and activating a
//! vault instance. A "yes" does it, a "no" leaves everything as it was. The question is a parameter,
//! so the answer is chosen here rather than by a terminal.

use unv_app::access::{self, AuthOpts};
use unv_app::error::CliError;
use unv_app::{plugin_cmd, vault_cmd};
use vault_core::identity::{self, Role};
use vault_core::plugin;

#[test]
fn a_no_changes_nothing_and_a_yes_does_it() {
    let dir = std::env::temp_dir().join(format!("unv-consent-{}", vault_core::new_uuid()));
    std::fs::create_dir_all(dir.join("plugins/demo")).unwrap();
    access::set_paths(Some(dir.join("vault.db")), Some(dir.join("vault.salt")));
    let password = vault_core::new_uuid();
    let a = access::open_access(&AuthOpts {
        server: None,
        password: Some(&password),
        user: None,
        token: None,
        session_token: None,
        totp: None,
        init: true,
    })
    .unwrap();
    let conn = a.conn().unwrap();

    // A plugin on disk, installed (disabled).
    std::fs::write(
        dir.join("plugins/demo/plugin.json"),
        r#"{"name":"demo","version":"1.0.0","protocol":"unv-plugin-rpc/1","exec":"run.sh","capabilities":["notifier"],"slots":["url"]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("plugins/demo/run.sh"), "#!/bin/sh\n").unwrap();
    plugin_cmd::install(&a, Some(&dir.join("plugins")), "demo").unwrap();

    // Declined: not enabled, and the question named what the plugin can do and who it acts as.
    let mut asked = String::new();
    let err = plugin_cmd::enable_with(&a, "demo", &mut |q| {
        asked = q.to_string();
        Ok(false)
    })
    .unwrap_err();
    assert_eq!(err.message, "Not enabled");
    assert!(
        asked.contains("notifier") && asked.contains("act as you") && asked.contains("url"),
        "{asked}"
    );
    assert!(!plugin::get(&conn, "demo").unwrap().unwrap().enabled);

    // A failing prompt (no terminal) is an error, not a yes.
    let err = plugin_cmd::enable_with(&a, "demo", &mut |_| Err(CliError::invalid("no terminal")))
        .unwrap_err();
    assert_eq!(err.message, "no terminal");
    assert!(!plugin::get(&conn, "demo").unwrap().unwrap().enabled);

    plugin_cmd::enable_with(&a, "demo", &mut |_| Ok(true)).unwrap();
    assert!(plugin::get(&conn, "demo").unwrap().unwrap().enabled);
    // Enabling something that was never installed is not found.
    assert!(plugin_cmd::enable_with(&a, "ghost", &mut |_| Ok(true)).is_err());

    // A restored copy: standby until someone says the old one is retired.
    identity::become_standby(&conn).unwrap();
    let err = vault_cmd::activate_with(&a, &mut |q| {
        assert!(q.contains("epoch 1 -> 2") && q.contains("retired"), "{q}");
        Ok(false)
    })
    .unwrap_err();
    assert_eq!(err.message, "Not activated");
    assert_eq!(identity::ensure(&conn).unwrap().role, Role::Standby);
    vault_cmd::activate_with(&a, &mut |_| Ok(true)).unwrap();
    let id = identity::ensure(&conn).unwrap();
    assert_eq!((id.role, id.epoch), (Role::Active, 2));

    drop(conn);
    drop(a);
    let _ = std::fs::remove_dir_all(&dir);
}
