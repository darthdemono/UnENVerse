// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The owner's side of Nodes against the local vault (R12).
//!
//! On a server the Nodes pane talks to `/api/nodes*` with a bearer session. On the desktop there is
//! no server, so the pane's same requests are answered here, from the same node registry and the
//! same audit chain, by the embedded hub. The paths, bodies and answers match the server's routes
//! for what a desktop can do; anything that needs a server (pulling a file a node uploads) answers
//! 404 with a reason instead of pretending.
//!
//! The vault must be unlocked to change anything: the key signs the audit row, and a locked vault
//! answers 503 rather than editing the registry unrecorded.

use crate::hub::HubHost;
use serde_json::{json, Value};
use vault_core::nodes::{NodeRecord, SignedApproval};

/// An HTTP-shaped answer, so the pane's existing handling of statuses applies unchanged.
pub struct Reply {
    pub status: u16,
    pub body: Value,
}

fn answer(status: u16, body: Value) -> Reply {
    Reply { status, body }
}

fn error(status: u16, msg: &str) -> Reply {
    answer(status, json!({ "error": msg }))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A node as the pane sees it: no public key, no replay counter.
fn public_view(n: &NodeRecord) -> Value {
    let mut v = serde_json::to_value(n).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.remove("pubkey");
        o.remove("last_ts_ms");
    }
    v
}

fn audit<H: HubHost>(host: &H, action: &str, who: &str, details: Value) {
    if let Some(key) = host.owner_key() {
        if let Ok(conn) = vault_core::open_db(&host.db_path(), &key) {
            let _ = vault_core::record_event(
                &conn,
                action,
                who,
                Some(&details.to_string()),
                Some("owner"),
            );
        }
    }
}

