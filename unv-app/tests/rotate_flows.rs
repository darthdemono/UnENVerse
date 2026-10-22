// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The parts of `unv rotate` the command line cannot reach: the entry store the engine writes
//! through, the connector choice and its settings, and the refusals before anything is called.
//! One vault for the whole file (the paths are process-wide), so it is a single ordered story.

use serde_json::json;
use unv_app::access::{self, AuthOpts};
use unv_app::rotate_cmd::{self, Opts, PgOpts, VaultEntry};
use vault_core::rotation::{Credential, EntryStore};

fn opts<'a>(entry: &'a str) -> Opts<'a> {
    Opts {
        entry,
        issuer: "cloudflare",
        field: "api_key",
        token_id: Some("tok-1"),
        base_url: Some("http://127.0.0.1:9"),
        allow_roll: false,
        revoke_now: false,
        resume: None,
        pg: PgOpts::default(),
    }
}

fn value_of(a: &access::Access, entry: &str, field: &str) -> Option<String> {
    let v = a.load_vault().unwrap();
    let list = unv_app::data::entries(&v);
    list.iter()
        .find(|e| e["provider"] == entry)
        .and_then(|e| e[field].as_str().map(str::to_string))
}

#[test]
fn the_entry_store_the_connector_choice_and_the_refusals() {
    let dir = std::env::temp_dir().join(format!("unv-rotflow-{}", vault_core::new_uuid()));
    std::fs::create_dir_all(&dir).unwrap();
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
    let old = format!("old-{}", vault_core::new_uuid());
    let mut vault = a.load_vault_or_empty().unwrap();
    unv_app::data::entries_mut(&mut vault).push(json!({
        "id": "e1", "provider": "Edge", "api_key": old, "compromised": true,
        "price_type": "free", "secretType": "api_key", "categories": [],
        "projectIds": ["Universal"], "scopes": []
    }));
    unv_app::data::entries_mut(&mut vault).push(json!({
        "id": "e2", "provider": "Blank", "api_key": "",
        "price_type": "free", "secretType": "api_key", "categories": [],
        "projectIds": ["Universal"], "scopes": []
    }));
    a.save(&vault).unwrap();

    // ── the entry store ──
    let mut store = VaultEntry::new(&a, "Edge", "api_key", "tok-1");
    assert_eq!(
        store.current(),
        Credential {
            id: "tok-1".into(),
            value: old.clone()
        }
    );
    let fresh = Credential {
        id: "tok-2".into(),
        value: format!("new-{}", vault_core::new_uuid()),
    };
    store.commit(&fresh, 2).unwrap();
    assert_eq!(
        value_of(&a, "Edge", "api_key").as_deref(),
        Some(fresh.value.as_str())
    );
    let saved = a.load_vault().unwrap();
    let edge = unv_app::data::entries(&saved)
        .into_iter()
        .find(|e| e["provider"] == "Edge")
        .unwrap();
    assert_eq!(
        edge["rotation"]["token_id"], "tok-2",
        "the id follows the credential"
    );
    assert!(edge["last_rotated_at"].is_string());
    assert!(
        edge.get("compromised").is_none(),
        "rotating answers a compromise"
    );
    assert_eq!(store.current().id, "tok-2");
    // Putting the old value back (the rollback rule) restores both value and id.
    store
        .restore(&Credential {
            id: "tok-1".into(),
            value: String::new(),
        })
        .unwrap();
    assert_eq!(
        value_of(&a, "Edge", "api_key").as_deref(),
        Some(old.as_str())
    );
    let back = unv_app::data::entries(&a.load_vault().unwrap())
        .into_iter()
        .find(|e| e["provider"] == "Edge")
        .unwrap();
    assert_eq!(back["rotation"]["token_id"], "tok-1");
    // A fresh process has no memory of the old value: restore says so instead of guessing.
    let mut forgetful = VaultEntry::new(&a, "Edge", "api_key", "tok-1");
    let err = forgetful
        .restore(&Credential {
            id: "tok-1".into(),
            value: String::new(),
        })
        .unwrap_err();
    assert!(err.contains("not available"), "{err}");
    // An entry that is not there reads as an empty credential, and cannot be written.
    let ghost = VaultEntry::new(&a, "Ghost", "api_key", "x");
    assert_eq!(ghost.current().value, "");
    let mut ghost = ghost;
    assert!(ghost.commit(&fresh, 2).is_err());

    // ── the connector choice ──
    let manager = Credential {
        id: "tok-1".into(),
        value: "v".into(),
    };
    let mut o = opts("Edge");
    o.issuer = "nonsense";
    assert!(rotate_cmd::connector(&o, manager.clone())
        .err()
        .unwrap()
        .message
        .contains("Unknown issuer"));
    o.issuer = "cloudflare";
    assert!(rotate_cmd::connector(&o, manager.clone()).is_ok());
    o.issuer = "postgres";
    std::env::remove_var("UNV_PG_ADMIN_PASSWORD");
    for roles in ["onlyone", "x,x", ",b", "a,"] {
        o.pg.roles = Some(roles);
        let err = rotate_cmd::connector(&o, manager.clone()).err().unwrap();
        assert!(
            err.message.contains("--pg-roles"),
            "{roles}: {}",
            err.message
        );
    }
    o.pg.roles = None;
    let err = rotate_cmd::connector(&o, manager.clone()).err().unwrap();
    assert!(
        err.message.contains("UNV_PG_ADMIN_PASSWORD"),
        "{}",
        err.message
    );
    std::env::set_var("UNV_PG_ADMIN_PASSWORD", vault_core::new_uuid());
    assert!(rotate_cmd::connector(&o, manager.clone()).is_ok());
    o.pg = PgOpts {
        host: Some("db"),
        port: Some("5433"),
        database: Some("app"),
        admin_user: Some("root"),
        roles: Some("blue,green"),
        psql: Some("/usr/bin/psql"),
    };
    assert!(rotate_cmd::connector(&o, manager.clone()).is_ok());
    std::env::remove_var("UNV_PG_ADMIN_PASSWORD");

    // ── refusals before anything is called ──
    let err = rotate_cmd::run(&a, &opts("Blank")).unwrap_err();
    assert!(err.message.contains("has no value"), "{}", err.message);
    let mut resume = opts("Edge");
    resume.resume = Some("no-such-run");
    assert_eq!(
        rotate_cmd::run(&a, &resume).unwrap_err().message,
        "No rotation run no-such-run"
    );
    // Edge remembers its token id (restored above), so --token-id is no longer needed; the issuer
    // here is a closed port, so the run fails cleanly at the first call and leaves the entry alone.
    let mut remembered = opts("Edge");
    remembered.token_id = None;
    let err = rotate_cmd::run(&a, &remembered).unwrap_err();
    assert!(
        err.message.contains("reading the old token"),
        "{}",
        err.message
    );
    assert_eq!(
        value_of(&a, "Edge", "api_key").as_deref(),
        Some(old.as_str())
    );
    // The failed run is on record and can be looked up by id.
    assert!(rotate_cmd::status(&a, None).is_ok());
    assert!(rotate_cmd::status(&a, Some("nope"))
        .unwrap_err()
        .message
        .contains("No rotation run nope"));
    // An entry that never had a token id asks for one.
    let err = rotate_cmd::run(&a, &{
        let mut o = opts("Blank");
        o.token_id = None;
        o
    })
    .unwrap_err();
    assert!(err.message.contains("--token-id"), "{}", err.message);
    let _ = std::fs::remove_dir_all(&dir);
}
