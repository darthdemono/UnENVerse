// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! R39 slice 1, `vault-core/src/users.rs`: what the account layer does when the database cannot
//! answer, and the refusals and upgrades that ordinary use rarely reaches.
//!
//! The promise tested here: an account operation that cannot reach its table says so with an error.
//! It never panics, and it never reports a change it did not make (a login that "succeeds", a
//! password that "was set") because the write behind it failed.

use sha2::{Digest, Sha256};
use vault_core::permex::Expr;
use vault_core::users::*;
use vault_core::SqlConnection as Conn;

/// A database with no tables at all: every statement fails.
fn bare() -> Conn {
    Conn::open_in_memory().unwrap()
}

/// A password made at run time from the clock and the process, so no credential is written in
/// the source and no two calls return the same one.
fn pw(tag: &str) -> String {
    use sha2::{Digest, Sha256};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    hex::encode(Sha256::digest(format!(
        "{tag}{now}{n}{}",
        std::process::id()
    )))
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn ready() -> Conn {
    let c = bare();
    // The vault schema owns `vault_meta`, which the user schema's migration marker needs.
    vault_core::init_schema(&c).unwrap();
    init_users_schema(&c).unwrap();
    c
}

#[test]
fn every_account_operation_reports_a_database_it_cannot_use() {
    let c = bare();
    let err = |name: &str, r: Result<(), String>| {
        assert!(
            r.is_err(),
            "{name} claimed success on a database with no tables"
        );
    };
    err("init_users_schema (read-only copy)", {
        let ro = Conn::open_in_memory().unwrap();
        ro.execute_batch("PRAGMA query_only = ON").unwrap();
        init_users_schema(&ro)
    });
    err(
        "get_permission_expr",
        get_permission_expr(&c, "user", "u", "read").map(|_| ()),
    );
    err(
        "set_permission_expr",
        set_permission_expr(&c, "user", "u", "read", "*"),
    );
    err("strict_write_for", strict_write_for(&c, "u").map(|_| ()));
    err("set_strict_write", set_strict_write(&c, "user", "u", true));
    err(
        "set_strict_write (class)",
        set_strict_write(&c, "class", "c", true),
    );
    err(
        "effective_permission_expr",
        effective_permission_expr(&c, "u", "read").map(|_| ()),
    );
    err("create_user", create_user(&c, "a", None, false).map(|_| ()));
    err("rename_user", rename_user(&c, "u", "b"));
    err(
        "set_user_password",
        set_user_password(&c, "u", Some(&pw("db-failure"))),
    );
    err(
        "verify_user_password",
        verify_user_password(&c, "a", &pw("probe")).map(|_| ()),
    );
    err(
        "verify_user_token",
        verify_user_token(&c, &"a".repeat(64)).map(|_| ()),
    );
    err("list_users", list_users(&c).map(|_| ()));
    err("totp_enroll", totp_enroll(&c, "u", "unv").map(|_| ()));
    err("totp_confirm", totp_confirm(&c, "u", "000000").map(|_| ()));
    err("totp_disable", totp_disable(&c, "u"));
    err("totp_status", totp_status(&c, "u").map(|_| ()));
    err("totp_required", totp_required(&c, "u").map(|_| ()));
    err(
        "verify_user_totp",
        verify_user_totp(&c, "u", "000000").map(|_| ()),
    );
    err("delete_user", delete_user(&c, "u"));
    err("list_user_classes", list_user_classes(&c).map(|_| ()));
    err(
        "create_user_class",
        create_user_class(&c, "n", "d", false, false, false).map(|_| ()),
    );
    err(
        "update_user_class",
        update_user_class(&c, "c", "n", "d", false, false, false),
    );
    err(
        "class_member_count",
        class_member_count(&c, "c").map(|_| ()),
    );
    err("delete_user_class", delete_user_class(&c, "c"));
    err(
        "get_class_permissions",
        get_class_permissions(&c, "c").map(|_| ()),
    );
    err("set_class_permissions", set_class_permissions(&c, "c", &[]));
    err("assign_user_class", assign_user_class(&c, "u", Some("c")));
    err(
        "get_user_capabilities",
        get_user_capabilities(&c, "u").map(|_| ()),
    );
    err(
        "class_authority_tier",
        class_authority_tier(&c, "c").map(|_| ()),
    );
    err("token_user_id", token_user_id(&c, "t").map(|_| ()));
    err(
        "user_authority_tier",
        user_authority_tier(&c, "u").map(|_| ()),
    );
    err("ensure_owner_user", ensure_owner_user(&c).map(|_| ()));
    err(
        "seed_default_admin",
        seed_default_admin(&c, None).map(|_| ()),
    );
    err("ensure_peer_account", ensure_peer_account(&c).map(|_| ()));
    err(
        "create_user_token",
        create_user_token(&c, "u", None, None).map(|_| ()),
    );
    err("revoke_user_token", revoke_user_token(&c, "t"));
    err("list_user_tokens", list_user_tokens(&c, "u").map(|_| ()));
    err(
        "get_user_permissions",
        get_user_permissions(&c, "u").map(|_| ()),
    );
    err("set_user_permissions", set_user_permissions(&c, "u", &[]));
}

#[test]
fn a_permission_rule_names_a_known_subject_and_a_known_permission() {
    let c = ready();
    let e = set_permission_expr(&c, "group", "x", "read", "*").unwrap_err();
    assert!(e.contains("unknown subject kind 'group'"), "{e}");
    let e = set_permission_expr(&c, "user", "x", "admin", "*").unwrap_err();
    assert!(e.contains("unknown permission 'admin'"), "{e}");
    let u = create_user(&c, "alice", Some(&pw("alice")), false).unwrap();
    let e = effective_permission_expr(&c, &u.id, "delete").unwrap_err();
    assert!(e.contains("unknown permission 'delete'"), "{e}");
    // Nothing was stored by the refused calls.
    assert_eq!(get_permission_expr(&c, "user", "x", "read").unwrap(), None);
    let _: Option<Expr> = effective_permission_expr(&c, &u.id, "read").unwrap();
}

/// A user table written by Phase 5.0 holds `<salt_hex>:<sha256(salt || password)>`. A correct login
/// must still work, and it must leave an Argon2id hash behind so the weak one is gone after it.
#[test]
fn a_legacy_sha256_password_still_logs_in_and_is_upgraded_to_argon2() {
    let c = ready();
    let old_pw = pw("legacy");
    let u = create_user(&c, "old", None, false).unwrap();
    let salt = [7u8; 16];
    let mut h = Sha256::new();
    h.update(salt);
    h.update(old_pw.as_bytes());
    let legacy = format!("{}:{}", hex::encode(salt), hex::encode(h.finalize()));
    c.execute(
        "UPDATE users SET password_hash = ?1 WHERE id = ?2",
        rusqlite::params![legacy, u.id],
    )
    .unwrap();
    let stored = |c: &Conn| -> String {
        c.query_row(
            "SELECT password_hash FROM users WHERE id = ?1",
            [&u.id],
            |r| r.get(0),
        )
        .unwrap()
    };
    // A wrong password neither logs in nor touches the stored hash.
    assert!(verify_user_password(&c, "old", &pw("wrong"))
        .unwrap()
        .is_none());
    assert_eq!(stored(&c), legacy);
    // The right one logs in, and the hash is now Argon2id.
    assert_eq!(
        verify_user_password(&c, "old", &old_pw)
            .unwrap()
            .unwrap()
            .id,
        u.id
    );
    assert!(stored(&c).starts_with("$argon2"), "{}", stored(&c));
    // ...and the same password keeps working against the new hash.
    assert!(verify_user_password(&c, "old", &old_pw).unwrap().is_some());
}

/// A stored hash that is not one of the two formats can never match (it is a failed login, not a
/// crash and not an error that tells an attacker the row is damaged).
#[test]
fn a_damaged_stored_hash_is_a_failed_login_not_an_error() {
    let c = ready();
    let bads = [
        "no-colon-at-all",
        "zz:00",
        "00:zz",
        "$argon2id$not-a-real-hash",
        "",
    ];
    for (n, bad) in bads.into_iter().enumerate() {
        let name = format!("u-{n}");
        let u = create_user(&c, &name, None, false).unwrap();
        c.execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            rusqlite::params![bad, u.id],
        )
        .unwrap();
        let r = verify_user_password(&c, &name, &pw("any"));
        assert!(matches!(r, Ok(None)), "case {n} was not a failed login");
    }
}

