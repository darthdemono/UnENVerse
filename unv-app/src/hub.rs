// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The hub half of Nodes, shared by `unv-server` and the desktop (R12).
//!
//! Deciding what a node is sent is the security-sensitive part of Nodes: which bytes are rendered
//! for it, whether a person has said yes to exactly those bytes, whether an automation rule may
//! stand in, and what is recorded. That logic lives here once. A host supplies only the things that
//! differ: where the owner's vault key comes from (a session on the server, the unlocked app on the
//! desktop), where the vault file and the node registry are, and which pull uploads are wanted.
//!
//! [`process`] answers one node beat; [`poll_node`] dials one listening node (the desktop's normal
//! mode, so it needs no inbound port) and answers its beat.

use base64::Engine;
use std::collections::HashMap;
use vault_core::nodes::{
    Action, ApplyResult, Beat, BeatReply, NodeRecord, NodeStore, TargetReport, LOCKED,
};
use vault_core::{load_vault, open_db, VaultKey};

/// Pushes per beat. A node catches up over several beats; one huge reply is the thing a slow link
/// and a short timeout turn into a retry loop.
pub const MAX_PUSHES: usize = 16;

/// What a host provides to the shared hub logic.
pub trait HubHost {
    /// The owner's vault key while the vault is open and the hub may use it, else `None` (locked).
    fn owner_key(&self) -> Option<VaultKey>;
    /// The vault database the hub reads and records into.
    fn db_path(&self) -> std::path::PathBuf;
    /// Runs `f` on the node registry, opening it on first use.
    fn store<R>(&self, f: impl FnOnce(&mut NodeStore) -> R) -> Result<R, String>;
    /// Of `pull_targets` (target ids of `node_id`), the ones an owner has asked to receive.
    fn wanted_uploads(&self, node_id: &str, pull_targets: &[String]) -> Vec<String>;
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn short(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn audit(
    db_path: &std::path::Path,
    key: &VaultKey,
    action: &str,
    who: &str,
    details: serde_json::Value,
    actor: &str,
) {
    if let Ok(conn) = open_db(db_path, key) {
        let _ =
            vault_core::record_event(&conn, action, who, Some(&details.to_string()), Some(actor));
    }
}

/// What the hub would write for each target of this node, or why it will not.
struct Plan {
    /// target id -> rendered bytes, for the targets it will render.
    rendered: HashMap<String, Result<Vec<u8>, String>>,
}

fn plan(vault: Option<&serde_json::Value>, node: &NodeRecord, targets: &[TargetReport]) -> Plan {
    let mut rendered = HashMap::new();
    let Some(vault) = vault else {
        for t in targets.iter().filter(|t| t.mode == "push") {
            rendered.insert(t.id.clone(), Err(LOCKED.to_string()));
        }
        return Plan { rendered };
    };
    let names = crate::check_cmd::vault_names(vault);
    let mut cache: HashMap<(String, String), Result<Vec<u8>, String>> = HashMap::new();
    for t in targets.iter().filter(|t| t.mode == "push") {
        if !node.projects.iter().any(|p| p == &t.project) {
            continue; // record_beat words this refusal itself
        }
        let r = cache
            .entry((t.project.clone(), t.exporter.clone()))
            .or_insert_with(|| render_gated(vault, &names, &t.project, &t.exporter))
            .clone();
        rendered.insert(t.id.clone(), r);
    }
    Plan { rendered }
}

/// The one project a node grant names. A grant is a project NAME, but `find_project_index` also
/// matches ids and falls back to substrings, so a writer who may add projects could add one whose
/// id equals the granted name and have the node served its content. Here the grant must match
/// exactly one project, by id or by name; two candidates is refused, never guessed.
pub fn resolve_grant(vault: &serde_json::Value, grant: &str) -> Result<usize, String> {
    let list = crate::data::projects(vault);
    let hits: Vec<usize> = list
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.get("id").and_then(|v| v.as_str()) == Some(grant)
                || p.get("name").and_then(|v| v.as_str()) == Some(grant)
        })
        .map(|(i, _)| i)
        .collect();
    match hits.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!("No project named '{grant}'")),
        _ => Err(format!(
            "'{grant}' names more than one project (an id and a name, or a duplicate); \
             refusing to choose. Rename one of them."
        )),
    }
}