/// Answers one request. `wake` is called after a change a polling hub should act on at once.
pub fn local_request<H: HubHost>(
    host: &H,
    method: &str,
    path: &str,
    body: &Value,
    wake: &dyn Fn(),
) -> Reply {
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let locked = || error(503, "the vault is locked");
    let store_err = |e: String| error(500, &e);
    let str_of = |k: &str| {
        body.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };

    match (method, parts.as_slice()) {
        ("GET", ["api", "nodes"]) => match host.store(|s| {
            s.list()
                .into_iter()
                .map(|n| {
                    let a = s.approvals(Some(&n.id));
                    (n, a)
                })
                .collect::<Vec<_>>()
        }) {
            Ok(list) => answer(
                200,
                json!({ "nodes": list.iter().map(|(n, approvals)| {
                    let mut v = public_view(n);
                    v["approvals"] = json!(approvals.iter().take(20).collect::<Vec<_>>());
                    v
                }).collect::<Vec<_>>() }),
            ),
            Err(e) => store_err(e),
        },
        ("POST", ["api", "nodes", "tokens"]) => {
            if host.owner_key().is_none() {
                return locked();
            }
            let name = str_of("name");
            let projects: Vec<String> = body
                .get("projects")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let ttl = body
                .get("ttl_secs")
                .and_then(Value::as_i64)
                .unwrap_or(vault_core::nodes::ENROLL_TTL_SECS);
            match host.store(|s| s.mint_token(&name, projects.clone(), ttl, now_secs())) {
                Ok(Ok((token, expires))) => {
                    audit(
                        host,
                        "node.token",
                        &name,
                        json!({ "projects": projects, "expires_at": expires }),
                    );
                    // The one time the plaintext exists outside the node that will use it.
                    answer(200, json!({ "token": token, "expires_at": expires, "name": name }))
                }
                Ok(Err(e)) => error(400, &e),
                Err(e) => store_err(e),
            }
        }
        ("POST", ["api", "nodes", id, "policy"]) => {
            if host.owner_key().is_none() {
                return locked();
            }
            let approval = str_of("approval");
            match host.store(|s| s.set_approval_policy(id, &approval)) {
                Ok(Ok(n)) => {
                    audit(
                        host,
                        "node.policy",
                        &n.name,
                        json!({ "node": n.id, "approval": approval }),
                    );
                    wake();
                    answer(200, json!({ "name": n.name, "approval": approval }))
                }
                Ok(Err(e)) => error(400, &e),
                Err(e) => store_err(e),
            }
        }
        ("POST", ["api", "node-approvals", aid, verb @ ("approve" | "reject")]) => {
            if host.owner_key().is_none() {
                return locked();
            }
            let approve = *verb == "approve";
            let signed: Option<SignedApproval> = body
                .get("signed")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            // A node set to `device` is approved by a device or not at all: the app's own click
            // does not count, so a machine someone else controls cannot approve itself.
            if approve && signed.is_none() {
                let device_only = host
                    .store(|s| {
                        s.approvals(None)
                            .into_iter()
                            .find(|a| a.id == *aid)
                            .and_then(|a| s.get(&a.node_id))
                            .is_some_and(|n| n.approval == "device")
                    })
                    .unwrap_or(false);
                if device_only {
                    return error(
                        409,
                        "This node's pushes are approved on your own device: sign the approval there (unv node approve, or the app).",
                    );
                }
            }
            let r = host.store(|s| match (approve, signed.clone()) {
                (true, Some(sig)) => s.decide_signed(aid, sig, now_secs(), &vault_core::iso_now()),
                _ => s.decide(aid, approve, "owner", now_secs(), &vault_core::iso_now()),
            });
            match r {
                Ok(Ok(a)) => {
                    audit(
                        host,
                        if approve { "node.approve" } else { "node.reject" },
                        &a.node_id,
                        json!({
                            "approval": a.id, "target": a.target, "sha256": a.sha256,
                            "from_sha": a.from_sha,
                        }),
                    );
                    wake();
                    answer(200, serde_json::to_value(&a).unwrap_or_default())
                }
                Ok(Err(e)) => error(409, &e),
                Err(e) => store_err(e),
            }
        }
        ("GET", ["api", "node-approvers"]) => match host.store(|s| s.approvers()) {
            Ok(a) => answer(200, json!({ "approvers": a })),
            Err(e) => store_err(e),
        },
        ("POST", ["api", "node-approvers"]) => {
            if host.owner_key().is_none() {
                return locked();
            }
            let (pubkey, label) = (str_of("pubkey"), str_of("label"));
            match host.store(|s| s.add_approver(&pubkey, &label, &vault_core::iso_now())) {
                Ok(Ok(a)) => {
                    audit(
                        host,
                        "node.approver.add",
                        &a.label,
                        json!({ "fingerprint": a.fingerprint }),
                    );
                    answer(200, serde_json::to_value(&a).unwrap_or_default())
                }
                Ok(Err(e)) => error(400, &e),
                Err(e) => store_err(e),
            }
        }
        ("DELETE", ["api", "node-approvers", fp]) => {
            if host.owner_key().is_none() {
                return locked();
            }
            match host.store(|s| s.remove_approver(fp)) {
                Ok(Ok(a)) => {
                    audit(
                        host,
                        "node.approver.remove",
                        &a.label,
                        json!({ "fingerprint": a.fingerprint }),
                    );
                    wake();
                    answer(204, Value::Null)
                }
                Ok(Err(e)) => error(404, &e),
                Err(e) => store_err(e),
            }
        }
        ("GET", ["api", "node-automation-key"]) => match automation_key(host) {
            Ok((public, fp)) => answer(200, json!({ "public_key": public, "fingerprint": fp })),
            Err(e) if e == "locked" => locked(),
            Err(e) => store_err(e),
        },
        ("DELETE", ["api", "nodes", id]) => {
            if host.owner_key().is_none() {
                return locked();
            }
            match host.store(|s| s.revoke(id, &vault_core::iso_now())) {
                Ok(Ok(n)) => {
                    wake();
                    audit(host, "node.revoke", &n.name, json!({ "node": n.id }));
                    answer(204, Value::Null)
                }
                Ok(Err(e)) => error(404, &e),
                Err(e) => store_err(e),
            }
        }
        ("POST", ["api", "nodes", id, "lease"]) => {
            if host.owner_key().is_none() {
                return locked();
            }
            let (target, sha, holder) = (str_of("target"), str_of("sha256"), str_of("holder"));
            let ttl = body.get("ttl_secs").and_then(Value::as_i64).unwrap_or(300);
            match host.store(|s| s.lease_target(id, &target, &sha, &holder, ttl, now_secs())) {
                Ok(Ok(l)) => {
                    audit(
                        host,
                        "node.lease",
                        &l.node_id,
                        json!({ "target": l.target, "sha256": l.sha256, "holder": l.holder }),
                    );
                    answer(200, serde_json::to_value(&l).unwrap_or_default())
                }
                Ok(Err(e)) => error(409, &e),
                Err(e) => store_err(e),
            }
        }
        ("POST", ["api", "nodes", id, "lease", "release"]) => {
            if host.owner_key().is_none() {
                return locked();
            }
            let (target, holder) = (str_of("target"), str_of("holder"));
            match host.store(|s| s.release_lease(id, &target, &holder, now_secs())) {
                Ok(Ok(())) => {
                    audit(
                        host,
                        "node.lease.release",
                        id,
                        json!({ "target": target, "holder": holder }),
                    );
                    wake();
                    answer(204, Value::Null)
                }
                Ok(Err(e)) => error(409, &e),
                Err(e) => store_err(e),
            }
        }
        _ => error(
            404,
            "Not available for a local vault: pulling a node's file needs a server started with --nodes.",
        ),
    }
}

