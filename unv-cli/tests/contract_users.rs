// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Multiple users and permission expressions, as the README describes them
//! ("Multiple users", "Permission expressions"), through the real `unv`
//! against a real `unv-server`:
//!
//! * a sub-user sees and changes only what its expressions grant, and an
//!   absent expression means "no grant", not "no restriction";
//! * a class expression and an individual one are ANDed, so a class
//!   restriction cannot be undone by an individual grant;
//! * strict write scoping makes a write satisfy every scope;
//! * an unparseable expression is refused when saved, never stored;
//! * a token is created without ever reaching stdout, the output policy is
//!   checked before minting, and a revoked token stops working;
//! * a user cannot grant itself more than it holds.

mod common;
#[path = "common/server.rs"]
mod server;

use common::Vault;
use serde_json::Value;
use server::Server;

const OWNER_NOTE: &str = "owner-note";
const PW: &str = "deploy-password-12345";

/// Two projects and four entries: web staging, web production, api, and one
/// with no project. A server over that vault, and a user `deploy` with a password
/// and no grants at all.
fn setup(tag: &str) -> (Vault, Server) {
    let v = Vault::new(tag);
    v.ok(&["project", "add", "Web"]);
    v.ok(&["project", "add", "Api"]);
    v.ok(&[
        "entry",
        "add",
        "WebStaging",
        "--key",
        "ws-secret-11",
        "--projects",
        "web",
        "--env",
        "staging",
    ]);
    v.ok(&[
        "entry",
        "add",
        "WebProd",
        "--key",
        "wp-secret-22",
        "--projects",
        "web",
        "--env",
        "production",
    ]);
    v.ok(&[
        "entry",
        "add",
        "ApiKey",
        "--key",
        "ak-secret-33",
        "--projects",
        "api",
        "--env",
        "staging",
    ]);
    let s = Server::start(&v, false);
    let added = v.on(&s, &["user", "add", "deploy", "--user-password", PW]);
    assert_eq!(added.code, 0, "{}", added.all());
    (v, s)
}