/// Renders one project for a node, after the config compiler has looked at it.
///
/// The compiler's error findings are exactly "the generated config is wrong",
/// and a node is about to write that config to a live host. A warning does not
/// stop a push; it is something a human can read in `unv check`.
pub fn render_gated(
    vault: &serde_json::Value,
    names: &[String],
    project: &str,
    exporter: &str,
) -> Result<Vec<u8>, String> {
    let list = crate::data::projects(vault);
    let idx = resolve_grant(vault, project)?;
    // Render by the resolved project's id, so the second lookup cannot land elsewhere.
    let id = list[idx]
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or(project)
        .to_string();
    let errors = vault_core::config_check::gate(&list[idx], names);
    if let Some(f) = errors.first() {
        return Err(format!(
            "config check: {} ({} error{} in project '{project}')",
            f.message,
            errors.len(),
            if errors.len() == 1 { "" } else { "s" }
        ));
    }
    crate::chunks::render_project(vault, &id, exporter)
        .map(String::into_bytes)
        .map_err(|e| e.to_string())
}

pub fn record_results(
    db_path: &std::path::Path,
    key: &VaultKey,
    node: &NodeRecord,
    results: &[ApplyResult],
) {
    let Ok(conn) = open_db(db_path, key) else {
        return;
    };
    for r in results.iter().take(64) {
        let details = serde_json::json!({
            "node": node.id,
            "target": short(&r.target, 63),
            "at": short(&r.at, 40),
            "sha256": short(&r.sha256, 64),
            "ok": r.ok,
            "error": r.error.as_deref().map(|e| short(e, 400)),
            "phases": r.phases.iter().take(8).map(|p| short(p, 16)).collect::<Vec<_>>(),
        })
        .to_string();
        // The node resends a result until the hub acknowledges it; when that reply is lost
        // the same result arrives again and must not become a second audit row.
        let _ = vault_core::record_event_once(
            &conn,
            "node.apply",
            &node.name,
            &details,
            Some(&format!("node:{}", node.name)),
        );
    }
}

