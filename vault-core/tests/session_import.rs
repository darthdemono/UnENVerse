// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Web-session capture parsers, from the contract their module states: a paste is
//! attacker-controlled text, so nothing is fetched or run and the size is bounded;
//! only the chosen origin's session survives; whatever is left out (another
//! credential, a body, a malformed cookie) is reported by NAME and never by value;
//! and a paste that is not what it claims to be is refused with a reason rather
//! than imported as an empty capture.

use vault_core::session_import::{
    detect, parse_auto, parse_curl, parse_har, parse_powershell, parse_set_cookie, read_firefox,
    MAX_INPUT,
};

fn dropped(c: &vault_core::session_import::Capture) -> String {
    format!("{:?}", c.dropped)
}

// ---- cURL

#[test]
fn text_that_is_not_a_curl_command_or_has_no_web_address_is_refused() {
    assert_eq!(
        parse_curl("wget https://a.test").unwrap_err(),
        "not a curl command"
    );
    assert!(parse_curl("").is_err());
    assert_eq!(
        parse_curl("curl --compressed -s").unwrap_err(),
        "no http(s) URL in the curl command"
    );
    for url in ["ftp://a.test/x", "file:///etc/passwd", "a.test/no-scheme"] {
        assert!(
            parse_curl(&format!("curl '{url}'")).is_err(),
            "{url} is not an http(s) origin"
        );
    }
}

#[test]
fn an_oversized_paste_is_refused_before_it_is_parsed() {
    let big = format!("curl 'https://a.test' -H 'x: {}'", "a".repeat(MAX_INPUT));
    assert!(parse_curl(&big).unwrap_err().contains("too large"));
    assert!(parse_har(&"x".repeat(MAX_INPUT + 1), None)
        .unwrap_err()
        .contains("32 MiB"));
}

#[test]
fn credentials_in_the_address_never_become_part_of_the_origin() {
    let c = parse_curl("curl 'https://user:hunter2@api.example.com:8443/v1?x=1#f'").unwrap();
    assert_eq!(c.origin.as_deref(), Some("https://api.example.com:8443"));
    assert!(!format!("{c:?}").contains("hunter2"));
}

#[test]
fn other_credentials_in_a_curl_paste_are_dropped_by_name_only() {
    let c = parse_curl(
        "curl 'https://a.test/' -u admin:s3cretpw -H 'X-Api-Key: key-value-1' \
         -H 'Proxy-Authorization: Basic cHJveHk6cHc=' -H 'X-Auth-Token: tok-value-2' \
         -H 'Authorization: Bearer abc' -H 'Authorization: Bearer abc' \
         -X POST -e https://ref.test -m 5 --data-raw 'a=1' -F 'f=@x'",
    )
    .unwrap();
    let d = dropped(&c);
    for name in [
        "basic-auth credentials (-u)",
        "x-api-key",
        "Proxy-Authorization",
        "X-Auth-Token",
        "request body",
    ] {
        assert!(
            d.to_lowercase().contains(&name.to_lowercase()),
            "{name} should be reported in {d}"
        );
    }
    for value in [
        "s3cretpw",
        "key-value-1",
        "cHJveHk6cHc",
        "tok-value-2",
        "Bearer abc",
    ] {
        assert!(!format!("{c:?}").contains(value), "{value} leaked: {c:?}");
    }
    assert_eq!(
        c.dropped
            .iter()
            .filter(|d| d.starts_with("authorization") || d.starts_with("Authorization"))
            .count(),
        1,
        "the same header named twice is reported once: {d}"
    );
    // Values of flags that take one must not be mistaken for the URL.
    assert_eq!(c.origin.as_deref(), Some("https://a.test"));
}

#[test]
fn cookie_flags_keep_pairs_and_report_a_cookie_file_reference() {
    let c = parse_curl("curl -b 'sid=abc; theme=dark' https://a.test/").unwrap();
    assert_eq!(c.cookies.len(), 2);
    let file = parse_curl("curl --cookie jar.txt https://a.test/").unwrap();
    assert!(file.cookies.is_empty());
    assert!(dropped(&file).contains("cookie file reference"));
}

