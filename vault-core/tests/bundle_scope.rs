// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Bundle-local template resolution, from its stated contract: a template can name
//! the bundle's own variables, a member's fields by slot, and (when the caller
//! allows it) global references; anything it cannot resolve is an error so a
//! half-rendered credential never looks valid; and a result is secret as soon as
//! any value that went into it was.

use serde_json::{json, Value};
use vault_core::bundle_scope::{
    render_composite, resolve, CompositeScopeError, ScopeError, ScopedValue,
};
use vault_core::composite::{Kind, RenderError};

fn none(_: &str) -> Option<ScopedValue> {
    None
}

fn text(bundle: &Value, members: &[Value], template: &str) -> Result<ScopedValue, ScopeError> {
    resolve(bundle, members, template, &[], none).map(|o| o.value)
}

fn member(slot: &str, fields: Value) -> Value {
    let mut m = json!({ "provider": "Svc", "bundle_slot": slot });
    for (k, v) in fields.as_object().unwrap() {
        m[k] = v.clone();
    }
    m
}

// ---- slots

#[test]
fn two_members_may_not_share_a_slot_and_a_slot_defaults_to_the_provider_name() {
    let a = member("main", json!({}));
    let b = member("main", json!({}));
    assert_eq!(
        text(&json!({}), &[a, b], "x").unwrap_err(),
        ScopeError::Invalid("duplicate bundle slot \"main\"".into())
    );
    let unslotted = json!({ "provider": "Discord", "api_key": "k" });
    assert_eq!(
        text(&json!({}), &[unslotted], "{Discord.key}")
            .unwrap()
            .value,
        "k"
    );
}

#[test]
fn a_name_that_exists_at_two_levels_is_reported_and_the_part_wins() {
    let bundle = json!({ "extra_vars": [{ "key": "host", "value": "local", "public": true }] });
    let parts = [json!({ "key": "host", "value": "part", "public": true })];
    let out = resolve(&bundle, &[], "{host}", &parts, none).unwrap();
    assert_eq!(out.value.value, "part");
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(out.warnings[0].contains("\"host\""));

    let slot_clash = member("host", json!({}));
    let out = resolve(&bundle, &[slot_clash], "{host}", &[], none).unwrap();
    assert_eq!(
        out.warnings.len(),
        1,
        "a slot named like a local is also ambiguous"
    );
}

// ---- references that cannot be resolved

#[test]
fn every_kind_of_unresolvable_reference_is_an_error_not_an_empty_string() {
    let b = json!({});
    for (template, want) in [
        ("{nothing}", ScopeError::Unresolved("nothing".into())),
        ("{slot.key}", ScopeError::Unresolved("slot.key".into())),
        ("${GLOBAL/x}", ScopeError::Unresolved("${GLOBAL/x}".into())),
    ] {
        assert_eq!(text(&b, &[], template).unwrap_err(), want, "{template}");
    }
    let m = member("s", json!({ "api_key": "k" }));
    assert_eq!(
        text(&b, &[m], "{s.no_such_field}").unwrap_err(),
        ScopeError::Unresolved("s.no_such_field".into())
    );
}

#[test]
fn malformed_templates_name_the_problem_and_where() {
    let b = json!({});
    for (template, fragment) in [
        ("a }", "unmatched closing brace at 2"),
        ("a {open", "unclosed brace at 2"),
        ("${open", "unclosed global reference at 0"),
        ("{1bad}", "invalid reference \"1bad\""),
        ("{a.b.c}", "invalid reference \"a.b.c\""),
        ("{a b}", "invalid reference \"a b\""),
        ("{}", "invalid reference \"\""),
        ("{a.}", "invalid reference \"a.\""),
    ] {
        match text(&b, &[], template).unwrap_err() {
            ScopeError::Invalid(msg) => {
                assert!(msg.contains(fragment), "{template}: {msg}")
            }
            other => panic!("{template}: {other:?}"),
        }
    }
}

#[test]
fn doubled_braces_are_literal_braces() {
    let out = text(&json!({}), &[], "{{not a ref}} é").unwrap();
    assert_eq!(out.value, "{not a ref} é");
    assert!(!out.secret);
}

// ---- globals

#[test]
fn a_global_reference_is_resolved_by_the_caller_and_keeps_the_callers_secrecy() {
    let globals = |name: &str| match name {
        "Vault/host" => Some(ScopedValue {
            value: "db.internal".into(),
            secret: false,
        }),
        "Vault/pw" => Some(ScopedValue {
            value: "hunter2".into(),
            secret: true,
        }),
        _ => None,
    };
    let host = resolve(&json!({}), &[], "h=${Vault/host}", &[], globals).unwrap();
    assert_eq!(
        host.value,
        ScopedValue {
            value: "h=db.internal".into(),
            secret: false
        }
    );
    let both = resolve(&json!({}), &[], "${Vault/host}:${Vault/pw}", &[], globals).unwrap();
    assert!(
        both.value.secret,
        "one secret input makes the whole result secret"
    );
}