/// One pass of beat handling: audit, plan, record, and decide the actions.
pub fn process<H: HubHost>(host: &H, node: &NodeRecord, beat: &Beat, first: bool) -> BeatReply {
    let db_path = host.db_path();
    let key = host.owner_key();
    let mut reply = BeatReply {
        interval_secs: 60,
        hub_locked: key.is_none(),
        results_ack: false,
        actions: Vec::new(),
        pending: Vec::new(),
        hub_pubkey: None,
        hub_epoch: None,
    };

    let conn = key.as_ref().and_then(|k| open_db(&db_path, k).ok());
    // Which instance of the vault this hub is (R19). A standby copy does not push, does not write
    // the audit chain and tells the node it is locked; the node keeps observing and tries again.
    if let Some(id) = conn
        .as_ref()
        .and_then(|c| vault_core::identity::ensure(c).ok())
    {
        reply.hub_epoch = Some(vault_core::nodes::HubEpoch {
            vault_id: id.vault_id,
            epoch: id.epoch,
        });
        if id.role == vault_core::identity::Role::Standby {
            reply.hub_locked = true;
            return reply;
        }
    }
    // The hub's approval-signing key lives in the vault's own (encrypted) meta
    // table, so it exists only while the vault is open, like the push it signs.
    let hub_seed = conn
        .as_ref()
        .and_then(|c| vault_core::nodes::hub_seed(c).ok());
    reply.hub_pubkey = hub_seed
        .as_deref()
        .and_then(|s| vault_core::nodes::hub_public(s).ok());
    // The key that signs rule approvals (R12). Its public half is what an operator puts in a node's
    // `automation_approver`; it never signs for a node that needs a person.
    let automation_seed = conn
        .as_ref()
        .and_then(|c| vault_core::nodes::automation_seed(c).ok());
    // Read the policy now, not from the clone taken when the request arrived: a
    // held beat that is woken by an approval must see what the owner set.
    let requires_approval = host
        .store(|s| s.get(&node.id))
        .ok()
        .flatten()
        .is_some_and(|n| matches!(n.approval.as_str(), "required" | "device"));
    let vault = conn.as_ref().and_then(|c| load_vault(c).ok().flatten());
    if let Some(k) = &key {
        if first {
            record_results(&db_path, k, node, &beat.results);
        }
        reply.results_ack = true;
    }

    // A grant names a project; once the project is gone the grant goes with it, so
    // a project created later under the same name is not inherited.
    if let Some(v) = &vault {
        // A grant may name a project by id or by name.
        let names: Vec<String> = crate::data::projects(v)
            .iter()
            .flat_map(|p| {
                ["id", "name"].map(|k| p.get(k).and_then(|x| x.as_str()).map(String::from))
            })
            .flatten()
            .collect();
        if let Ok(Ok(gone)) = host.store(|s| s.prune_projects(&names)) {
            if let (Some(k), true) = (&key, first) {
                for (node_name, project) in gone {
                    audit(
                        &db_path,
                        k,
                        "node.project.pruned",
                        &node_name,
                        serde_json::json!({ "project": project }),
                        "hub",
                    );
                }
            }
        }
    }
    let plan = plan(vault.as_ref(), node, &beat.targets);
    let desired = |t: &TargetReport| -> Result<String, String> {
        match plan.rendered.get(&t.id) {
            Some(Ok(b)) => Ok(vault_core::nodes_apply::sha256_hex(b)),
            Some(Err(e)) => Err(e.clone()),
            None => Err(LOCKED.to_string()),
        }
    };
    // A node that needs a human gets a request for each set of bytes it is about
    // to be sent, with the proposal recorded in the history so the human reads it
    // as a diff. The request exists before the status is computed, so the very
    // beat that notices a change already shows it as awaiting approval.
    if requires_approval {
        if let (Some(c), Some(v)) = (&conn, &vault) {
            for t in beat.targets.iter().filter(|t| t.mode == "push" && t.apply) {
                let Some(Ok(bytes)) = plan.rendered.get(&t.id) else {
                    continue;
                };
                let sha = vault_core::nodes_apply::sha256_hex(bytes);
                if t.sha256.as_deref() == Some(sha.as_str()) {
                    continue;
                }
                let known = host
                    .store(|s| s.approval_for(&node.id, &t.id, &sha))
                    .ok()
                    .flatten()
                    .is_some_and(|a| {
                        matches!(a.status.as_str(), "pending" | "approved" | "rejected")
                    });
                if known {
                    continue;
                }
                let actor = format!("node:{}", node.name);
                let to_seq = crate::history::snapshot_stream(
                    c,
                    v,
                    &t.project,
                    &t.exporter,
                    &format!("proposed:{}", node.name),
                    Some(&actor),
                )
                .ok()
                .flatten();
                let to_seq = reviewable_seq(c, to_seq, &sha);
                let from_seq = t.sha256.as_deref().and_then(|h| {
                    vault_core::config_history::find_by_sha(c, h, 1)
                        .ok()
                        .and_then(|m| m.first().map(|x| x.seq))
                });
                let made = host.store(|s| {
                    s.request_approval(
                        &node.id,
                        t,
                        &sha,
                        from_seq,
                        to_seq,
                        now_secs(),
                        &vault_core::iso_now(),
                    )
                });
                if let Ok(Ok(rec)) = made {
                    if first {
                        if let Some(k) = &key {
                            audit(
                                &db_path,
                                k,
                                "node.approval.request",
                                &node.name,
                                serde_json::json!({
                                    "approval": rec.id, "target": t.id,
                                    "sha256": sha, "from_sha": rec.from_sha,
                                }),
                                &actor,
                            );
                        }
                    }
                }
            }
        }
    }
    let rec = host.store(|s| s.record_beat(&node.id, beat, &desired, &vault_core::iso_now()));
    if !matches!(rec, Ok(Ok(()))) {
        return reply;
    }

    for t in &beat.targets {
        if reply.actions.len() >= MAX_PUSHES {
            break;
        }
        if t.mode == "push" && t.apply {
            if let Some(Ok(bytes)) = plan.rendered.get(&t.id) {
                let sha = vault_core::nodes_apply::sha256_hex(bytes);
                if t.sha256.as_deref() != Some(sha.as_str()) {
                    // A lease pins the target to the bytes a rotation is deploying: any other
                    // rendering (a save from a stale window, say) waits until it ends.
                    if let Ok(Some(lease)) =
                        host.store(|s| s.lease_for(&node.id, &t.id, now_secs()))
                    {
                        if lease.sha256 != sha {
                            continue;
                        }
                    }
                    // What is about to leave this hub is recorded first, so the
                    // history (and blast radius with it) accounts for every file a
                    // host was ever sent. A no-op when a save already recorded it.
                    if let (Some(c), Some(v)) = (&conn, &vault) {
                        let _ = crate::history::snapshot_stream(
                            c,
                            v,
                            &t.project,
                            &t.exporter,
                            &format!("push:{}", node.name),
                            Some(&format!("node:{}", node.name)),
                        );
                    }
                    // A node that needs a human is sent nothing until there is a yes
                    // for exactly these bytes, and then it is sent that yes, signed.
                    let approval = if requires_approval {
                        let usable = host
                            .store(|s| s.usable_approval(&node.id, &t.id, &sha, now_secs()))
                            .ok()
                            .flatten();
                        let Some(a) = usable else {
                            let rec = host
                                .store(|s| s.approval_for(&node.id, &t.id, &sha))
                                .ok()
                                .flatten();
                            reply.pending.push(vault_core::nodes::PendingNote {
                                target: t.id.clone(),
                                sha256: sha.clone(),
                                status: rec
                                    .as_ref()
                                    .map_or("awaiting_approval", |r| r.status.as_str())
                                    .to_string(),
                                approval_id: rec.map(|r| r.id).unwrap_or_default(),
                            });
                            continue;
                        };
                        // Signed on the owner's device: the hub sends exactly that and
                        // holds no key that could have made it. A `device` node is
                        // never sent a hub-made approval, whatever the record says.
                        if let Some(signed) = a.signed.clone() {
                            reply.actions.push(Action::Push {
                                target: t.id.clone(),
                                sha256: sha,
                                content_b64: base64::engine::general_purpose::STANDARD
                                    .encode(bytes),
                                approval: Some(signed),
                            });
                            continue;
                        }
                        if node.approval == "device" {
                            continue;
                        }
                        let Some(seed) = &hub_seed else {
                            continue;
                        };
                        let token = vault_core::nodes::ApprovalToken {
                            v: 1,
                            node_id: node.id.clone(),
                            target: t.id.clone(),
                            sha256: sha.clone(),
                            approval_id: a.id.clone(),
                            approved_by: a.decided_by.clone().unwrap_or_default(),
                            approved_at: a.decided_at.clone().unwrap_or_default(),
                            expires_secs: a.decided_secs.unwrap_or(0)
                                + vault_core::nodes::APPROVAL_TTL_SECS,
                            // One approval, one nonce: the node remembers it once it has
                            // written, so the same yes cannot overwrite a later local edit.
                            nonce: a.id.clone(),
                        };
                        vault_core::nodes::sign_approval(seed, &token).ok()
                    } else if t.accept_rule_approval {
                        // A node that asked for an automation rule gets one: short-lived, for
                        // exactly these bytes and this operation, signed by the automation key.
                        automation_seed.as_deref().and_then(|seed| {
                            let now = now_secs();
                            vault_core::nodes::sign_approval_v2(
                                seed,
                                &vault_core::nodes::ApprovalTokenV2 {
                                    v: 2,
                                    kind: "rule".into(),
                                    node_id: node.id.clone(),
                                    target: t.id.clone(),
                                    sha256: sha.clone(),
                                    approval_id: vault_core::new_uuid(),
                                    operation_id: vault_core::new_uuid(),
                                    credential_generation: None,
                                    policy_rev: 1,
                                    rule_id: Some("node-push".into()),
                                    approved_by: "rule:node-push".into(),
                                    approved_at: vault_core::iso_now(),
                                    expires_secs: now + vault_core::nodes::RULE_APPROVAL_TTL_SECS,
                                    nonce: vault_core::new_uuid(),
                                },
                            )
                            .ok()
                        })
                    } else {
                        None
                    };
                    reply.actions.push(Action::Push {
                        target: t.id.clone(),
                        sha256: sha,
                        content_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
                        approval,
                    });
                }
            }
        }
    }
    let pull: Vec<String> = beat
        .targets
        .iter()
        .filter(|t| t.mode == "pull")
        .map(|t| t.id.clone())
        .collect();
    for target in host.wanted_uploads(&node.id, &pull) {
        reply.actions.push(Action::Upload { target });
    }
    reply
}

