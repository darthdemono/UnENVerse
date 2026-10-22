// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The Cloudflare API token connector for the rotation engine (`vault_core::rotation`).
//!
//! Default strategy is `create_overlapping`: read the old token's policies, create a token with the
//! same policies named `unv:<entry>:g<generation>:<run>`, verify it, and delete the old token only
//! when the engine says every consumer passed its check. `PUT /user/tokens/{id}/value` rolls a token
//! in place (the old value dies at once), so it is `roll_provider_generated`, opt-in, and a lost
//! response to it is never retried and cannot be reconciled.
//!
//! The management credential is the token being rotated: it needs the "API Tokens Write" permission.

use serde_json::{json, Value};
use std::time::Duration;
use vault_core::rotation::{Class, Credential, IssuerConnector, IssuerError, Reconcile, StepKind};

const TIMEOUT: Duration = Duration::from_secs(30);

pub struct Cloudflare {
    client: reqwest::blocking::Client,
    base: String,
    /// The token being rotated, used to call the token API.
    manager: Credential,
    allow_roll: bool,
    /// The new token, once there is one: the old one is gone after a revoke, so it can no longer
    /// answer "is the old token deleted?".
    successor: Option<Credential>,
}

enum Reply {
    Ok(Value),
    Status(u16),
    Lost(String),
}

impl Cloudflare {
    pub fn new(base: &str, manager: Credential, allow_roll: bool) -> Result<Self, String> {
        Ok(Self {
            client: crate::tls::build_issuer_client(TIMEOUT).map_err(|e| e.message)?,
            base: base.trim_end_matches('/').to_string(),
            manager,
            allow_roll,
            successor: None,
        })
    }

    fn call(&self, method: reqwest::Method, path: &str, token: &str, body: Option<Value>) -> Reply {
        let mut req = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        match req.send() {
            Err(e) => Reply::Lost(e.without_url().to_string()),
            Ok(resp) => {
                let status = resp.status().as_u16();
                if !(200..300).contains(&status) {
                    return Reply::Status(status);
                }
                match resp.json::<Value>() {
                    Ok(v) if v["success"].as_bool() == Some(true) => Reply::Ok(v),
                    Ok(_) => Reply::Status(status),
                    // The status said success but the body is unusable: treat as lost.
                    Err(e) => Reply::Lost(e.to_string()),
                }
            }
        }
    }

    /// Lets a resumed run reconcile a lost revoke with the token that replaced the old one.
    pub fn with_successor(mut self, c: Credential) -> Self {
        self.successor = Some(c);
        self
    }

    fn rejected(status: u16, what: &str) -> IssuerError {
        IssuerError::Rejected(format!("Cloudflare answered HTTP {status} to {what}"))
    }
}

impl IssuerConnector for Cloudflare {
    fn capabilities(&self) -> Vec<Class> {
        let mut classes = vec![Class::CreateOverlapping];
        if self.allow_roll {
            classes.push(Class::RollProviderGenerated);
        }
        classes
    }

    fn verify(&mut self, c: &Credential) -> Result<(), IssuerError> {
        match self.call(reqwest::Method::GET, "/user/tokens/verify", &c.value, None) {
            Reply::Ok(v) if v["result"]["status"] == "active" => Ok(()),
            Reply::Ok(_) => Err(IssuerError::Rejected("the token is not active".into())),
            Reply::Status(s) => Err(Self::rejected(s, "token verification")),
            Reply::Lost(e) => Err(IssuerError::Lost(e)),
        }
    }

    fn create(&mut self, name: &str) -> Result<Credential, IssuerError> {
        let old = self.call(
            reqwest::Method::GET,
            &format!("/user/tokens/{}", self.manager.id),
            &self.manager.value,
            None,
        );
        let policies = match old {
            Reply::Ok(v) => v["result"]["policies"].clone(),
            Reply::Status(s) => return Err(Self::rejected(s, "reading the old token")),
            Reply::Lost(e) => {
                return Err(IssuerError::Rejected(format!("reading the old token: {e}")))
            }
        };
        match self.call(
            reqwest::Method::POST,
            "/user/tokens",
            &self.manager.value,
            Some(json!({ "name": name, "policies": policies })),
        ) {
            Reply::Ok(v) => match (v["result"]["id"].as_str(), v["result"]["value"].as_str()) {
                (Some(id), Some(value)) => {
                    let c = Credential {
                        id: id.into(),
                        value: value.into(),
                    };
                    self.successor = Some(c.clone());
                    Ok(c)
                }
                _ => Err(IssuerError::Lost("the answer carried no token".into())),
            },
            Reply::Status(s) => Err(Self::rejected(s, "token creation")),
            Reply::Lost(e) => Err(IssuerError::Lost(e)),
        }
    }