// ---- local variables

#[test]
fn a_local_is_secret_unless_it_says_otherwise_and_a_template_inherits_its_inputs() {
    let bundle = json!({ "extra_vars": [
        { "key": "plain", "value": "p" },
        { "key": "open", "value": "o", "public": true },
        { "key": "flagged_open", "value": "f", "secret": false },
        { "key": "mix", "value": "{open}-{plain}", "kind": "template", "public": true },
        { "key": "novalue" }
    ]});
    assert!(text(&bundle, &[], "{plain}").unwrap().secret);
    assert!(!text(&bundle, &[], "{open}").unwrap().secret);
    assert!(!text(&bundle, &[], "{flagged_open}").unwrap().secret);
    let mix = text(&bundle, &[], "{mix}").unwrap();
    assert_eq!(mix.value, "o-p");
    assert!(
        mix.secret,
        "a public-looking template built from a secret is secret"
    );
    assert_eq!(
        text(&bundle, &[], "{novalue}").unwrap_err(),
        ScopeError::Unresolved("novalue".into()),
        "a variable with no value does not resolve to nothing"
    );
}

#[test]
fn a_part_without_a_string_value_is_invalid() {
    let parts = [json!({ "key": "p", "value": 5 })];
    assert!(matches!(
        resolve(&json!({}), &[], "{p}", &parts, none),
        Err(ScopeError::Invalid(_))
    ));
    let secret_part = [json!({ "key": "p", "value": "v" })];
    assert!(
        resolve(&json!({}), &[], "{p}", &secret_part, none)
            .unwrap()
            .value
            .secret
    );
    let public_part = [json!({ "key": "p", "value": "v", "public": true })];
    assert!(
        !resolve(&json!({}), &[], "{p}", &public_part, none)
            .unwrap()
            .value
            .secret
    );
}

#[test]
fn a_chain_of_templates_four_deep_resolves_and_a_fifth_level_is_refused() {
    let var =
        |k: &str, v: &str| json!({ "key": k, "value": v, "kind": "template", "public": true });
    let ok = json!({ "extra_vars": [var("a", "{b}"), var("b", "{c}"), var("c", "{d}"), var("d", "{e}"), var("e", "end")] });
    assert_eq!(text(&ok, &[], "{a}").unwrap().value, "end");
    let too_deep = json!({ "extra_vars": [var("a", "{b}"), var("b", "{c}"), var("c", "{d}"), var("d", "{e}"), var("e", "{f}"), var("f", "end")] });
    assert_eq!(
        text(&too_deep, &[], "{a}").unwrap_err(),
        ScopeError::Depth("e".into())
    );
    let self_ref = json!({ "extra_vars": [var("a", "x{a}")] });
    assert_eq!(
        text(&self_ref, &[], "{a}").unwrap_err(),
        ScopeError::Cycle(vec!["a".into(), "a".into()])
    );
}

// ---- member fields

#[test]
fn well_known_aliases_reach_the_members_standard_fields() {
    let m = member(
        "s",
        json!({
            "api_key": "KEY", "api_secret": "SEC", "username": "USER", "api_url": "https://x.test",
            "email": "me@x.test", "key_id": "KID", "mount_path": "/mnt/x",
            "primary_public": true,
        }),
    );
    for (field, want) in [
        ("token", "KEY"),
        ("Password", "KEY"),
        ("api_key", "KEY"),
        ("bearer", "KEY"),
        ("client_secret", "SEC"),
        ("shared_secret", "SEC"),
        ("login", "USER"),
        ("user", "USER"),
        ("endpoint", "https://x.test"),
        ("base_url", "https://x.test"),
        ("mail", "me@x.test"),
        ("client_id", "KID"),
        ("kid", "KID"),
        ("mount", "/mnt/x"),
        ("file", "/mnt/x"),
    ] {
        let out = text(
            &json!({}),
            std::slice::from_ref(&m),
            &format!("{{s.{field}}}"),
        )
        .unwrap_or_else(|e| panic!("{field}: {e:?}"));
        assert_eq!(out.value, want, "{field}");
    }
    // Only the key was marked public; the secret and everything else stay secret
    // where the field is a credential, and plain where it is not.
    assert!(
        !text(&json!({}), std::slice::from_ref(&m), "{s.token}")
            .unwrap()
            .secret
    );
    assert!(
        text(&json!({}), std::slice::from_ref(&m), "{s.client_secret}")
            .unwrap()
            .secret
    );
    assert!(
        !text(&json!({}), std::slice::from_ref(&m), "{s.login}")
            .unwrap()
            .secret
    );
}