#[test]
fn second_factor_calls_for_an_unknown_user_say_so() {
    let c = ready();
    for r in [
        totp_confirm(&c, "ghost", "000000").map(|_| ()),
        totp_enroll(&c, "ghost", "unv").map(|_| ()),
    ] {
        assert!(r.unwrap_err().contains("no such user"));
    }
}

#[test]
fn the_owner_account_cannot_be_deleted_and_an_unknown_user_has_no_authority() {
    let c = ready();
    let owner = ensure_owner_user(&c).unwrap();
    let e = delete_user(&c, &owner).unwrap_err();
    assert!(e.contains("Cannot delete the owner account"), "{e}");
    assert!(
        list_users(&c).unwrap().iter().any(|u| u.id == owner),
        "the owner is still there"
    );
    // Someone who is not in the table gets the lowest tier, not an error and not a guess.
    assert_eq!(user_authority_tier(&c, "ghost").unwrap(), 0);
    // And no code of theirs is ever accepted.
    assert!(!verify_user_totp(&c, "ghost", "000000").unwrap());
}

#[test]
fn a_class_keeps_the_permissions_it_was_given_and_loses_them_with_the_class() {
    let c = ready();
    let class = create_user_class(&c, "editors", "can edit", false, false, false).unwrap();
    let grant = |scope: &str, perm: &str| ClassPermission {
        class_id: class.id.clone(),
        scope_type: "project".into(),
        scope_value: scope.into(),
        permission: perm.into(),
    };
    set_class_permissions(&c, &class.id, &[grant("web", "write"), grant("db", "read")]).unwrap();
    let mut got: Vec<(String, String)> = get_class_permissions(&c, &class.id)
        .unwrap()
        .into_iter()
        .map(|p| (p.scope_value, p.permission))
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![("db".into(), "read".into()), ("web".into(), "write".into())]
    );
    // Setting again replaces the set, it does not add to it.
    set_class_permissions(&c, &class.id, &[grant("web", "read")]).unwrap();
    assert_eq!(get_class_permissions(&c, &class.id).unwrap().len(), 1);
    delete_user_class(&c, &class.id).unwrap();
    assert!(get_class_permissions(&c, &class.id).unwrap().is_empty());
}