#[test]
fn a_cookie_with_a_forbidden_character_is_dropped_by_name_and_a_later_value_wins() {
    let c = parse_curl(
        "curl 'https://a.test/' -H 'cookie: a=1; bad=x\"y; =novalue; flag; a=2; semi=ok'",
    )
    .unwrap();
    let names: Vec<_> = c.cookies.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["a", "semi"], "{c:?}");
    assert_eq!(
        c.cookies[0].value, "2",
        "a later capture of the same cookie wins"
    );
    assert!(dropped(&c).contains("cookie bad (illegal character in value)"));
    assert!(!dropped(&c).contains("x\"y"));
}

#[test]
fn quotes_do_not_make_a_space_legal_in_a_cookie_value() {
    let c = parse_curl("curl 'https://a.test/' -H 'cookie: q=\"a b\"; ok=\"fine\"'").unwrap();
    let names: Vec<_> = c.cookies.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["ok"], "{c:?}");
    assert!(dropped(&c).contains("cookie q (illegal character in value)"));
}

#[test]
fn bash_quoting_forms_are_decoded_the_way_a_shell_would() {
    let c = parse_curl(
        "curl \\\n  'https://a.test/' \\\n  -A $'caf\\u00e9 \\x41\\t\\n\\r\\\\z\\q' \\\n  --url \"https://b.test/\\\"x\"",
    )
    .unwrap();
    // `--url` names the address explicitly and wins over the positional one.
    assert_eq!(c.origin.as_deref(), Some("https://b.test"));
    assert_eq!(c.user_agent.as_deref(), Some("café A\t\n\r\\zq"));
}

#[test]
fn cmd_flavoured_curl_with_line_continuations_is_read() {
    let c = parse_curl(
        "curl.exe ^\"https://www.example.com/a^\" ^\r\n  -H ^\"cookie: sid=abc^\" ^\n  -H ^\"x-trace: 1^\"",
    )
    .unwrap();
    assert_eq!(c.origin.as_deref(), Some("https://www.example.com"));
    assert_eq!(c.cookies[0].value, "abc");
    assert_eq!(c.headers, [("x-trace".to_string(), "1".to_string())]);
}

#[test]
fn only_replay_relevant_headers_are_kept() {
    let c = parse_curl(
        "curl 'https://a.test/' -H 'Accept: x/y' -H 'Host: a.test' -H 'Content-Length: 3' \
         -H 'Accept-Encoding: gzip' -H 'Connection: keep-alive' -H 'Priority: u=1' -H 'TE: trailers' \
         -H 'Sec-Fetch-Site: same-origin' -H 'X-CSRF-Token: t' -H 'no-colon-here'",
    )
    .unwrap();
    let kept: Vec<_> = c.headers.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kept, ["Accept", "X-CSRF-Token"]);
}

// ---- PowerShell

#[test]
fn text_without_invoke_webrequest_or_a_uri_is_not_a_powershell_capture() {
    assert!(parse_powershell("curl https://a.test")
        .unwrap_err()
        .contains("not a PowerShell"));
    assert!(parse_powershell("Invoke-WebRequest -Method GET")
        .unwrap_err()
        .contains("no -Uri"));
}

