//! "Calendar feeds and the unique-ID registry" (README), through the real `unv`
//! and a real `unv-server --uid-registry`:
//!
//! * a feed URL is a bearer credential: written to a 0600 file, never printed,
//!   shown once; the feed lists expiries by name and never a value, RBAC is
//!   applied when it is fetched, and a revoked feed stops answering;
//! * the registry enforces uniqueness by reserving, not checking: `mint`
//!   returns a value that was actually stored, `register` fails closed on a
//!   collision, `lookup` knows an issued value and not an unknown one, and the
//!   registry is only ever reached through a server.

mod common;
#[path = "common/server.rs"]
mod server;

use common::Vault;
use server::{http_get, Server};

const SECRET: &str = "feed-secret-value-123456";
const PW: &str = "reader-password-12345";

fn date_in(days: i64) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let z = (secs + days * 86_400).div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn feed_vault(tag: &str) -> (Vault, Server) {
    let v = Vault::new(tag);
    v.ok(&["project", "add", "Web"]);
    v.ok(&["project", "add", "Api"]);
    v.ok(&[
        "entry",
        "add",
        "WebCert",
        "--key",
        SECRET,
        "--expires",
        &date_in(20),
        "--projects",
        "web",
    ]);
    v.ok(&[
        "entry",
        "add",
        "ApiCert",
        "--key",
        "other-secret-value-9999",
        "--expires",
        &date_in(25),
        "--projects",
        "api",
    ]);
    let s = Server::start(&v, false);
    (v, s)
}