fn visible(v: &Vault, s: &Server) -> Vec<String> {
    let o = v.as_user(s, "deploy", PW, &["list"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let mut names: Vec<String> = o.json()["data"]["entries"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| e["provider"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn a_user_with_no_grant_sees_nothing_because_absence_means_no_grant() {
    let (v, s) = setup("nogrant");
    assert!(
        visible(&v, &s).is_empty(),
        "absent expression must mean no access"
    );
    let set = v.as_user(
        &s,
        "deploy",
        PW,
        &["entry", "set", "WebStaging", "--notes", "x"],
    );
    assert_ne!(set.code, 0, "{}", set.all());
}

#[test]
fn a_read_expression_decides_exactly_which_entries_a_user_sees() {
    let (v, s) = setup("read");
    v.on(
        &s,
        &["perm", "set", "user", "deploy", "--read", "project:web"],
    );
    assert_eq!(visible(&v, &s), vec!["WebProd", "WebStaging"]);

    v.on(
        &s,
        &[
            "perm",
            "set",
            "user",
            "deploy",
            "--read",
            "project:web AND env:staging",
        ],
    );
    assert_eq!(visible(&v, &s), vec!["WebStaging"]);

    v.on(
        &s,
        &[
            "perm",
            "set",
            "user",
            "deploy",
            "--read",
            "project:web OR project:api",
        ],
    );
    assert_eq!(visible(&v, &s), vec!["ApiKey", "WebProd", "WebStaging"]);

    v.on(
        &s,
        &[
            "perm",
            "set",
            "user",
            "deploy",
            "--read",
            "NOT env:production",
        ],
    );
    assert_eq!(visible(&v, &s), vec!["ApiKey", "WebStaging"]);
}

#[test]
fn an_entry_outside_the_grant_looks_exactly_like_one_that_does_not_exist() {
    let (v, s) = setup("hidden");
    v.on(
        &s,
        &["perm", "set", "user", "deploy", "--read", "project:web"],
    );
    let hidden = v.as_user(&s, "deploy", PW, &["get", "ApiKey"]);
    let missing = v.as_user(&s, "deploy", PW, &["get", "NoSuchEntry"]);
    assert_eq!(hidden.code, 3, "{}", hidden.all());
    assert_eq!(
        hidden.code, missing.code,
        "'not allowed' and 'not there' must be indistinguishable from outside"
    );
    assert_eq!(hidden.error_code(), missing.error_code());
}

#[test]
fn a_user_may_change_only_what_the_write_expression_allows() {
    let (v, s) = setup("write");
    v.on(
        &s,
        &[
            "perm",
            "set",
            "user",
            "deploy",
            "--read",
            "project:web",
            "--write",
            "project:web AND env:staging",
        ],
    );
    let ok = v.as_user(
        &s,
        "deploy",
        PW,
        &["entry", "set", "WebStaging", "--notes", "by-deploy"],
    );
    assert_eq!(ok.code, 0, "{}", ok.all());
    let denied = v.as_user(
        &s,
        "deploy",
        PW,
        &["entry", "set", "WebProd", "--notes", "by-deploy"],
    );
    assert_eq!(denied.code, 5, "{}", denied.all());
    assert_eq!(denied.error_code(), "denied");

    // The refused write changed nothing.
    let notes = v.run_with(&["--reveal"], &["get", "WebProd"]).stdout;
    assert!(!notes.contains("by-deploy"), "{notes}");
    let owner = v.on(&s, &["get", "WebStaging"]).stdout;
    assert!(owner.contains("by-deploy"), "{owner}");
    let _ = OWNER_NOTE;
}

#[test]
fn a_class_restriction_cannot_be_undone_by_an_individual_grant() {
    let (v, s) = setup("class");
    assert_eq!(v.on(&s, &["class", "add", "Deployers"]).code, 0);
    v.on(
        &s,
        &["perm", "set", "class", "Deployers", "--read", "project:web"],
    );
    v.on(&s, &["user", "class", "deploy", "Deployers"]);
    // The individual grant is wider than the class's.
    v.on(
        &s,
        &[
            "perm",
            "set",
            "user",
            "deploy",
            "--read",
            "project:web OR project:api",
        ],
    );

    assert_eq!(
        visible(&v, &s),
        vec!["WebProd", "WebStaging"],
        "class AND individual: the api entry must stay hidden"
    );
}

#[test]
fn strict_write_makes_a_write_satisfy_every_scope() {
    let (v, s) = setup("strict");
    // Two scopes joined with OR: by default a write satisfies any one of them.
    v.on(
        &s,
        &[
            "perm",
            "set",
            "user",
            "deploy",
            "--read",
            "project:web OR project:api",
            "--write",
            "project:web OR env:staging",
        ],
    );
    // WebProd is in project web (first scope) but is not staging (second scope).
    let loose = v.as_user(
        &s,
        "deploy",
        PW,
        &["entry", "set", "WebProd", "--notes", "loose"],
    );
    assert_eq!(loose.code, 0, "default is any-match: {}", loose.all());

    // Strict write scoping is set on the vault file itself (the CLI refuses it
    // through a server for now); the running server then enforces it.
    assert_eq!(v.run(&["user", "strict-write", "deploy"]).code, 0);
    let strict = v.as_user(
        &s,
        "deploy",
        PW,
        &["entry", "set", "WebProd", "--notes", "strict"],
    );
    assert_eq!(
        strict.code,
        5,
        "strict mode: must satisfy every scope: {}",
        strict.all()
    );
    // WebStaging satisfies both scopes, so it is still writable.
    let both = v.as_user(
        &s,
        "deploy",
        PW,
        &["entry", "set", "WebStaging", "--notes", "both"],
    );
    assert_eq!(both.code, 0, "{}", both.all());

    // Reads are untouched by strict mode.
    assert_eq!(visible(&v, &s), vec!["ApiKey", "WebProd", "WebStaging"]);
    assert_eq!(v.run(&["user", "strict-write", "deploy", "--off"]).code, 0);
    let relaxed = v.as_user(
        &s,
        "deploy",
        PW,
        &["entry", "set", "WebProd", "--notes", "relaxed"],
    );
    assert_eq!(
        relaxed.code,
        0,
        "turning strict mode off restores any-match: {}",
        relaxed.all()
    );
}

#[test]
fn an_expression_that_cannot_be_parsed_is_refused_and_never_stored() {
    let (v, s) = setup("badexpr");
    v.on(
        &s,
        &["perm", "set", "user", "deploy", "--read", "project:web"],
    );
    let bad = v.on(
        &s,
        &[
            "perm",
            "set",
            "user",
            "deploy",
            "--read",
            "project:web AND (",
        ],
    );
    assert_eq!(bad.code, 10, "{}", bad.all());
    let shown = v.on(&s, &["perm", "show", "user", "deploy"]).json();
    assert_eq!(
        shown["data"]["read"], "project:web",
        "the previous expression must survive"
    );
    assert_eq!(
        visible(&v, &s),
        vec!["WebProd", "WebStaging"],
        "nothing was locked out"
    );
}

#[test]
fn perm_check_parses_without_storing_anything() {
    let (v, s) = setup("permcheck");
    let ok = v.on(&s, &["perm", "check", "project:web AND NOT env:production"]);
    assert_eq!(ok.code, 0, "{}", ok.all());
    assert_eq!(ok.json()["data"]["valid"], true);
    let bad = v.on(&s, &["perm", "check", "project:web AND AND"]);
    assert_eq!(bad.code, 10, "{}", bad.all());
    assert!(
        visible(&v, &s).is_empty(),
        "perm check must not have granted anything"
    );
}

#[cfg(unix)]
#[test]
fn a_token_goes_to_a_file_not_to_stdout_and_stops_working_when_revoked() {
    use common::mode_of;
    let (v, s) = setup("token");
    v.on(
        &s,
        &["perm", "set", "user", "deploy", "--read", "project:web"],
    );
    let file = v.path("deploy.token");
    let made = v.on(
        &s,
        &[
            "user",
            "token",
            "new",
            "deploy",
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(made.code, 0, "{}", made.all());
    let token = std::fs::read_to_string(&file).unwrap().trim().to_string();
    assert!(
        token.len() >= 32,
        "a token must be long enough to be one: {token:?}"
    );
    assert!(
        !made.all().contains(&token),
        "the token reached the terminal"
    );
    assert_eq!(mode_of(&file), 0o600);

    // The token authenticates with the user's own scope.
    let list = v.with_token(&s, &token, &["list"]);
    assert_eq!(list.code, 0, "{}", list.all());
    let names: Vec<String> = list.json()["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["provider"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"WebStaging".to_string()) && !names.contains(&"ApiKey".to_string()),
        "{names:?}"
    );

    // Revoking it ends it.
    let id = made.json()["data"]["token_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        v.on(&s, &["user", "token", "revoke", "deploy", &id]).code,
        0
    );
    let after = v.with_token(&s, &token, &["list"]);
    assert_eq!(
        after.code,
        5,
        "a revoked token must be denied: {}",
        after.all()
    );
}

#[test]
fn the_output_policy_is_checked_before_a_token_is_minted() {
    let (v, s) = setup("tokenpolicy");
    // Neither --out nor --reveal: printing is refused. The first version minted the
    // token and then refused to print it, leaving a live credential nobody could read.
    let refused = v.on(&s, &["user", "token", "new", "deploy"]);
    assert_eq!(refused.code, 9, "{}", refused.all());
    let tokens = v.on(&s, &["user", "token", "ls", "deploy"]);
    let count = tokens.json()["data"]["tokens"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(
        count, 0,
        "a refused command must not leave a live token behind: {}",
        tokens.stdout
    );
}

#[test]
fn a_wrong_user_password_is_denied() {
    let (v, s) = setup("userpw");
    let o = v.as_user(&s, "deploy", "not-the-right-password", &["list"]);
    assert_eq!(o.code, 5, "{}", o.all());
    assert_eq!(o.error_code(), "denied");
}

#[test]
fn a_user_cannot_grant_itself_or_anyone_more_than_it_holds() {
    let (v, s) = setup("escalate");
    v.on(
        &s,
        &[
            "perm",
            "set",
            "user",
            "deploy",
            "--read",
            "project:web",
            "--write",
            "project:web",
        ],
    );
    for args in [
        vec!["user", "add", "sneaky", "--no-password"],
        vec!["class", "add", "Powerful", "--manage-users"],
        vec![
            "perm",
            "set",
            "user",
            "deploy",
            "--read",
            "project:web OR project:api",
        ],
    ] {
        let o = v.as_user(&s, "deploy", PW, &args);
        assert_eq!(o.code, 5, "{args:?} must be denied: {}", o.all());
    }
    // And none of it happened.
    let users: Value = v.on(&s, &["user", "ls"]).json();
    assert!(!users.to_string().contains("sneaky"), "{users}");
    assert_eq!(visible(&v, &s), vec!["WebProd", "WebStaging"]);
}