#[test]
fn a_role_name_picks_the_field_the_entry_gave_that_role() {
    let m = member(
        "s",
        json!({
            "api_key": "ID-VALUE", "primary_role": "Client ID", "primary_public": true,
            "api_secret": "SECRET-VALUE", "secret_role": "client-secret",
        }),
    );
    let one = std::slice::from_ref(&m);
    let id = text(&json!({}), one, "{s.CLIENT_ID}").unwrap();
    assert_eq!((id.value.as_str(), id.secret), ("ID-VALUE", false));
    let sec = text(&json!({}), one, "{s.client_secret}").unwrap();
    assert_eq!((sec.value.as_str(), sec.secret), ("SECRET-VALUE", true));
}

#[test]
fn a_member_variable_is_found_by_name_with_its_own_secrecy_and_internal_fields_are_not_reachable() {
    let m = member(
        "s",
        json!({
            "extra_vars": [
                { "key": "region", "value": "eu", "public": true },
                { "key": "token2", "value": "t" },
                { "key": "novalue" },
            ],
            "projectIds": ["a"], "categories": ["c"], "version_history": [],
            "totp_secret": "JBSWY3DP",
        }),
    );
    let one = std::slice::from_ref(&m);
    let region = text(&json!({}), one, "{s.region}").unwrap();
    assert_eq!((region.value.as_str(), region.secret), ("eu", false));
    assert!(text(&json!({}), one, "{s.token2}").unwrap().secret);
    for hidden in [
        "projectIds",
        "categories",
        "version_history",
        "novalue",
        "missing",
    ] {
        assert!(
            text(&json!({}), one, &format!("{{s.{hidden}}}")).is_err(),
            "{hidden} must not resolve"
        );
    }
    assert!(text(&json!({}), one, "{s.totp_secret}").unwrap().secret);
}

// ---- composites

#[test]
fn a_connection_string_encodes_a_password_so_it_cannot_move_the_host() {
    let m = member(
        "db",
        json!({ "username": "app", "api_key": "p@ss:w/rd#1", "api_url": "db.internal" }),
    );
    let out = render_composite(
        &json!({}),
        &[m],
        "postgres://{db.user}:{db.key}@{db.endpoint}/main",
        &[],
        Kind::Connection,
        none,
    )
    .unwrap();
    assert!(out.secret);
    assert!(
        out.value.starts_with("postgres://app:") && out.value.ends_with("@db.internal/main"),
        "{}",
        out.value
    );
    assert!(
        !out.value.contains("p@ss"),
        "the password is encoded: {}",
        out.value
    );
    assert_eq!(out.value.matches('@').count(), 1, "{}", out.value);
}

#[test]
fn a_composite_takes_globals_literal_braces_and_unicode_through_and_reports_what_it_cannot_render()
{
    let globals = |n: &str| {
        (n == "V/host").then(|| ScopedValue {
            value: "h.test".into(),
            secret: false,
        })
    };
    let out = render_composite(
        &json!({ "extra_vars": [{ "key": "p", "value": "443", "public": true }] }),
        &[],
        "https://${V/host}:{p}/ünï {{x}}",
        &[],
        Kind::Custom,
        globals,
    )
    .unwrap();
    assert_eq!(out.value, "https://h.test:443/ünï {x}");
    assert!(!out.secret);

    for template in ["{missing}", "${unclosed", "{unclosed", "a } b", "${nope}"] {
        let err = render_composite(&json!({}), &[], template, &[], Kind::Custom, none).unwrap_err();
        assert!(
            matches!(err, CompositeScopeError::Scope(_)),
            "{template}: {err:?}"
        );
    }

    // A control character in a part is refused for URL-shaped kinds (it could
    // smuggle a header), but a custom kind renders raw.
    let bundle = json!({ "extra_vars": [{ "key": "p", "value": "a\nb", "public": true }] });
    let err = render_composite(&bundle, &[], "https://x/{p}", &[], Kind::Link, none).unwrap_err();
    assert_eq!(
        err,
        CompositeScopeError::Composite(RenderError::ControlCharacterInPart("scope_0".into()))
    );
    assert!(render_composite(&bundle, &[], "{p}", &[], Kind::Custom, none).is_ok());
}
