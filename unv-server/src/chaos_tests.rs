//! Several actors, one server (chaos testing, RBAC and session level).
//!
//! Each test is a situation two or more people can really produce: an admin revokes
//! while a user is mid-request, a user saves a document they were served before their
//! permissions changed, two logins race a lock. They drive the real router with
//! `tower::ServiceExt::oneshot`, so every handler, extractor and session check is the
//! production one. A test names the bug it was written for; with the fix reverted it
//! must fail.

use super::*;
use axum::http::HeaderMap;
use serde_json::{json, Value};

struct Rig {
    router: Router,
    state: AppState,
    owner: HeaderMap,
    key: [u8; 32],
}

fn bearer(token: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    h
}

fn scratch(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "unv-chaos-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A server whose vault already holds `entries`, with the owner signed in.
fn rig(entries: Vec<Value>) -> Rig {
    let d = scratch("rbac");
    let state = AppState::new(
        d.join("vault.db"),
        d.join("vault.salt"),
        None,
        480,
        24,
        false,
    );
    let key = [42u8; 32];
    let owner_token = state.adopt_owner_key(key, "owner".into());
    let conn = open_db(&state.db_path, &key).unwrap();
    init_schema(&conn).unwrap();
    save_vault(
        &conn,
        json!({ "api_keys": entries, "user_categories": [], "projects": [] }),
        vault_core::SaveCtx::default(),
    )
    .unwrap();
    let router = build_router(state.clone(), 8743);
    Rig {
        router,
        state,
        owner: bearer(&owner_token),
        key,
    }
}

async fn send(
    r: &Router,
    method: &str,
    uri: &str,
    auth: Option<&HeaderMap>,
    extra: &[(&str, &str)],
    body: Option<Value>,
    ip: &str,
) -> (StatusCode, Value, HeaderMap) {
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(a) = auth {
        req = req.header(
            axum::http::header::AUTHORIZATION,
            a[axum::http::header::AUTHORIZATION].clone(),
        );
    }
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let mut req = req
        .body(axum::body::Body::from(
            body.map(|b| b.to_string()).unwrap_or_default(),
        ))
        .unwrap();
    req.extensions_mut().insert(ConnectInfo::<SocketAddr>(
        format!("{ip}:50000").parse().unwrap(),
    ));
    let resp = r.clone().oneshot(req).await.unwrap();
    let (parts, body) = resp.into_parts();
    let bytes = axum::body::to_bytes(body, 4 * 1024 * 1024).await.unwrap();
    let v = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (parts.status, v, parts.headers)
}

async fn call(
    r: &Router,
    m: &str,
    u: &str,
    a: Option<&HeaderMap>,
    b: Option<Value>,
) -> (StatusCode, Value) {
    let (s, v, _) = send(r, m, u, a, &[], b, "127.0.0.1").await;
    (s, v)
}

struct User {
    id: String,
    token: HeaderMap,
    name: String,
    pw: String,
}

async fn make_user(rig: &Rig, name: &str, read: &str, write: &str) -> User {
    let pw = format!("pw-{name}-long-enough");
    let (st, body) = call(
        &rig.router,
        "POST",
        "/api/users",
        Some(&rig.owner),
        Some(json!({ "username": name, "password": pw })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{body}");
    let id = body["id"].as_str().unwrap().to_string();
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{id}/permissions"),
        Some(&rig.owner),
        Some(json!({ "read": read, "write": write })),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let token = login(rig, name, &pw).await;
    User {
        id,
        token,
        name: name.into(),
        pw,
    }
}

async fn login(rig: &Rig, name: &str, pw: &str) -> HeaderMap {
    let (st, body) = call(
        &rig.router,
        "POST",
        "/api/auth",
        None,
        Some(json!({ "username": name, "password": pw })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    bearer(body["token"].as_str().unwrap())
}

fn e(id: &str, ty: &str, key: &str) -> Value {
    json!({ "id": id, "provider": format!("P-{id}"), "secretType": ty, "api_key": key })
}

fn ids(v: &Value) -> Vec<String> {
    v["api_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap().to_string())
        .collect()
}

async fn owner_ids(rig: &Rig) -> Vec<String> {
    let (st, v) = call(&rig.router, "GET", "/api/vault", Some(&rig.owner), None).await;
    assert_eq!(st, StatusCode::OK);
    ids(&v)
}

// ── Identity changes must reach live sessions ────────────────────────────────

/// BUG: a session was checked only against the in-memory map. Deleting a user, revoking
/// the token it came from, changing its password or disabling its TOTP left every live
/// session working until idle expiry (and a busy session slides that forever). The user
/// kept reading and writing after the admin had finished removing them.
#[tokio::test]
async fn deleting_a_user_ends_its_live_sessions() {
    let rig = rig(vec![e("a", "api_key", "k")]);
    let u = make_user(&rig, "alice", "type:*", "type:*").await;
    assert_eq!(
        call(&rig.router, "GET", "/api/vault", Some(&u.token), None)
            .await
            .0,
        StatusCode::OK
    );
    let (st, _) = call(
        &rig.router,
        "DELETE",
        &format!("/api/users/{}", u.id),
        Some(&rig.owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert_eq!(
        call(&rig.router, "GET", "/api/vault", Some(&u.token), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED,
        "a deleted user still reads the vault"
    );
    assert_eq!(
        call(
            &rig.router,
            "PUT",
            "/api/vault",
            Some(&u.token),
            Some(json!({ "api_keys": [] }))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED,
        "a deleted user still writes the vault"
    );
}

#[tokio::test]
async fn revoking_a_token_ends_the_sessions_made_from_it() {
    let rig = rig(vec![e("a", "api_key", "k")]);
    let u = make_user(&rig, "bob", "type:*", "type:*").await;
    let (st, body) = call(
        &rig.router,
        "POST",
        &format!("/api/users/{}/tokens", u.id),
        Some(&rig.owner),
        Some(json!({ "description": "ci" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{body}");
    let (plain, token_id) = (
        body["token"].as_str().unwrap().to_string(),
        body["token_id"].as_str().unwrap().to_string(),
    );
    let (st, auth) = call(
        &rig.router,
        "POST",
        "/api/auth",
        None,
        Some(json!({ "token": plain })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{auth}");
    let session = bearer(auth["token"].as_str().unwrap());
    assert_eq!(
        call(&rig.router, "GET", "/api/vault", Some(&session), None)
            .await
            .0,
        StatusCode::OK
    );
    let (st, _) = call(
        &rig.router,
        "DELETE",
        &format!("/api/users/{}/tokens/{token_id}", u.id),
        Some(&rig.owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert_eq!(
        call(&rig.router, "GET", "/api/vault", Some(&session), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED,
        "a revoked token's session still works"
    );
    // The same user's password session is a different credential and stays valid.
    assert_eq!(
        call(&rig.router, "GET", "/api/vault", Some(&u.token), None)
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn changing_a_password_ends_the_sessions_made_with_the_old_one() {
    let rig = rig(vec![e("a", "api_key", "k")]);
    let u = make_user(&rig, "carol", "type:*", "type:*").await;
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{}/password", u.id),
        Some(&rig.owner),
        Some(json!({ "password": "a-brand-new-password" })),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert_eq!(
        call(&rig.router, "GET", "/api/vault", Some(&u.token), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED,
        "a session signed in with the old password survives the change"
    );
    let _ = (&u.name, &u.pw);
}

// ── Permission changes must not turn a stale save into a deletion ────────────

/// BUG: widening a user's permissions, then a save of the document they were served
/// before: every entry the new permissions cover but the old ones hid looked "deleted by
/// the user" and was removed. Permission edits do not move the vault version, so the
/// compare-and-swap passed.
#[tokio::test]
async fn widening_permissions_never_lets_a_stale_save_delete_unseen_entries() {
    let rig = rig(vec![e("api", "api_key", "k1"), e("pw", "password", "p1")]);
    let u = make_user(&rig, "dave", "type:api_key", "type:api_key").await;
    let (_, served, headers) = send(
        &rig.router,
        "GET",
        "/api/vault",
        Some(&u.token),
        &[],
        None,
        "127.0.0.1",
    )
    .await;
    assert_eq!(
        ids(&served),
        vec!["api"],
        "dave must only be served the api_key entry"
    );
    let etag = headers.get("etag").unwrap().to_str().unwrap().to_string();
    // The owner widens dave to the password entry as well.
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{}/permissions", u.id),
        Some(&rig.owner),
        Some(json!({ "read": "type:api_key OR type:password", "write": "type:api_key OR type:password" })),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    // Dave saves what he was served (edited), carrying the version he read.
    let mut mine = served.clone();
    mine["api_keys"][0]["api_key"] = json!("k1-edited");
    let (_st, _body, _) = send(
        &rig.router,
        "PUT",
        "/api/vault",
        Some(&u.token),
        &[("if-match", &etag)],
        Some(mine),
        "127.0.0.1",
    )
    .await;
    let have = owner_ids(&rig).await;
    assert!(
        have.contains(&"pw".to_string()),
        "the password entry dave never saw was deleted: {have:?}"
    );
}

/// BUG: without an `If-Match` the merge base was the version at the moment of the PUT, so
/// every entry another writer added since the user's GET looked deleted by the user.
#[tokio::test]
async fn a_save_without_if_match_does_not_delete_what_others_added_since_the_read() {
    let rig = rig(vec![e("a", "api_key", "k")]);
    let u = make_user(&rig, "erin", "type:*", "type:*").await;
    let (_, served, _) = send(
        &rig.router,
        "GET",
        "/api/vault",
        Some(&u.token),
        &[],
        None,
        "127.0.0.1",
    )
    .await;
    // The owner adds an entry after erin's read.
    let mut full = call(&rig.router, "GET", "/api/vault", Some(&rig.owner), None)
        .await
        .1;
    full["api_keys"]
        .as_array_mut()
        .unwrap()
        .push(e("new", "api_key", "n"));
    let etag = send(
        &rig.router,
        "GET",
        "/api/vault",
        Some(&rig.owner),
        &[],
        None,
        "127.0.0.1",
    )
    .await
    .2
    .get("etag")
    .unwrap()
    .to_str()
    .unwrap()
    .to_string();
    let (st, _) = call_if_match(&rig, &rig.owner, &etag, full).await;
    assert!(st.is_success(), "{st}");
    // Erin edits her copy and saves it without naming a version.
    let mut mine = served;
    mine["api_keys"][0]["api_key"] = json!("k2");
    let _ = call(&rig.router, "PUT", "/api/vault", Some(&u.token), Some(mine)).await;
    assert!(
        owner_ids(&rig).await.contains(&"new".to_string()),
        "an entry added after erin's read was deleted by her save"
    );
}

async fn call_if_match(rig: &Rig, who: &HeaderMap, etag: &str, body: Value) -> (StatusCode, Value) {
    let (s, v, _) = send(
        &rig.router,
        "PUT",
        "/api/vault",
        Some(who),
        &[("if-match", etag)],
        Some(body),
        "127.0.0.1",
    )
    .await;
    (s, v)
}

/// BUG: the write check ran on every submitted entry, including ones the user only has
/// read access to and did not change. A user with read `*` and write on one type got
/// "Write permission denied" for a faithful save of their own, untouched view.
#[tokio::test]
async fn saving_an_unchanged_read_only_entry_is_not_a_write() {
    let rig = rig(vec![e("api", "api_key", "k1"), e("pw", "password", "p1")]);
    let u = make_user(&rig, "frank", "type:*", "type:api_key").await;
    let (_, served, headers) = send(
        &rig.router,
        "GET",
        "/api/vault",
        Some(&u.token),
        &[],
        None,
        "127.0.0.1",
    )
    .await;
    assert_eq!(ids(&served).len(), 2);
    let etag = headers.get("etag").unwrap().to_str().unwrap().to_string();
    let mut mine = served;
    mine["api_keys"][0]["api_key"] = json!("k1-edited"); // the one he may write
    let (st, body) = call_if_match(&rig, &u.token, &etag, mine).await;
    assert!(
        st.is_success(),
        "a save of the served view was refused: {st} {body}"
    );
    assert_eq!(owner_ids(&rig).await.len(), 2);
    // Control: really changing the read-only entry is still refused.
    let (_, served, headers) = send(
        &rig.router,
        "GET",
        "/api/vault",
        Some(&u.token),
        &[],
        None,
        "127.0.0.1",
    )
    .await;
    let etag = headers.get("etag").unwrap().to_str().unwrap().to_string();
    let mut tampered = served;
    for ent in tampered["api_keys"].as_array_mut().unwrap() {
        if ent["id"] == "pw" {
            ent["api_key"] = json!("hacked");
        }
    }
    let (st, _) = call_if_match(&rig, &u.token, &etag, tampered).await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "changing a read-only entry must still be refused"
    );
}

/// BUG: deleting a class set its members' class to NULL, and a user with an individual
/// expression `*` and a class restriction `type:api_key` (ANDed) became unrestricted.
#[tokio::test]
async fn deleting_a_class_never_widens_its_members() {
    let rig = rig(vec![e("api", "api_key", "k1"), e("pw", "password", "p1")]);
    let (st, class) = call(
        &rig.router,
        "POST",
        "/api/classes",
        Some(&rig.owner),
        Some(json!({ "name": "API only", "description": "" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let class_id = class["id"].as_str().unwrap().to_string();
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/classes/{class_id}/permissions"),
        Some(&rig.owner),
        Some(json!({ "read": "type:api_key", "write": "" })),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let u = make_user(&rig, "gina", "type:*", "").await;
    let (st, _) = call(
        &rig.router,
        "PUT",
        &format!("/api/users/{}/class", u.id),
        Some(&rig.owner),
        Some(json!({ "class_id": class_id })),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let before = call(&rig.router, "GET", "/api/vault", Some(&u.token), None)
        .await
        .1;
    assert_eq!(
        ids(&before),
        vec!["api"],
        "the class restriction must apply"
    );
    let _ = call(
        &rig.router,
        "DELETE",
        &format!("/api/classes/{class_id}"),
        Some(&rig.owner),
        None,
    )
    .await;
    let after = call(&rig.router, "GET", "/api/vault", Some(&u.token), None)
        .await
        .1;
    assert_eq!(
        ids(&after),
        vec!["api"],
        "deleting the class widened gina to the whole vault"
    );
}

// ── Sign-in races ────────────────────────────────────────────────────────────

/// BUG: `auth_user` copied the owner's vault key, awaited the Argon2 check (about a
/// second), then inserted a session. An owner lock that landed in between drained every
/// session, and the login then minted one holding a live key after "locked" was
/// reported.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_in_flight_cannot_outlive_an_owner_lock() {
    let rig = rig(vec![e("a", "api_key", "k")]);
    let u = make_user(&rig, "hank", "type:*", "type:*").await;
    let router = rig.router.clone();
    let (name, pw) = (u.name.clone(), u.pw.clone());
    let login = tokio::spawn(async move {
        call(
            &router,
            "POST",
            "/api/auth",
            None,
            Some(json!({ "username": name, "password": pw })),
        )
        .await
    });
    // Let the login pass the owner-key check and reach the (slow) password hash.
    tokio::time::sleep(Duration::from_millis(80)).await;
    let (st, _) = call(&rig.router, "DELETE", "/api/unlock", Some(&rig.owner), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (st, body) = login.await.unwrap();
    if st == StatusCode::OK {
        let session = bearer(body["token"].as_str().unwrap());
        let (after, _) = call(&rig.router, "GET", "/api/vault", Some(&session), None).await;
        assert_eq!(
            after,
            StatusCode::UNAUTHORIZED,
            "a session minted during the lock still holds a working key"
        );
    }
    assert!(
        live_sessions(&rig.state).is_empty(),
        "sessions exist after the lock"
    );
    let _ = rig.key;
}

/// BUG: the failed-attempt limiter checked before the (slow) password hash and
/// recorded only after it, so a burst of N simultaneous guesses all passed the check at
/// zero failures; only the hash queue limited them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burst_of_guesses_from_one_address_is_cut_off_at_the_limit() {
    let rig = rig(vec![]);
    let u = make_user(&rig, "ivy", "type:*", "type:*").await;
    let mut hs = Vec::new();
    for _ in 0..40 {
        let r = rig.router.clone();
        let name = u.name.clone();
        hs.push(tokio::spawn(async move {
            send(
                &r,
                "POST",
                "/api/auth",
                None,
                &[],
                Some(json!({ "username": name, "password": "wrong" })),
                "10.9.9.9",
            )
            .await
            .0
        }));
    }
    let mut guessed = 0;
    let mut limited = 0;
    for h in hs {
        match h.await.unwrap() {
            StatusCode::UNAUTHORIZED => guessed += 1,
            StatusCode::TOO_MANY_REQUESTS => limited += 1,
            other => panic!("{other}"),
        }
    }
    assert!(
        guessed <= 12,
        "{guessed} of 40 simultaneous guesses reached the password check (limit 10)"
    );
    assert!(limited >= 28, "only {limited} were refused");
}

// ── Seeding and token hygiene ────────────────────────────────────────────────

/// BUG: every owner unlock created an `admin` user when none was named "admin", so an
/// admin the owner had deleted (or renamed) came back, with a known password when
/// `UNV_ADMIN_PASSWORD` is set.
#[tokio::test]
async fn a_deleted_admin_does_not_come_back_on_the_next_unlock() {
    let d = scratch("admin");
    let state = AppState::new(
        d.join("vault.db"),
        d.join("vault.salt"),
        None,
        480,
        24,
        false,
    );
    let salt = [7u8; 16];
    std::fs::write(&state.salt_path, salt).unwrap();
    let key = vault_core::derive_key("owner-password-for-test", &salt).unwrap();
    let conn = open_db(&state.db_path, &key).unwrap();
    init_schema(&conn).unwrap();
    save_vault(
        &conn,
        json!({ "api_keys": [], "user_categories": [], "projects": [] }),
        vault_core::SaveCtx::default(),
    )
    .unwrap();
    drop(conn);
    let router = build_router(state.clone(), 8743);
    let unlock = || async {
        let (st, b) = call(
            &router,
            "POST",
            "/api/unlock",
            None,
            Some(json!({ "password": "owner-password-for-test" })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{b}");
        bearer(b["token"].as_str().unwrap())
    };
    let owner = unlock().await;
    let users = call(&router, "GET", "/api/users", Some(&owner), None)
        .await
        .1;
    let admin = users
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "admin")
        .expect("the first unlock seeds an admin")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (st, _) = call(
        &router,
        "DELETE",
        &format!("/api/users/{admin}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert_eq!(
        call(&router, "DELETE", "/api/unlock", Some(&owner), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let owner = unlock().await;
    let users = call(&router, "GET", "/api/users", Some(&owner), None)
        .await
        .1;
    assert!(
        !users
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["username"] == "admin"),
        "the deleted admin was recreated by the next unlock"
    );
}

/// BUG: a token's `expires_at` was stored as given and compared as text. "never" or
/// "tomorrow" sorts after any date and never expires; a bare date expires at the start
/// of that day.
#[tokio::test]
async fn a_token_expiry_that_is_not_a_timestamp_is_refused() {
    let rig = rig(vec![]);
    let u = make_user(&rig, "jack", "type:*", "type:*").await;
    for bad in ["never", "tomorrow", "zzz", "2026-13-45T00:00:00Z", ""] {
        let (st, body) = call(
            &rig.router,
            "POST",
            &format!("/api/users/{}/tokens", u.id),
            Some(&rig.owner),
            Some(json!({ "description": "x", "expires_at": bad })),
        )
        .await;
        if bad.is_empty() {
            continue; // an empty value means "no expiry" and is allowed
        }
        assert_eq!(st, StatusCode::BAD_REQUEST, "{bad:?} was accepted: {body}");
    }
    let (st, _) = call(
        &rig.router,
        "POST",
        &format!("/api/users/{}/tokens", u.id),
        Some(&rig.owner),
        Some(json!({ "description": "ok", "expires_at": "2099-01-01T00:00:00Z" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
}
