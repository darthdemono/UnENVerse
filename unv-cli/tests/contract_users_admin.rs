//! User administration ("Multiple users", "Users and TOTP", README), through the
//! real `unv` against a real `unv-server`:
//!
//! * classes: created with capability flags, listed, changed, removed, and
//!   unknown or duplicate names are refused with the right codes;
//! * users: created, renamed, listed (never with a password hash) and removed,
//!   and removing one ends its tokens;
//! * a user's second factor is two-phase: `enroll` mints a secret without
//!   enabling anything, `confirm` with a live code enables it, a password login
//!   then needs a fresh code (a used one cannot be replayed), a token skips it,
//!   and `disable` returns to password-only. The owner cannot enrol.

mod common;
#[path = "common/server.rs"]
mod server;

use common::Vault;
use server::Server;

const PW: &str = "member-password-12345";

fn hub(tag: &str) -> (Vault, Server) {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "Seed", "--key", "a-seed-secret-value-1"]);
    let s = Server::start(&v, false);
    (v, s)
}

/// Classes and users that exist on a fresh server (`Admin`, `Moderator`, `Viewer`,
/// and the `admin` and `owner` accounts) are not what these tests are about.
const BUILT_IN_CLASSES: [&str; 3] = ["Admin", "Moderator", "Viewer"];
const BUILT_IN_USERS: [&str; 2] = ["admin", "owner"];

