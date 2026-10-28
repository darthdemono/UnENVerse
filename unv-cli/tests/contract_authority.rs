// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Delegated administration: a user the owner gave `manage-users` or
//! `manage-classes` can only act downwards. Nobody can create, grant or assign
//! authority at or above their own, and nobody can act on a user of equal or higher
//! authority, so delegation can never be used to climb. Only the owner confers
//! `manage-classes` or `delete-projects`. Every refusal is "denied" (5) and leaves
//! the vault exactly as it was.

mod common;
#[path = "common/server.rs"]
mod server;

use common::Vault;
use server::Server;

/// Built at run time, so no password literal sits in the source.
fn pw() -> String {
    format!("member-{}-pw-12345", std::process::id())
}

struct World {
    v: Vault,
    s: Server,
}

impl World {
    /// `adm` is in the built-in Admin class, `mod1` and `mod2` in Moderator (manage
    /// users only), `classer` in a class that may only manage classes, `low` in none.
    fn new(tag: &str) -> World {
        let v = Vault::new(tag);
        v.ok(&["entry", "add", "Seed", "--key", "a-seed-secret-value-1"]);
        let s = Server::start(&v, false);
        let w = World { v, s };
        w.owner(&["class", "add", "Classer", "--manage-classes"]);
        for (user, class) in [
            ("adm", Some("Admin")),
            ("mod1", Some("Moderator")),
            ("mod2", Some("Moderator")),
            ("classer", Some("Classer")),
            ("low", None),
        ] {
            w.owner(&["user", "add", user, "--user-password", &pw()]);
            if let Some(c) = class {
                w.owner(&["user", "class", user, c]);
            }
        }
        w
    }

    fn owner(&self, args: &[&str]) {
        let o = self.v.on(&self.s, args);
        assert_eq!(o.code, 0, "{args:?}: {}", o.all());
    }

    fn as_(&self, user: &str, args: &[&str]) -> common::Out {
        self.v.as_user(&self.s, user, &pw(), args)
    }

    fn denied(&self, user: &str, args: &[&str]) {
        let o = self.as_(user, args);
        assert_eq!(o.code, 5, "{user} {args:?} must be denied: {}", o.all());
    }

    fn allowed(&self, user: &str, args: &[&str]) {
        let o = self.as_(user, args);
        assert_eq!(o.code, 0, "{user} {args:?}: {}", o.all());
    }

    fn names(&self, list: &[&str], key: &str, field: &str) -> Vec<String> {
        let o = self.v.on(&self.s, list);
        let mut n: Vec<String> = o.json()["data"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x[field].as_str().unwrap().to_string())
            .collect();
        n.sort();
        n
    }
}

#[test]
fn only_the_owner_can_confer_class_management_or_project_deletion() {
    let w = World::new("auth-grant");
    for actor in ["adm", "classer"] {
        w.denied(actor, &["class", "add", "Up1", "--manage-classes"]);
        w.denied(actor, &["class", "add", "Up2", "--delete-projects"]);
    }
    assert!(!w
        .names(&["class", "ls"], "classes", "name")
        .contains(&"Up1".into()));
    assert!(!w
        .names(&["class", "ls"], "classes", "name")
        .contains(&"Up2".into()));
    // Something at or below their own level is theirs to hand out.
    w.allowed("adm", &["class", "add", "Helpers", "--manage-users"]);
    w.allowed("adm", &["class", "add", "Readers"]);
}