#[test]
fn powershell_cookies_headers_agent_and_body_are_read_and_unescaped() {
    let ps = r#"$session = New-Object Microsoft.PowerShell.Commands.WebRequestSession
$session.UserAgent = "x"
$session.Cookies.Add((New-Object System.Net.Cookie("sid", "abc123", "/", "www.example.com")))
$session.Cookies.Add((New-Object System.Net.Cookie("bad", "a b", "/", "www.example.com")))
Invoke-WebRequest -UseBasicParsing -Uri "https://www.example.com/api" `
-WebSession $session `
-UserAgent "Agent `"quoted`" `$1" `
-Headers @{
"accept"="application/json"
"authorization"="Bearer tok"
"x-note"="a``b"
} `
-Body "a=1""#;
    let c = parse_powershell(ps).unwrap();
    assert_eq!(c.origin.as_deref(), Some("https://www.example.com"));
    let names: Vec<_> = c.cookies.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["sid"], "{c:?}");
    assert_eq!(c.cookies[0].path.as_deref(), Some("/"));
    assert_eq!(c.cookies[0].domain.as_deref(), Some("www.example.com"));
    assert_eq!(c.user_agent.as_deref(), Some("Agent \"quoted\" $1"));
    assert_eq!(
        c.headers,
        [
            ("accept".to_string(), "application/json".to_string()),
            ("x-note".to_string(), "a`b".to_string())
        ]
    );
    let d = dropped(&c);
    assert!(d.contains("authorization header") && d.contains("request body"));
    assert!(d.contains("cookie bad (illegal character in value)"));
    assert!(!d.contains("tok"));
}

// ---- HAR