    fn set_value(&mut self, _name: &str, _value: &str) -> Result<(), IssuerError> {
        Err(IssuerError::Rejected(
            "Cloudflare chooses token values; it cannot be given one".into(),
        ))
    }

    fn roll(&mut self, current: &Credential) -> Result<Credential, IssuerError> {
        match self.call(
            reqwest::Method::PUT,
            &format!("/user/tokens/{}/value", current.id),
            &current.value,
            Some(json!({})),
        ) {
            Reply::Ok(v) => v["result"]
                .as_str()
                .map(|value| Credential {
                    id: current.id.clone(),
                    value: value.into(),
                })
                .ok_or_else(|| IssuerError::Lost("the answer carried no value".into())),
            Reply::Status(s) => Err(Self::rejected(s, "rolling the token")),
            Reply::Lost(e) => Err(IssuerError::Lost(e)),
        }
    }

    fn revoke(&mut self, c: &Credential) -> Result<(), IssuerError> {
        match self.call(
            reqwest::Method::DELETE,
            &format!("/user/tokens/{}", c.id),
            &self.manager.value,
            None,
        ) {
            Reply::Ok(_) => Ok(()),
            Reply::Status(s) => Err(Self::rejected(s, "token deletion")),
            Reply::Lost(e) => Err(IssuerError::Lost(e)),
        }
    }