fn class_names(v: &Vault, s: &Server) -> Vec<String> {
    let o = v.on(s, &["class", "ls"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let mut n: Vec<String> = o.json()["data"]["classes"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| c["name"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();
    n.retain(|c| !BUILT_IN_CLASSES.contains(&c.as_str()));
    n.sort();
    n
}

fn user_names(v: &Vault, s: &Server) -> Vec<String> {
    let o = v.on(s, &["user", "ls"]);
    assert_eq!(o.code, 0, "{}", o.all());
    let mut n: Vec<String> = o.json()["data"]["users"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|u| u["username"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();
    n.retain(|u| !BUILT_IN_USERS.contains(&u.as_str()));
    n.sort();
    n
}

// ---- classes

#[test]
fn a_class_is_created_listed_changed_and_removed() {
    let (v, s) = hub("class");
    let made = v.on(
        &s,
        &[
            "class",
            "add",
            "Deployers",
            "--desc",
            "CI deploy accounts",
            "--manage-users",
        ],
    );
    assert_eq!(made.code, 0, "{}", made.all());
    assert_eq!(class_names(&v, &s), vec!["Deployers"]);
    let ls = v.on(&s, &["class", "ls"]).stdout;
    assert!(ls.contains("CI deploy accounts"), "{ls}");

    let changed = v.on(
        &s,
        &[
            "class",
            "set",
            "Deployers",
            "--name",
            "Releasers",
            "--desc",
            "release accounts",
        ],
    );
    assert_eq!(changed.code, 0, "{}", changed.all());
    assert_eq!(class_names(&v, &s), vec!["Releasers"]);

    let removed = v.on(&s, &["class", "rm", "Releasers"]);
    assert_eq!(removed.code, 0, "{}", removed.all());
    assert!(class_names(&v, &s).is_empty());
}

#[test]
fn a_duplicate_class_is_a_conflict_and_an_unknown_one_is_not_found() {
    let (v, s) = hub("classerr");
    v.on(&s, &["class", "add", "Deployers"]);
    let dup = v.on(&s, &["class", "add", "Deployers"]);
    assert_eq!(dup.code, 6, "{}", dup.all());
    for args in [
        vec!["class", "set", "Nope", "--desc", "x"],
        vec!["class", "rm", "Nope"],
    ] {
        let o = v.on(&s, &args);
        assert_eq!(o.code, 3, "{args:?}: {}", o.all());
    }
    assert_eq!(class_names(&v, &s), vec!["Deployers"]);
}

#[test]
fn a_class_with_members_is_not_removed_until_it_is_empty() {
    let (v, s) = hub("classrm");
    v.on(&s, &["class", "add", "Deployers"]);
    v.on(&s, &["user", "add", "deploy", "--user-password", PW]);
    v.on(&s, &["user", "class", "deploy", "Deployers"]);
    let refused = v.on(&s, &["class", "rm", "Deployers"]);
    assert_eq!(
        refused.code,
        6,
        "removing a class people belong to is refused: {}",
        refused.all()
    );
    assert_eq!(
        class_names(&v, &s),
        vec!["Deployers"],
        "a refused removal changes nothing"
    );
    assert_eq!(user_names(&v, &s), vec!["deploy"]);

    assert_eq!(v.on(&s, &["user", "class", "deploy", "--none"]).code, 0);
    assert_eq!(v.on(&s, &["class", "rm", "Deployers"]).code, 0);
    assert!(class_names(&v, &s).is_empty());
    assert_eq!(
        user_names(&v, &s),
        vec!["deploy"],
        "the member itself was never deleted"
    );
}

#[test]
fn user_class_none_removes_the_class_assignment() {
    let (v, s) = hub("classnone");
    v.on(&s, &["class", "add", "Deployers"]);
    v.on(&s, &["user", "add", "deploy", "--user-password", PW]);
    assert_eq!(v.on(&s, &["user", "class", "deploy", "Deployers"]).code, 0);
    assert_eq!(v.on(&s, &["user", "class", "deploy", "--none"]).code, 0);
    let unknown = v.on(&s, &["user", "class", "deploy", "NoSuchClass"]);
    assert_eq!(unknown.code, 3, "{}", unknown.all());
}

// ---- users

#[test]
fn users_are_created_listed_renamed_and_removed_and_a_list_never_shows_a_hash() {
    let (v, s) = hub("users");
    assert_eq!(
        v.on(&s, &["user", "add", "alice", "--user-password", PW])
            .code,
        0
    );
    assert_eq!(v.on(&s, &["user", "add", "bob", "--no-password"]).code, 0);
    assert_eq!(user_names(&v, &s), vec!["alice", "bob"]);

    let dup = v.on(&s, &["user", "add", "alice", "--user-password", PW]);
    assert_eq!(dup.code, 6, "{}", dup.all());

    let listing = v.on(&s, &["user", "ls"]);
    assert!(
        !listing.all().contains("$argon2"),
        "a password hash must never be listed: {}",
        listing.all()
    );
    assert!(!listing.all().contains(PW));

    assert_eq!(v.on(&s, &["user", "rename", "alice", "alicia"]).code, 0);
    assert_eq!(user_names(&v, &s), vec!["alicia", "bob"]);
    let clash = v.on(&s, &["user", "rename", "alicia", "bob"]);
    assert_ne!(
        clash.code,
        0,
        "renaming onto an existing user is refused: {}",
        clash.all()
    );

    assert_eq!(v.on(&s, &["user", "rm", "bob"]).code, 0);
    assert_eq!(user_names(&v, &s), vec!["alicia"]);
    assert_eq!(
        v.on(&s, &["user", "rm", "bob"]).code,
        3,
        "removing twice is not_found"
    );
}

#[test]
fn removing_a_user_ends_its_tokens_and_its_login() {
    let (v, s) = hub("userrm");
    v.on(&s, &["user", "add", "alice", "--user-password", PW]);
    v.on(&s, &["perm", "set", "user", "alice", "--read", "vault:*"]);
    let file = v.path("alice.token");
    assert_eq!(
        v.on(
            &s,
            &[
                "user",
                "token",
                "new",
                "alice",
                "--out",
                file.to_str().unwrap()
            ]
        )
        .code,
        0
    );
    let token = std::fs::read_to_string(&file).unwrap().trim().to_string();
    assert_eq!(v.with_token(&s, &token, &["list"]).code, 0);

    assert_eq!(v.on(&s, &["user", "rm", "alice"]).code, 0);
    assert_eq!(
        v.with_token(&s, &token, &["list"]).code,
        5,
        "a removed user's token must stop working"
    );
    assert_eq!(
        v.as_user(&s, "alice", PW, &["list"]).code,
        5,
        "and so must its password"
    );
}

#[test]
fn an_owner_scoped_command_by_an_unknown_user_is_not_found() {
    let (v, s) = hub("userunknown");
    for args in [
        vec!["user", "rm", "ghost"],
        vec!["user", "rename", "ghost", "x"],
        vec!["perm", "show", "user", "ghost"],
    ] {
        let o = v.on(&s, &args);
        assert_eq!(o.code, 3, "{args:?}: {}", o.all());
    }
}

#[test]
fn perm_show_reports_the_stored_and_the_effective_expression() {
    let (v, s) = hub("permshow");
    v.on(&s, &["user", "add", "alice", "--user-password", PW]);
    v.on(
        &s,
        &[
            "perm",
            "set",
            "user",
            "alice",
            "--read",
            "project:web OR project:api",
            "--write",
            "project:web",
        ],
    );
    let shown = v.on(&s, &["perm", "show", "user", "alice"]).json();
    assert_eq!(
        shown["data"]["read"], "(project:web OR project:api)",
        "stored in its normalised form: {shown}"
    );
    assert!(
        shown["data"]["write"]
            .as_str()
            .unwrap()
            .contains("project:web"),
        "{shown}"
    );
    assert_eq!(shown["data"]["strict_write"], false);
    // Clearing an expression denies everything (an empty value is "no grant").
    v.on(&s, &["perm", "set", "user", "alice", "--read", ""]);
    let cleared = v.on(&s, &["perm", "show", "user", "alice"]).json();
    assert_eq!(cleared["data"]["read"], "", "{cleared}");
}

// ---- second factor

/// A code from a seed the test also holds as an ordinary entry, so the CLI itself
/// computes it. `next` skips the window a confirm already used (anti-replay).
fn code(v: &Vault, seed_entry: &str, next: bool) -> String {
    let mut a = vec!["totp", "code", seed_entry];
    if next {
        a.push("--next");
    }
    let d = v.ok(&a);
    if next {
        d["next_code"].as_str().unwrap().to_string()
    } else {
        d["code"].as_str().unwrap().to_string()
    }
}

fn enroll(v: &Vault, s: &Server, user: &str) -> String {
    let file = v.path(&format!("{user}.totp"));
    let o = v.on(
        s,
        &[
            "user",
            "totp",
            "enroll",
            user,
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    let body = std::fs::read_to_string(&file).unwrap();
    let secret = body.lines().next().unwrap().trim().to_string();
    assert!(
        !o.all().contains(&secret),
        "the seed must not be printed: {}",
        o.all()
    );
    assert!(
        body.contains("otpauth://"),
        "the file also carries the enrolment URI: {body}"
    );
    secret
}

#[test]
fn enrolling_mints_a_seed_without_enabling_the_factor_and_confirm_turns_it_on() {
    let (v, s) = hub("totpflow");
    v.on(&s, &["user", "add", "alice", "--user-password", PW]);
    v.on(&s, &["perm", "set", "user", "alice", "--read", "vault:*"]);
    let status = |v: &Vault| v.on(&s, &["user", "totp", "status", "alice"]).json()["data"].clone();
    let before = status(&v);
    assert_eq!(before["enabled"], false, "{before}");

    let secret = enroll(&v, &s, "alice");
    let mid = status(&v);
    assert_eq!(
        mid["enabled"], false,
        "enrolling must not enable it, or a bad scan locks the user out: {mid}"
    );
    assert_eq!(
        v.as_user(&s, "alice", PW, &["list"]).code,
        0,
        "password login still works"
    );

    v.ok(&[
        "entry",
        "add",
        "AliceSeed",
        "--key",
        "unused-value-123456",
        "--totp",
        &secret,
    ]);
    let confirm = v.on(
        &s,
        &[
            "user",
            "totp",
            "confirm",
            "alice",
            &code(&v, "AliceSeed", false),
        ],
    );
    assert_eq!(confirm.code, 0, "{}", confirm.all());
    assert_eq!(status(&v)["enabled"], true);

    // A password login now needs a code, and a wrong one is denied.
    let missing = v.as_user(&s, "alice", PW, &["list"]);
    assert_eq!(missing.code, 5, "{}", missing.all());
    let wrong = v.as_user(&s, "alice", PW, &["--totp", "000000", "list"]);
    assert_eq!(wrong.code, 5, "{}", wrong.all());
    // The window the confirm used cannot be replayed; the next one works.
    let fresh = code(&v, "AliceSeed", true);
    let ok = v.as_user(&s, "alice", PW, &["--totp", &fresh, "list"]);
    assert_eq!(ok.code, 0, "{}", ok.all());
    let replay = v.as_user(&s, "alice", PW, &["--totp", &fresh, "list"]);
    assert_eq!(
        replay.code,
        5,
        "a code that has been used is refused: {}",
        replay.all()
    );
}

#[test]
fn a_token_skips_the_second_factor_and_disable_returns_to_password_only() {
    let (v, s) = hub("totptoken");
    v.on(&s, &["user", "add", "alice", "--user-password", PW]);
    v.on(&s, &["perm", "set", "user", "alice", "--read", "vault:*"]);
    let secret = enroll(&v, &s, "alice");
    v.ok(&[
        "entry",
        "add",
        "AliceSeed",
        "--key",
        "unused-value-123456",
        "--totp",
        &secret,
    ]);
    v.on(
        &s,
        &[
            "user",
            "totp",
            "confirm",
            "alice",
            &code(&v, "AliceSeed", false),
        ],
    );

    // CI has no human to read a code, so a token deliberately skips it.
    let file = v.path("alice.token");
    v.on(
        &s,
        &[
            "user",
            "token",
            "new",
            "alice",
            "--out",
            file.to_str().unwrap(),
        ],
    );
    let token = std::fs::read_to_string(&file).unwrap().trim().to_string();
    assert_eq!(v.with_token(&s, &token, &["list"]).code, 0);

    let off = v.on(&s, &["user", "totp", "disable", "alice"]);
    assert_eq!(off.code, 0, "{}", off.all());
    assert_eq!(
        v.on(&s, &["user", "totp", "status", "alice"]).json()["data"]["enabled"],
        false
    );
    assert_eq!(
        v.as_user(&s, "alice", PW, &["list"]).code,
        0,
        "password-only again"
    );
}

#[test]
fn the_owner_cannot_enrol_a_second_factor_and_a_wrong_confirm_changes_nothing() {
    let (v, s) = hub("totpowner");
    let owner = v.on(&s, &["user", "totp", "enroll", "owner"]);
    assert_ne!(
        owner.code,
        0,
        "TOTP is refused for the owner (the master password is the proof): {}",
        owner.all()
    );

    v.on(&s, &["user", "add", "alice", "--user-password", PW]);
    enroll(&v, &s, "alice");
    let bad = v.on(&s, &["user", "totp", "confirm", "alice", "000000"]);
    assert_ne!(bad.code, 0, "{}", bad.all());
    assert_eq!(
        v.on(&s, &["user", "totp", "status", "alice"]).json()["data"]["enabled"],
        false
    );
}

#[test]
fn a_class_with_members_cannot_be_deleted_from_the_local_cli_either() {
    // Bug it pins: locally, `class rm` unassigned the members, and a member with a
    // broad expression of their own became unrestricted. The server already refused.
    let v = Vault::new("classrmlocal");
    v.ok(&["class", "add", "Deployers"]);
    v.ok(&["user", "add", "deploy", "--user-password", PW]);
    v.ok(&["user", "class", "deploy", "Deployers"]);
    let refused = v.run(&["class", "rm", "Deployers"]);
    assert_eq!(refused.code, 6, "{}", refused.all());
    assert!(refused.all().contains("still has 1 member"), "{}", refused.all());
    v.ok(&["user", "class", "deploy", "--none"]);
    assert_eq!(v.run(&["class", "rm", "Deployers"]).code, 0);
}