/// The history snapshot a human should read before approving `sha`: the candidate if it
/// really holds those bytes, otherwise the newest snapshot that does, otherwise none. The
/// approval binds the hash; the review link used to be whatever snapshot the stream last
/// wrote, which can be a different rendering than the bytes the node would be sent.
pub fn reviewable_seq(
    conn: &vault_core::SqlConnection,
    candidate: Option<i64>,
    sha: &str,
) -> Option<i64> {
    if let Some(seq) = candidate {
        if let Ok(Some(snap)) = vault_core::config_history::get(conn, seq) {
            if snap.meta.sha256 == sha {
                return Some(seq);
            }
        }
    }
    vault_core::config_history::find_by_sha(conn, sha, 1)
        .ok()
        .and_then(|m| m.first().map(|x| x.seq))
}

// ── Dialling listening nodes ─────────────────────────────────────────────────

static LAST_HUB_TS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// A strictly increasing millisecond timestamp, even if the clock steps back.
fn next_hub_ts() -> i64 {
    use std::sync::atomic::Ordering;
    let now = now_ms();
    let mut prev = LAST_HUB_TS.load(Ordering::SeqCst);
    loop {
        let next = now.max(prev + 1);
        match LAST_HUB_TS.compare_exchange(prev, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return next,
            Err(p) => prev = p,
        }
    }
}