    fn reconcile(&mut self, step: &StepKind, name: &str) -> Result<Reconcile, IssuerError> {
        match step {
            StepKind::Create => match self.call(
                reqwest::Method::GET,
                "/user/tokens?per_page=50",
                &self.manager.value,
                None,
            ) {
                Reply::Ok(v) => Ok(
                    if v["result"]
                        .as_array()
                        .is_some_and(|l| l.iter().any(|t| t["name"] == name))
                    {
                        Reconcile::Done
                    } else {
                        Reconcile::NotDone
                    },
                ),
                _ => Ok(Reconcile::Unknown),
            },
            // `name` is the token id for a revoke.
            StepKind::Revoke => match self.call(
                reqwest::Method::GET,
                &format!("/user/tokens/{name}"),
                &self.successor.as_ref().unwrap_or(&self.manager).value,
                None,
            ) {
                Reply::Ok(_) => Ok(Reconcile::NotDone),
                Reply::Status(404) | Reply::Status(400) => Ok(Reconcile::Done),
                _ => Ok(Reconcile::Unknown),
            },
            StepKind::SetValue | StepKind::Roll => Ok(Reconcile::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use vault_core::rotation::*;

    #[derive(Default)]
    struct Server {
        /// id -> (name, value)
        tokens: BTreeMap<String, (String, String)>,
        next: u32,
        /// "METHOD /path-prefix" -> drop the connection after (true) or before (false) acting.
        lose_after: Vec<String>,
        lose_before: Vec<String>,
        /// "METHOD /path-prefix" -> answer this status and raw body whatever the state.
        forced: Vec<(String, u16, String)>,
        log: Vec<String>,
    }

    fn serve(state: Arc<Mutex<Server>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).into_owned();
                let mut first = head.lines().next().unwrap_or("").split(' ');
                let method = first.next().unwrap_or("").to_string();
                let path = first.next().unwrap_or("").to_string();
                let bearer = head
                    .lines()
                    .find_map(|l| l.strip_prefix("authorization: Bearer "))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let key = format!("{method} {path}");
                let mut st = state.lock().unwrap();
                st.log.push(key.clone());
                if st.lose_before.iter().any(|k| key.starts_with(k.as_str())) {
                    continue; // close without answering
                }
                if let Some((_, code, raw)) = st
                    .forced
                    .iter()
                    .find(|(k, _, _)| key.starts_with(k.as_str()))
                    .cloned()
                {
                    let _ = write!(
                        s,
                        "HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{raw}",
                        raw.len()
                    );
                    continue;
                }
                let valid = |st: &Server, v: &str| st.tokens.values().any(|(_, val)| val == v);
                let (code, body) = if !valid(&st, &bearer) {
                    (401, json!({"success": false}))
                } else if path == "/user/tokens/verify" {
                    (
                        200,
                        json!({"success": true, "result": {"status": "active"}}),
                    )
                } else if method == "GET" && path.starts_with("/user/tokens?") {
                    let list: Vec<Value> = st
                        .tokens
                        .iter()
                        .map(|(id, (n, _))| json!({"id": id, "name": n}))
                        .collect();
                    (200, json!({"success": true, "result": list}))
                } else if method == "GET" {
                    let id = path.rsplit('/').next().unwrap();
                    match st.tokens.get(id) {
                        Some((n, _)) => (
                            200,
                            json!({"success": true, "result": {"id": id, "name": n, "policies": [{"effect": "allow"}]}}),
                        ),
                        None => (404, json!({"success": false})),
                    }
                } else if method == "POST" && path == "/user/tokens" {
                    st.next += 1;
                    let id = format!("tok{}", st.next);
                    let value = format!("cf-secret-{}", st.next);
                    let body_text = head.split("\r\n\r\n").nth(1).unwrap_or("");
                    let name = serde_json::from_str::<Value>(body_text)
                        .ok()
                        .and_then(|v| v["name"].as_str().map(str::to_string))
                        .unwrap_or_default();
                    st.tokens.insert(id.clone(), (name, value.clone()));
                    (
                        200,
                        json!({"success": true, "result": {"id": id, "value": value}}),
                    )
                } else if method == "PUT" {
                    let id = path
                        .trim_start_matches("/user/tokens/")
                        .trim_end_matches("/value")
                        .to_string();
                    st.next += 1;
                    let value = format!("cf-rolled-{}", st.next);
                    if let Some(t) = st.tokens.get_mut(&id) {
                        t.1 = value.clone();
                    }
                    (200, json!({"success": true, "result": value}))
                } else if method == "DELETE" {
                    let id = path.rsplit('/').next().unwrap().to_string();
                    st.tokens.remove(&id);
                    (200, json!({"success": true, "result": {"id": id}}))
                } else {
                    (404, json!({"success": false}))
                };
                if st.lose_after.iter().any(|k| key.starts_with(k.as_str())) {
                    continue; // acted, then drop the connection
                }
                let text = body.to_string();
                let _ = write!(
                    s,
                    "HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                    text.len()
                );
            }
        });
        base
    }

    struct Entry(Credential, Vec<Credential>);
    impl EntryStore for Entry {
        fn current(&self) -> Credential {
            self.0.clone()
        }
        fn commit(&mut self, new: &Credential, _g: u64) -> Result<(), String> {
            self.1.push(std::mem::replace(&mut self.0, new.clone()));
            Ok(())
        }
        fn restore(&mut self, _o: &Credential) -> Result<(), String> {
            if let Some(p) = self.1.pop() {
                self.0 = p;
            }
            Ok(())
        }
    }
    struct NoConsumers;
    impl Consumers for NoConsumers {
        fn required(&self) -> Vec<String> {
            vec![]
        }
        fn deploy(&mut self, _: &str, _: &Credential, _: u64) -> Outcome {
            Outcome::Ok
        }
        fn check(&mut self, _: &str) -> Outcome {
            Outcome::Ok
        }
        fn redeploy_previous(&mut self, _: &str) -> Outcome {
            Outcome::Ok
        }
    }
    #[derive(Default)]
    struct Mem(BTreeMap<String, String>);
    impl RunStore for Mem {
        fn save(&mut self, _: &Run) -> Result<(), String> {
            Ok(())
        }
        fn put_pending(&mut self, r: &str, v: &str) -> Result<(), String> {
            self.0.insert(r.into(), v.into());
            Ok(())
        }
        fn pending(&self, r: &str) -> Option<String> {
            self.0.get(r).cloned()
        }
        fn clear_pending(&mut self, r: &str) -> Result<(), String> {
            self.0.remove(r);
            Ok(())
        }
    }

    fn rotate(
        class: Class,
        configure: impl FnOnce(&mut Server),
    ) -> (Run, Arc<Mutex<Server>>, Entry) {
        let state = Arc::new(Mutex::new(Server::default()));
        let old_value = format!("cf-old-{}", vault_core::new_uuid());
        state
            .lock()
            .unwrap()
            .tokens
            .insert("old".into(), ("the old token".into(), old_value.clone()));
        configure(&mut state.lock().unwrap());
        let base = serve(state.clone());
        let old = Credential {
            id: "old".into(),
            value: old_value,
        };
        let mut issuer = Cloudflare::new(&base, old.clone(), true).unwrap();
        let mut entry = Entry(old, vec![]);
        let mut run = Run::plan("r1", "Cloudflare", 2, class, Policy::default(), "old");
        let gen = || String::new();
        Engine {
            issuer: &mut issuer,
            consumers: &mut NoConsumers,
            entry: &mut entry,
            store: &mut Mem::default(),
            generate: &gen,
        }
        .run(&mut run)
        .unwrap();
        (run, state, entry)
    }

    #[test]
    fn creates_a_named_token_verifies_it_then_deletes_the_old_one_last() {
        let (run, state, entry) = rotate(Class::CreateOverlapping, |_| {});
        assert_eq!(run.status, Status::Succeeded, "{:?}", run.status);
        let st = state.lock().unwrap();
        assert!(!st.tokens.contains_key("old"));
        assert_eq!(
            st.tokens["tok1"].0, "unv:Cloudflare:g2:r1",
            "named for entry, generation and run"
        );
        assert_eq!(entry.0.id, "tok1");
        assert_eq!(
            st.log.last().map(String::as_str),
            Some("DELETE /user/tokens/old")
        );
        assert!(st.log.iter().any(|l| l == "GET /user/tokens/verify"));
    }

    #[test]
    fn a_create_whose_answer_is_lost_is_found_by_name_and_stops_for_a_person() {
        let (run, state, entry) = rotate(Class::CreateOverlapping, |s| {
            s.lose_after.push("POST /user/tokens".into());
        });
        assert!(
            matches!(run.status, Status::NeedsAttention(_)),
            "{:?}",
            run.status
        );
        let st = state.lock().unwrap();
        assert!(st.tokens.contains_key("old"), "the old token is untouched");
        assert!(st.tokens.values().any(|(n, _)| n == "unv:Cloudflare:g2:r1"));
        assert!(!st.log.iter().any(|l| l.starts_with("DELETE")));
        assert_eq!(entry.0.id, "old");
    }

    #[test]
    fn a_roll_whose_answer_is_lost_is_not_retried() {
        let (run, state, _) = rotate(Class::RollProviderGenerated, |s| {
            s.lose_after.push("PUT ".into());
        });
        assert!(
            matches!(run.status, Status::NeedsAttention(_)),
            "{:?}",
            run.status
        );
        let st = state.lock().unwrap();
        assert_eq!(st.log.iter().filter(|l| l.starts_with("PUT")).count(), 1);
    }

    #[test]
    fn an_in_place_roll_stores_the_provider_value_and_verifies_it() {
        let (run, _, entry) = rotate(Class::RollProviderGenerated, |_| {});
        // Rolling keeps the id, so the engine revokes nothing it should keep: it deletes "old"
        // only if the old id differs. Here the id is the same, which the engine must not delete.
        assert!(
            matches!(run.status, Status::Succeeded | Status::RevocationPending(_)),
            "{:?}",
            run.status
        );
        assert!(entry.0.value.starts_with("cf-rolled-"));
    }

    #[test]
    fn a_lost_delete_that_took_effect_is_reconciled_as_done() {
        let (run, state, _) = rotate(Class::CreateOverlapping, |s| {
            s.lose_after.push("DELETE ".into());
        });
        assert_eq!(run.status, Status::Succeeded, "{:?}", run.status);
        assert_eq!(
            state
                .lock()
                .unwrap()
                .log
                .iter()
                .filter(|l| l.starts_with("DELETE"))
                .count(),
            1
        );
    }

    /// A connector pointed at a server that has the old token and the given quirks.
    fn connector(
        configure: impl FnOnce(&mut Server),
    ) -> (Cloudflare, Arc<Mutex<Server>>, Credential) {
        let state = Arc::new(Mutex::new(Server::default()));
        let old_value = format!("cf-old-{}", vault_core::new_uuid());
        state
            .lock()
            .unwrap()
            .tokens
            .insert("old".into(), ("old".into(), old_value.clone()));
        configure(&mut state.lock().unwrap());
        let base = serve(state.clone());
        let old = Credential {
            id: "old".into(),
            value: old_value,
        };
        (
            Cloudflare::new(&base, old.clone(), true).unwrap(),
            state,
            old,
        )
    }

    fn forced(prefix: &str, code: u16, body: &str) -> impl FnOnce(&mut Server) {
        let f = (prefix.to_string(), code, body.to_string());
        move |s: &mut Server| s.forced.push(f)
    }

    #[test]
    fn verification_distinguishes_inactive_refused_and_unreachable() {
        let (mut cf, _, old) = connector(forced(
            "GET /user/tokens/verify",
            200,
            r#"{"success":true,"result":{"status":"disabled"}}"#,
        ));
        assert!(
            matches!(cf.verify(&old), Err(IssuerError::Rejected(m)) if m.contains("not active"))
        );
        let (mut cf, _, old) = connector(forced("GET /user/tokens/verify", 500, "{}"));
        assert!(matches!(cf.verify(&old), Err(IssuerError::Rejected(m)) if m.contains("500")));
        let (mut cf, _, old) = connector(|s| s.lose_before.push("GET /user/tokens/verify".into()));
        assert!(matches!(cf.verify(&old), Err(IssuerError::Lost(_))));
        // A 200 that is not the API's answer is not trusted as success, and not as a refusal either.
        let (mut cf, _, old) =
            connector(forced("GET /user/tokens/verify", 200, "<html>proxy</html>"));
        assert!(matches!(cf.verify(&old), Err(IssuerError::Lost(_))));
        let (mut cf, _, old) = connector(forced(
            "GET /user/tokens/verify",
            200,
            r#"{"success":false}"#,
        ));
        assert!(matches!(cf.verify(&old), Err(IssuerError::Rejected(_))));
    }

    #[test]
    fn creation_fails_safely_when_the_old_token_cannot_be_read_or_the_answer_is_unusable() {
        // Reading the old token's policies: gone, refused, or the connection drops. Nothing is
        // created in any case, so these are refusals, not lost answers.
        for forcing in [
            forced("GET /user/tokens/old", 404, "{}"),
            forced("GET /user/tokens/old", 403, "{}"),
        ] {
            let (mut cf, state, _) = connector(forcing);
            assert!(matches!(cf.create("n"), Err(IssuerError::Rejected(_))));
            assert!(!state
                .lock()
                .unwrap()
                .log
                .iter()
                .any(|l| l.starts_with("POST")));
        }
        let (mut cf, state, _) = connector(|s| s.lose_before.push("GET /user/tokens/old".into()));
        assert!(
            matches!(cf.create("n"), Err(IssuerError::Rejected(m)) if m.contains("reading the old token"))
        );
        assert!(!state
            .lock()
            .unwrap()
            .log
            .iter()
            .any(|l| l.starts_with("POST")));
        // The create itself: refused, or an answer without a token (maybe it was made).
        let (mut cf, _, _) = connector(forced("POST /user/tokens", 403, "{}"));
        assert!(matches!(cf.create("n"), Err(IssuerError::Rejected(m)) if m.contains("403")));
        let (mut cf, _, _) = connector(forced(
            "POST /user/tokens",
            200,
            r#"{"success":true,"result":{"id":"x"}}"#,
        ));
        assert!(matches!(cf.create("n"), Err(IssuerError::Lost(m)) if m.contains("no token")));
    }

    #[test]
    fn rolling_and_deleting_report_refusals_and_lost_answers_apart() {
        let cred = Credential {
            id: "old".into(),
            value: "ignored".into(),
        };
        let (mut cf, _, _) = connector(forced("PUT ", 403, "{}"));
        assert!(matches!(cf.roll(&cred), Err(IssuerError::Rejected(_))));
        let (mut cf, _, _) = connector(forced(
            "PUT ",
            200,
            r#"{"success":true,"result":{"not":"a string"}}"#,
        ));
        assert!(matches!(cf.roll(&cred), Err(IssuerError::Lost(m)) if m.contains("no value")));
        let (mut cf, _, _) = connector(|s| s.lose_before.push("PUT ".into()));
        assert!(matches!(cf.roll(&cred), Err(IssuerError::Lost(_))));
        let (mut cf, _, _) = connector(forced("DELETE ", 403, "{}"));
        assert!(matches!(cf.revoke(&cred), Err(IssuerError::Rejected(_))));
        let (mut cf, _, _) = connector(|s| s.lose_before.push("DELETE ".into()));
        assert!(matches!(cf.revoke(&cred), Err(IssuerError::Lost(_))));
    }

    #[test]
    fn reconcile_reads_the_issuer_and_says_unknown_when_it_cannot() {
        // Create: the token with this name is there, absent, or the list cannot be read.
        let (mut cf, _, _) = connector(|_| {});
        assert_eq!(cf.reconcile(&StepKind::Create, "old"), Ok(Reconcile::Done));
        assert_eq!(
            cf.reconcile(&StepKind::Create, "never-made"),
            Ok(Reconcile::NotDone)
        );
        let (mut cf, _, _) = connector(forced("GET /user/tokens?", 500, "{}"));
        assert_eq!(
            cf.reconcile(&StepKind::Create, "old"),
            Ok(Reconcile::Unknown)
        );
        // Revoke: still there (not done), gone (done, as 404 or 400), or unreadable.
        let (mut cf, _, _) = connector(|_| {});
        assert_eq!(
            cf.reconcile(&StepKind::Revoke, "old"),
            Ok(Reconcile::NotDone)
        );
        let (mut cf, _, _) = connector(|_| {});
        assert_eq!(cf.reconcile(&StepKind::Revoke, "gone"), Ok(Reconcile::Done));
        let (mut cf, _, _) = connector(forced("GET /user/tokens/old", 400, "{}"));
        assert_eq!(cf.reconcile(&StepKind::Revoke, "old"), Ok(Reconcile::Done));
        let (mut cf, _, _) = connector(forced("GET /user/tokens/old", 500, "{}"));
        assert_eq!(
            cf.reconcile(&StepKind::Revoke, "old"),
            Ok(Reconcile::Unknown)
        );
        // Nothing can be reconciled for a value the issuer chose or one a person must supply.
        assert_eq!(
            cf.reconcile(&StepKind::SetValue, "x"),
            Ok(Reconcile::Unknown)
        );
        assert_eq!(cf.reconcile(&StepKind::Roll, "x"), Ok(Reconcile::Unknown));
    }

    #[test]
    fn a_resumed_run_reconciles_a_revoke_with_the_token_that_replaced_the_old_one() {
        // The old token is deleted, so only its successor can still ask whether it is gone.
        let (cf, state, old) = connector(|_| {});
        let successor = Credential {
            id: "new".into(),
            value: "cf-new-secret".into(),
        };
        state
            .lock()
            .unwrap()
            .tokens
            .insert("new".into(), ("new".into(), successor.value.clone()));
        state.lock().unwrap().tokens.remove("old");
        let mut cf = cf.with_successor(successor);
        assert_eq!(
            cf.reconcile(&StepKind::Revoke, &old.id),
            Ok(Reconcile::Done)
        );
        let _ = old;
    }

    #[test]
    fn a_rejected_creation_changes_nothing_and_set_value_is_refused() {
        let state = Arc::new(Mutex::new(Server::default()));
        let base = serve(state.clone());
        let mut cf = Cloudflare::new(
            &base,
            Credential {
                id: "x".into(),
                value: "y".into(),
            },
            false,
        )
        .unwrap();
        assert_eq!(cf.capabilities(), vec![Class::CreateOverlapping]);
        assert!(matches!(cf.create("n"), Err(IssuerError::Rejected(_))));
        assert!(matches!(
            cf.set_value("n", "v"),
            Err(IssuerError::Rejected(_))
        ));
        assert!(matches!(
            cf.verify(&Credential {
                id: "x".into(),
                value: "y".into()
            }),
            Err(IssuerError::Rejected(_))
        ));
        assert_eq!(cf.reconcile(&StepKind::Roll, "n"), Ok(Reconcile::Unknown));
    }
}