/// The public half of the automation key and its fingerprint, for a node's `automation_approver`.
/// The seed never leaves the vault. `Err("locked")` when the vault is closed.
pub fn automation_key<H: HubHost>(host: &H) -> Result<(String, String), String> {
    let key = host.owner_key().ok_or_else(|| "locked".to_string())?;
    let conn = vault_core::open_db(&host.db_path(), &key).map_err(|e| e.to_string())?;
    let seed = vault_core::nodes::automation_seed(&conn)?;
    let public = vault_core::nodes::hub_public(&seed)?;
    let fp = vault_core::nodes::key_fingerprint(&public)?;
    Ok((public, fp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use vault_core::nodes::NodeStore;

    /// A hub over a scratch registry and a scratch vault, with the vault open or locked.
    struct Host {
        dir: std::path::PathBuf,
        store: Mutex<Option<NodeStore>>,
        key: Option<vault_core::VaultKey>,
    }

    impl Host {
        fn new(tag: &str, unlocked: bool) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "unv-hubowner-{tag}-{}-{}",
                std::process::id(),
                vault_core::new_uuid()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let key =
                vault_core::derive_key("hub-owner-test-password", b"unv-hub-test-slt").unwrap();
            let conn = vault_core::open_db(&dir.join("vault.db"), &key).unwrap();
            vault_core::init_schema(&conn).unwrap();
            drop(conn);
            Self {
                store: Mutex::new(Some(NodeStore::open(&dir.join("nodes.json")).unwrap())),
                key: unlocked.then_some(key),
                dir,
            }
        }

        fn audit_actions(&self) -> Vec<String> {
            let key =
                vault_core::derive_key("hub-owner-test-password", b"unv-hub-test-slt").unwrap();
            let conn = vault_core::open_db(&self.dir.join("vault.db"), &key).unwrap();
            vault_core::load_audit(&conn)
                .unwrap()
                .into_iter()
                .map(|r| r.action)
                .collect()
        }
    }

    impl HubHost for Host {
        fn owner_key(&self) -> Option<vault_core::VaultKey> {
            self.key
        }
        fn db_path(&self) -> std::path::PathBuf {
            self.dir.join("vault.db")
        }
        fn store<R>(&self, f: impl FnOnce(&mut NodeStore) -> R) -> Result<R, String> {
            let mut g = self.store.lock().unwrap();
            Ok(f(g.as_mut().unwrap()))
        }
        fn wanted_uploads(&self, _: &str, _: &[String]) -> Vec<String> {
            Vec::new()
        }
    }

    fn call(h: &Host, m: &str, p: &str, body: Value) -> Reply {
        local_request(h, m, p, &body, &|| {})
    }

    /// Enrols a node named `name` straight into the registry and returns its id.
    fn enrol(h: &Host, name: &str) -> String {
        let (_, public) = vault_core::nodes::generate_identity();
        let t = call(
            h,
            "POST",
            "/api/nodes/tokens",
            json!({ "name": name, "projects": ["web"] }),
        );
        assert_eq!(t.status, 200, "{}", t.body);
        let token = t.body["token"].as_str().unwrap().to_string();
        h.store(|s| s.enroll(&token, &public, None, now_secs(), &vault_core::iso_now()))
            .unwrap()
            .unwrap()
            .id
    }

    #[test]
    fn minting_a_token_is_audited_and_the_node_then_appears_without_its_key() {
        let h = Host::new("mint", true);
        let id = enrol(&h, "edge-1");
        let list = call(&h, "GET", "/api/nodes", Value::Null);
        assert_eq!(list.status, 200);
        let node = &list.body["nodes"][0];
        assert_eq!(node["id"], id.as_str());
        assert!(node.get("pubkey").is_none() && node.get("last_ts_ms").is_none());
        assert!(h.audit_actions().iter().any(|a| a == "node.token"));
    }

    #[test]
    fn a_locked_vault_changes_nothing_and_says_503() {
        let h = Host::new("locked", false);
        for (m, p) in [
            ("POST", "/api/nodes/tokens"),
            ("POST", "/api/nodes/x/policy"),
            ("POST", "/api/node-approvals/a/approve"),
            ("POST", "/api/node-approvers"),
            ("DELETE", "/api/node-approvers/fp"),
            ("DELETE", "/api/nodes/x"),
            ("POST", "/api/nodes/x/lease"),
            ("GET", "/api/node-automation-key"),
        ] {
            let r = call(&h, m, p, json!({ "name": "n", "projects": ["web"] }));
            assert_eq!(r.status, 503, "{m} {p}: {}", r.body);
        }
        assert!(h.store(|s| s.list()).unwrap().is_empty());
        // Reading the registry needs no key.
        assert_eq!(call(&h, "GET", "/api/nodes", Value::Null).status, 200);
    }

    #[test]
    fn the_policy_route_validates_and_a_device_node_refuses_the_apps_own_click() {
        let h = Host::new("policy", true);
        let id = enrol(&h, "edge-2");
        let bad = call(
            &h,
            "POST",
            &format!("/api/nodes/{id}/policy"),
            json!({ "approval": "sometimes" }),
        );
        assert_eq!(bad.status, 400);
        let ok = call(
            &h,
            "POST",
            &format!("/api/nodes/{id}/policy"),
            json!({ "approval": "required" }),
        );
        assert_eq!(ok.status, 200, "{}", ok.body);
        // `device` needs a registered approver first.
        let dev = call(
            &h,
            "POST",
            &format!("/api/nodes/{id}/policy"),
            json!({ "approval": "device" }),
        );
        assert_eq!(dev.status, 400, "{}", dev.body);
        assert!(h.audit_actions().iter().any(|a| a == "node.policy"));
    }

    #[test]
    fn revoking_removes_the_node_from_use_and_is_audited() {
        let h = Host::new("revoke", true);
        let id = enrol(&h, "edge-3");
        let revoke =
            |target: &str| call(&h, "DELETE", &format!("/api/nodes/{target}"), Value::Null);
        assert_eq!(revoke(&id).status, 204);
        // Revoking again is harmless; an id that never existed is not found.
        assert_eq!(revoke(&id).status, 204);
        assert_eq!(revoke("no-such-node").status, 404);
        assert!(h.audit_actions().iter().any(|a| a == "node.revoke"));
    }

    #[test]
    fn leases_are_taken_refused_for_others_and_released_by_their_holder() {
        let h = Host::new("lease", true);
        let id = enrol(&h, "edge-4");
        let sha = "a".repeat(64);
        let take = |holder: &str| {
            call(
                &h,
                "POST",
                &format!("/api/nodes/{id}/lease"),
                json!({ "target": "nginx", "sha256": sha, "holder": holder, "ttl_secs": 60 }),
            )
        };
        assert_eq!(take("rot-1").status, 200);
        assert_eq!(take("rot-2").status, 409);
        let release = |holder: &str| {
            call(
                &h,
                "POST",
                &format!("/api/nodes/{id}/lease/release"),
                json!({ "target": "nginx", "holder": holder }),
            )
        };
        assert_eq!(release("rot-2").status, 409);
        assert_eq!(release("rot-1").status, 204);
        assert_eq!(take("rot-2").status, 200);
        let actions = h.audit_actions();
        assert!(actions.iter().any(|a| a == "node.lease"));
        assert!(actions.iter().any(|a| a == "node.lease.release"));
    }

    #[test]
    fn the_automation_key_is_public_only_and_stable() {
        let h = Host::new("autokey", true);
        let a = call(&h, "GET", "/api/node-automation-key", Value::Null);
        assert_eq!(a.status, 200, "{}", a.body);
        let b = call(&h, "GET", "/api/node-automation-key", Value::Null);
        assert_eq!(a.body, b.body, "the same key every time");
        assert_eq!(a.body["public_key"].as_str().unwrap().len(), 64);
        assert_eq!(a.body["fingerprint"].as_str().unwrap().len(), 64);
        assert!(!a.body.to_string().to_lowercase().contains("seed"));
    }

    #[test]
    fn what_needs_a_server_says_so_instead_of_failing_quietly() {
        let h = Host::new("pull", true);
        let r = call(&h, "POST", "/api/nodes/x/pull", json!({ "target": "t" }));
        assert_eq!(r.status, 404);
        assert!(r.body["error"].as_str().unwrap().contains("server"));
    }

    #[test]
    fn the_poller_starts_can_be_woken_and_stops_promptly_even_with_nothing_to_dial() {
        let h = std::sync::Arc::new(Host::new("poller", true));
        let mut p = crate::hub::Poller::spawn(h);
        p.wake();
        std::thread::sleep(std::time::Duration::from_millis(400));
        let t = std::time::Instant::now();
        p.stop();
        assert!(
            t.elapsed() < std::time::Duration::from_secs(3),
            "{:?}",
            t.elapsed()
        );
        // Stopping twice, and dropping, are harmless.
        p.stop();
    }
}