/// One signed POST to a listening node.
fn dial(
    fingerprint: &str,
    seed: &str,
    base: &str,
    path: &str,
    body: &[u8],
) -> Result<crate::tls::Dialed, String> {
    let ts = next_hub_ts();
    let sig = vault_core::nodes::sign_hub_request(seed, "POST", path, ts, body)?;
    crate::tls::hub_post(fingerprint, base, path, ts, &sig, body)
}

/// Asks one listening node for its beat and answers it. Blocking: the pinned
/// client is `reqwest::blocking`, and `process` takes `std` locks.
pub fn poll_node<H: HubHost>(host: &H, node: &NodeRecord) -> Result<(), String> {
    let listen = node.listen.as_ref().ok_or("not a listening node")?;
    let (seed, _) = host.store(|s| s.hub_node_key())??;
    let (fp, base) = (listen.cert_sha256.as_str(), listen.base());

    let resp = dial(fp, &seed, base, "/node/v1/poll", b"")?;
    if !(200..300).contains(&resp.status) {
        return Err(format!("the node answered {}", resp.status));
    }
    let (Some(id), Some(ts), Some(sig)) = (
        resp.headers.get("x-node-id"),
        resp.headers.get("x-node-ts"),
        resp.headers.get("x-node-sig"),
    ) else {
        return Err("the node's answer is not signed".into());
    };
    if id != &node.id {
        return Err("the answer is signed for a different node".into());
    }
    let ts: i64 = ts.parse().map_err(|_| "bad timestamp".to_string())?;
    // The same check, replay mark and clock rule as a beat that was dialled in.
    let node = host
        .store(|s| s.authenticate(id, "POST", "/api/nodes/beat", ts, &resp.body, sig, now_ms()))?
        .map_err(|e| e.message().to_string())?;
    let beat: Beat = serde_json::from_slice(&resp.body).map_err(|e| format!("bad beat: {e}"))?;

    let reply = process(host, &node, &beat, true);
    let act = serde_json::to_vec(&reply).map_err(|e| e.to_string())?;
    let r = dial(fp, &seed, base, "/node/v1/act", &act)?;
    if !(200..300).contains(&r.status) {
        return Err(format!("the node refused the reply: {}", r.status));
    }
    let _ = host.store(|s| s.mark_polled(&node.id, &vault_core::iso_now()));
    Ok(())
}