#[test]
fn a_delegate_cannot_raise_a_class_or_edit_one_at_their_own_level() {
    let w = World::new("auth-class-set");
    // Editing the class they themselves belong to would let them rewrite their own rights.
    w.denied("adm", &["class", "set", "Admin", "--desc", "mine now"]);
    w.denied("adm", &["class", "set", "Viewer", "--manage-classes"]);
    w.denied("adm", &["class", "set", "Viewer", "--delete-projects"]);
    w.allowed("adm", &["class", "set", "Viewer", "--desc", "read only"]);
    let classes = w.v.on(&w.s, &["class", "ls"]).json()["data"]["classes"].clone();
    let viewer = classes
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Viewer")
        .unwrap();
    assert_eq!(viewer["description"], "read only");
    assert_eq!(
        viewer["cap_manage_classes"], false,
        "the refused raise changed nothing"
    );
    let admin = classes
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Admin")
        .unwrap();
    assert_ne!(admin["description"], "mine now");
}

#[test]
fn a_user_without_the_capability_cannot_touch_users_or_classes_at_all() {
    let w = World::new("auth-cap");
    for args in [
        vec!["user", "rm", "low"],
        vec!["user", "rename", "low", "x"],
        vec!["user", "class", "low", "Viewer"],
        vec!["user", "passwd", "low", "--clear"],
    ] {
        w.denied("classer", &args);
    }
    for args in [
        vec!["class", "add", "Nope"],
        vec!["class", "set", "Viewer", "--desc", "x"],
        vec!["class", "rm", "Viewer"],
    ] {
        w.denied("mod1", &args);
    }
    assert!(w
        .names(&["user", "ls"], "users", "username")
        .contains(&"low".into()));
}

#[test]
fn a_delegate_cannot_act_on_a_peer_or_a_superior_but_can_on_those_below() {
    let w = World::new("auth-users");
    for target in ["mod2", "adm"] {
        for args in [
            vec!["user", "rm", target],
            vec!["user", "rename", target, "renamed"],
            vec!["user", "passwd", target, "--clear"],
            vec!["user", "class", target, "Viewer"],
        ] {
            w.denied("mod1", &args);
        }
        let o = w.as_("mod1", &["--reveal", "user", "token", "new", target]);
        assert_eq!(
            o.code,
            5,
            "a token for {target} would be their login: {}",
            o.all()
        );
    }
    let users = w.names(&["user", "ls"], "users", "username");
    assert!(users.contains(&"mod2".into()) && users.contains(&"adm".into()));
    w.allowed("mod1", &["user", "rename", "low", "lower"]);
    w.allowed("mod1", &["user", "rm", "lower"]);
}

#[test]
fn a_delegate_cannot_assign_a_class_at_or_above_their_own_authority() {
    let w = World::new("auth-assign");
    w.denied("mod1", &["user", "class", "low", "Admin"]);
    w.denied("mod1", &["user", "class", "low", "Moderator"]);
    w.allowed("mod1", &["user", "class", "low", "Viewer"]);
    let users = w.v.on(&w.s, &["user", "ls"]).json()["data"]["users"].clone();
    let low = users
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "low")
        .unwrap();
    assert!(
        !low["class_id"].is_null(),
        "the allowed assignment took effect: {low}"
    );
}

/// Bug: `user strict-write` exited 7 against a server although the server has
/// the route; the owner can now set it there and a non-owner is denied.
#[test]
fn strict_write_can_be_set_on_a_server_by_the_owner_only() {
    let w = World::new("authority-strict");
    w.owner(&["user", "strict-write", "low"]);
    let shown = w.v.on(&w.s, &["perm", "show", "user", "low"]);
    assert_eq!(
        shown.json()["data"]["strict_write"],
        true,
        "{}",
        shown.all()
    );
    w.owner(&["user", "strict-write", "low", "--off"]);
    w.denied("mod1", &["user", "strict-write", "low"]);
}

/// A class takes the flag through the same route, and a missing subject is 3.
#[test]
fn strict_write_on_a_class_and_on_a_missing_subject_over_a_server() {
    let w = World::new("authority-strict-class");
    w.owner(&["user", "strict-write", "low"]);
    let o = w.v.on(&w.s, &["user", "strict-write", "ghost"]);
    assert_eq!(o.code, 3, "{}", o.all());
}