fn mint_feed(v: &Vault, s: &Server, name: &str) -> String {
    let file = v.path(&format!("{name}.url"));
    let o = v.on(
        s,
        &[
            "calendar",
            "feed",
            "new",
            "--name",
            name,
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    std::fs::read_to_string(&file).unwrap().trim().to_string()
}

#[cfg(unix)]
#[test]
fn a_feed_url_goes_to_a_private_file_and_is_never_printed() {
    use common::mode_of;
    let (v, s) = feed_vault("feedurl");
    let file = v.path("feed.url");
    let o = v.on(
        &s,
        &[
            "calendar",
            "feed",
            "new",
            "--name",
            "Team",
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    let url = std::fs::read_to_string(&file).unwrap().trim().to_string();
    assert!(url.contains("/ics/") && url.ends_with(".ics"), "{url}");
    assert!(
        !o.all().contains(&url),
        "the feed URL is a bearer credential: {}",
        o.all()
    );
    assert_eq!(mode_of(&file), 0o600);

    let ls = v.on(&s, &["calendar", "feed", "ls"]);
    assert_eq!(ls.code, 0, "{}", ls.all());
    assert!(ls.stdout.contains("Team"), "{}", ls.stdout);
    let token = url.rsplit('/').next().unwrap().trim_end_matches(".ics");
    assert!(
        !ls.all().contains(token),
        "the token is shown once, never listed again: {}",
        ls.all()
    );
}

#[test]
fn the_feed_lists_expiries_by_name_and_never_a_value() {
    let (v, s) = feed_vault("feedbody");
    let url = mint_feed(&v, &s, "Team");
    let (status, headers, body) = http_get(&url);
    assert_eq!(status, 200, "{headers}\n{body}");
    assert!(headers.contains("text/calendar"), "{headers}");
    assert!(
        body.contains("BEGIN:VCALENDAR") && body.contains("BEGIN:VEVENT"),
        "{body}"
    );
    assert!(
        body.contains("WebCert") && body.contains("ApiCert"),
        "expiries appear as events: {body}"
    );
    for secret in [SECRET, "other-secret-value-9999"] {
        assert!(
            !body.contains(secret),
            "a feed shows expiries, never a value: {body}"
        );
    }
}

#[test]
fn a_revoked_feed_stops_answering_and_an_unknown_token_is_not_a_calendar() {
    let (v, s) = feed_vault("feedrevoke");
    let url = mint_feed(&v, &s, "Team");
    assert_eq!(http_get(&url).0, 200);

    let ls = v.on(&s, &["calendar", "feed", "ls"]).json();
    let id = ls["data"]["feeds"][0]["id"]
        .as_str()
        .expect("a feed id")
        .to_string();
    let r = v.on(&s, &["calendar", "feed", "revoke", &id]);
    assert_eq!(r.code, 0, "{}", r.all());
    let after = http_get(&url).0;
    assert!(
        after == 404 || after == 410,
        "a revoked feed must not answer: {after}"
    );

    let bogus = format!("{}/ics/{}.ics", s.url, "0".repeat(64));
    let st = http_get(&bogus).0;
    assert!(
        st == 404 || st == 401,
        "an unknown token is not a calendar: {st}"
    );
}

#[test]
fn a_feed_minted_by_a_scoped_user_only_shows_what_that_user_may_read() {
    let (v, s) = feed_vault("feedscope");
    assert_eq!(
        v.on(&s, &["user", "add", "reader", "--user-password", PW])
            .code,
        0
    );
    assert_eq!(
        v.on(
            &s,
            &["perm", "set", "user", "reader", "--read", "project:web"]
        )
        .code,
        0
    );
    let file = v.path("reader.url");
    let o = v.as_user(
        &s,
        "reader",
        PW,
        &[
            "calendar",
            "feed",
            "new",
            "--name",
            "Mine",
            "--out",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    let url = std::fs::read_to_string(&file).unwrap().trim().to_string();
    let (status, _h, body) = http_get(&url);
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("WebCert"), "{body}");
    assert!(
        !body.contains("ApiCert"),
        "RBAC is applied when the feed is fetched: {body}"
    );

    // Tightening the grant afterwards shrinks the feed on the next fetch.
    v.on(
        &s,
        &["perm", "set", "user", "reader", "--read", "project:nothing"],
    );
    let (_s2, _h2, shrunk) = http_get(&url);
    assert!(
        !shrunk.contains("WebCert"),
        "a permission revoked later shrinks the feed: {shrunk}"
    );
}

// ---- unique-ID registry

fn registry(tag: &str) -> (Vault, Server) {
    let v = Vault::new(tag);
    v.ok(&["entry", "add", "Seed", "--key", "a-seed-secret-value-1"]);
    let s = Server::start_with(&v, false, true);
    (v, s)
}

#[test]
fn the_registry_is_only_reachable_through_a_server() {
    let v = Vault::new("uidlocal");
    let o = v.run(&["uid", "stats"]);
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(o.all().to_lowercase().contains("server"), "{}", o.all());
}

#[test]
fn register_reserves_a_value_and_a_second_registration_collides() {
    let (v, s) = registry("uidreg");
    let first = v.on(
        &s,
        &["uid", "register", "order-1001", "--namespace", "orders"],
    );
    assert_eq!(first.code, 0, "{}", first.all());
    let again = v.on(
        &s,
        &["uid", "register", "order-1001", "--namespace", "orders"],
    );
    assert_eq!(
        again.code,
        6,
        "uniqueness is enforced by the insert, not by a check: {}",
        again.all()
    );
    assert_eq!(again.error_code(), "conflict");
}

#[test]
fn lookup_knows_an_issued_value_and_not_an_unknown_one() {
    let (v, s) = registry("uidlookup");
    v.on(
        &s,
        &["uid", "register", "order-2002", "--namespace", "orders"],
    );
    let known = v.on(&s, &["uid", "lookup", "order-2002"]);
    assert_eq!(known.code, 0, "{}", known.all());
    assert_eq!(known.json()["data"]["issued"], true, "{}", known.stdout);
    assert_eq!(
        known.json()["data"]["namespace"],
        "orders",
        "the metadata the caller may see: {}",
        known.stdout
    );
    let unknown = v.on(&s, &["uid", "lookup", "never-issued"]);
    assert_eq!(unknown.code, 0, "{}", unknown.all());
    assert_eq!(
        unknown.json()["data"]["issued"],
        false,
        "{}",
        unknown.stdout
    );
    assert!(
        unknown.json()["data"]["namespace"].is_null(),
        "an unknown value reveals nothing: {}",
        unknown.stdout
    );
}

#[test]
fn check_is_advisory_and_does_not_reserve() {
    let (v, s) = registry("uidcheck");
    let before = v.on(&s, &["uid", "check", "order-3003"]);
    assert_eq!(before.code, 0, "{}", before.all());
    // Checking twice still says it is free: a check reserves nothing.
    let again = v.on(&s, &["uid", "check", "order-3003"]);
    assert_eq!(before.stdout, again.stdout, "{}", again.stdout);
    // And it can still be registered afterwards.
    assert_eq!(
        v.on(
            &s,
            &["uid", "register", "order-3003", "--namespace", "orders"]
        )
        .code,
        0
    );
}

#[test]
fn mint_returns_a_value_that_was_actually_stored_and_two_mints_differ() {
    let (v, s) = registry("uidmint");
    let a = v.on(
        &s,
        &["uid", "mint", "--namespace", "orders", "--length", "16"],
    );
    assert_eq!(a.code, 0, "{}", a.all());
    let b = v.on(
        &s,
        &["uid", "mint", "--namespace", "orders", "--length", "16"],
    );
    let av = a.json()["data"]["value"]
        .as_str()
        .expect("the minted value")
        .to_string();
    let bv = b.json()["data"]["value"].as_str().unwrap().to_string();
    assert_ne!(av, bv);
    let found = v.on(&s, &["uid", "lookup", &av]);
    assert_eq!(
        found.json()["data"]["issued"],
        true,
        "a minted value is stored: {}",
        found.stdout
    );
    let collide = v.on(&s, &["uid", "register", &av, "--namespace", "orders"]);
    assert_eq!(collide.code, 6, "{}", collide.all());
}

#[test]
fn stats_counts_what_was_registered() {
    let (v, s) = registry("uidstats");
    v.on(&s, &["uid", "register", "a-1", "--namespace", "n"]);
    v.on(&s, &["uid", "register", "a-2", "--namespace", "n"]);
    let st = v.on(&s, &["uid", "stats"]);
    assert_eq!(st.code, 0, "{}", st.all());
    let rows = st.json()["data"]["rows"]
        .as_i64()
        .or_else(|| st.json()["data"]["count"].as_i64());
    assert_eq!(rows, Some(2), "{}", st.stdout);
}