fn har(entries: &str) -> String {
    format!(r#"{{"log":{{"entries":[{entries}]}}}}"#)
}

#[test]
fn a_document_that_is_not_a_har_is_refused_with_the_reason() {
    assert!(parse_har("not json", None)
        .unwrap_err()
        .contains("not valid JSON"));
    assert!(parse_har("{}", None)
        .unwrap_err()
        .contains("no log.entries"));
    assert!(parse_har(&har(""), None)
        .unwrap_err()
        .contains("no http(s) requests"));
    assert!(parse_har(&har(r#"{"request":{"url":"ftp://x/y"}}"#), None)
        .unwrap_err()
        .contains("no http(s) requests"));
    let one = har(r#"{"request":{"url":"https://a.test/"}}"#);
    assert!(parse_har(&one, Some("gopher://a.test"))
        .unwrap_err()
        .contains("is not an http(s) origin"));
}

#[test]
fn a_single_origin_har_needs_no_origin_and_a_chosen_one_is_honoured() {
    let h = har(
        r#"{"request":{"url":"https://a.test/x","cookies":[{"name":"sid","value":"1"},
            {"name":"bad","value":"a;b"},{"name":"novalue"}],
            "headers":[{"name":":authority","value":"a.test"},{"name":"X-Seq","value":"first"},
                       {"name":"Authorization","value":"Bearer zzz"}],
            "postData":{"text":"secret=1"}}},
        {"request":{"url":"https://a.test/y","headers":[{"name":"x-seq","value":"second"}],
            "postData":{"text":"again"}},
         "response":{"cookies":[{"name":"sid","value":"2","domain":"a.test","path":"/p",
            "secure":true,"httpOnly":true,"expires":"2030-01-01T00:00:00Z"},
            {"name":"nope","value":"a b"},{"name":"x"}]}}"#,
    );
    let c = parse_har(&h, None).unwrap();
    assert_eq!(c.origin.as_deref(), Some("https://a.test"));
    let sid = c.cookies.iter().find(|c| c.name == "sid").unwrap();
    assert_eq!(sid.value, "2", "the response cookie carries the attributes");
    assert!(sid.secure && sid.http_only);
    assert_eq!(sid.expires, 1_893_456_000);
    assert_eq!(sid.path.as_deref(), Some("/p"));
    assert_eq!(
        c.headers,
        [("x-seq".to_string(), "second".to_string())],
        "a later request wins by header name and pseudo-headers are not headers"
    );
    let d = dropped(&c);
    assert!(d.contains("cookie bad (illegal character in value)"));
    assert!(d.contains("Authorization header"));
    assert_eq!(
        c.dropped.iter().filter(|d| *d == "request body").count(),
        1,
        "the body is reported once: {d}"
    );
    assert!(!d.contains("secret=1") && !d.contains("zzz"));
    assert_eq!(
        parse_har(&h, Some("https://a.test/anything"))
            .unwrap()
            .origin,
        c.origin
    );
}

// ---- Set-Cookie

#[test]
fn pasted_text_with_no_cookie_in_it_is_refused() {
    for text in ["", "   \n# a comment\n", "Set-Cookie:\nno equals sign here"] {
        assert_eq!(
            parse_set_cookie(text, 0).unwrap_err(),
            "no Set-Cookie lines found",
            "{text:?}"
        );
    }
}

#[test]
fn set_cookie_lines_apply_the_browsers_own_rules() {
    let now = 1_700_000_000;
    let c = parse_set_cookie(
        "set-cookie: a=1; Domain=Example.com; Path=/x; Max-Age=0\n\
         Set-Cookie: b=2; Max-Age=-5\n\
         Set-Cookie: c=3; Expires=Wed, 21-Oct-2015 07:28:00 GMT\n\
         Set-Cookie: d=4; Expires=Wed, 21 Oct 2037 07:28:00 GMT; Domain=\n\
         Set-Cookie: e=5; Expires=garbage\n\
         # comment\n\
         __Secure-s=1\n\
         __Secure-t=1; Secure\n\
         __Host-h=1; Secure; Path=/; Domain=example.com\n\
         bad=a\\b\n\
         =noname",
        now,
    )
    .unwrap();
    let get = |n: &str| c.cookies.iter().find(|c| c.name == n);
    assert_eq!(get("a").unwrap().domain.as_deref(), Some(".Example.com"));
    assert_eq!(
        get("a").unwrap().expires,
        1,
        "Max-Age 0 means already expired"
    );
    assert_eq!(get("b").unwrap().expires, 1);
    assert_eq!(
        get("c").unwrap().expires,
        1_445_412_480,
        "obsolete date spelling"
    );
    assert!(get("d").unwrap().expires > now && get("d").unwrap().domain.is_none());
    assert_eq!(
        get("e").unwrap().expires,
        0,
        "an unreadable date is a session cookie"
    );
    assert!(get("__Secure-s").is_none() && get("__Secure-t").is_some());
    assert!(get("__Host-h").is_none());
    assert!(get("bad").is_none());
    let d = dropped(&c);
    assert!(d.contains("__Secure-s (violates the __Secure- prefix rules)"));
    assert!(d.contains("__Host-h (violates the __Host- prefix rules)"));
    assert!(d.contains("cookie bad (illegal character in value)"));
}

// ---- Firefox, detect, auto

#[test]
fn a_file_that_is_not_a_firefox_cookie_store_is_refused() {
    let dir = std::env::temp_dir().join(format!("unv-si-ff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let missing = read_firefox(&dir.join("absent.sqlite"), "a.test").unwrap_err();
    assert!(missing.contains("cannot read"), "{missing}");
    let other = dir.join("other.sqlite");
    rusqlite::Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE unrelated (x TEXT);")
        .unwrap();
    let err = read_firefox(&other, "a.test").unwrap_err();
    assert!(err.contains("not a Firefox cookies.sqlite"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_paste_is_sniffed_and_routed_to_the_matching_parser() {
    assert_eq!(detect("  curl 'https://a.test'"), "curl");
    assert_eq!(
        detect("x\nInvoke-RestMethod -Uri \"https://a.test\""),
        "powershell"
    );
    assert_eq!(detect("{\"log\": {}}"), "har");
    assert_eq!(detect("{\"other\": 1}"), "set-cookie");
    assert!(parse_auto("curl 'https://a.test/' -b 'k=v'", None, 0).is_ok());
    assert!(parse_auto("Invoke-WebRequest -Uri \"https://a.test/\"", None, 0).is_ok());
    assert!(parse_auto(&har(r#"{"request":{"url":"https://a.test/"}}"#), None, 0).is_ok());
    assert_eq!(
        parse_auto("k=v; Secure", None, 5).unwrap().cookies[0].name,
        "k"
    );
}
