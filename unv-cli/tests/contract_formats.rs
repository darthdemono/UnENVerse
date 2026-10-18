//! The materialising formats ("Web sessions, composites and bundles", `unv emit`,
//! `unv cookie`, `unv cxf`): each writes a credential in the shape the consuming
//! tool reads, and each follows the one output rule: refused to a terminal
//! unless `--reveal`, written 0600 by `--out`.

mod common;
use common::Vault;

const SESSION: &str = "abc123secretcookie";

fn jar(tag: &str) -> Vault {
    let v = Vault::new(tag);
    let f = v.path("capture.txt");
    std::fs::write(
        &f,
        format!(
            "Set-Cookie: sessionid={SESSION}; Path=/; Domain=example.com; Secure; HttpOnly\n\
             Set-Cookie: csrf=xyz789; Path=/; Domain=example.com\n"
        ),
    )
    .unwrap();
    let o = v.run(&[
        "cookie",
        "import",
        f.to_str().unwrap(),
        "--from",
        "set-cookie",
        "--entry",
        "Jar",
        "--create",
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    v.ok(&["entry", "set", "Jar", "--user-agent", "TestAgent/1.0"]);
    v
}

fn revealed(v: &Vault, form: &str) -> String {
    let o = v.run_with(&["--reveal"], &["cookie", form, "Jar"]);
    assert_eq!(o.code, 0, "{form}: {}", o.all());
    o.stdout
}

// ---- cookies

#[test]
fn a_cookie_jar_comes_out_in_each_form_a_tool_wants() {
    let v = jar("cookies");
    let header = revealed(&v, "header");
    assert_eq!(header.trim(), format!("sessionid={SESSION}; csrf=xyz789"));

    let txt = revealed(&v, "txt");
    assert!(txt.starts_with("# Netscape HTTP Cookie File"), "{txt}");
    assert!(txt.contains(&format!("sessionid\t{SESSION}")), "{txt}");
    assert!(
        txt.contains("#HttpOnly_.example.com"),
        "HttpOnly is kept: {txt}"
    );

    let json: serde_json::Value = serde_json::from_str(&revealed(&v, "json")).unwrap();
    let names: Vec<&str> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["sessionid", "csrf"], "{json}");
}

#[test]
fn a_curl_command_for_a_session_carries_the_jar_and_the_user_agent() {
    let v = jar("cookiecurl");
    let curl = revealed(&v, "curl");
    assert!(curl.starts_with("curl "), "{curl}");
    assert!(
        curl.contains(&format!("Cookie: sessionid={SESSION}; csrf=xyz789")),
        "a command that does not carry the jar does not authenticate: {curl}"
    );
    assert!(
        curl.contains("User-Agent: TestAgent/1.0"),
        "the session was minted against one: {curl}"
    );
}

#[test]
fn every_cookie_form_is_refused_to_the_terminal_without_reveal() {
    let v = jar("cookiemask");
    for form in ["header", "txt", "json", "curl"] {
        let o = v.run(&["cookie", form, "Jar"]);
        assert_eq!(o.code, 9, "{form}: {}", o.all());
        assert!(!o.all().contains(SESSION), "{form} leaked: {}", o.all());
    }
}

#[cfg(unix)]
#[test]
fn a_cookie_file_is_written_owner_only() {
    use common::mode_of;
    let v = jar("cookiefile");
    for form in ["header", "txt", "json", "curl"] {
        let out = v.path(&format!("jar.{form}"));
        let o = v.run(&["cookie", form, "Jar", "--out", out.to_str().unwrap()]);
        assert_eq!(o.code, 0, "{form}: {}", o.all());
        assert!(
            std::fs::read_to_string(&out).unwrap().contains(SESSION),
            "{form}"
        );
        assert_eq!(mode_of(&out), 0o600, "{form}");
        assert!(
            !o.all().contains(SESSION),
            "{form} printed it while writing the file"
        );
    }
}

#[test]
fn an_entry_with_no_cookies_is_not_found_rather_than_an_empty_file() {
    let v = Vault::new("nocookies");
    v.ok(&["entry", "add", "Plain", "--key", "a-plain-key-1234567"]);
    let o = v.run_with(&["--reveal"], &["cookie", "header", "Plain"]);
    assert_eq!(o.code, 3, "{}", o.all());
}

// ---- emit

fn registry(v: &Vault) {
    v.ok(&[
        "entry",
        "add",
        "Reg",
        "--type",
        "registry_token",
        "--key",
        "npm_secret_value_123456",
        "--username",
        "ci",
        "--url",
        "https://registry.example.org",
    ]);
}

#[test]
fn emit_lists_the_formats_an_entry_type_offers_and_refuses_one_it_does_not() {
    let v = Vault::new("emitlist");
    registry(&v);
    let formats = v.ok(&["emit", "Reg"])["formats"].clone();
    let names: Vec<&str> = formats
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    for want in ["npmrc", "netrc", "docker-config"] {
        assert!(names.contains(&want), "{want} in {names:?}");
    }
    let o = v.run_with(&["--reveal"], &["emit", "Reg", "--as", "wifi-uri"]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert!(
        o.all().contains("offers"),
        "the error names what the entry does offer: {}",
        o.all()
    );

    // A type with nothing to emit says so instead of failing.
    v.ok(&["entry", "add", "Plain", "--key", "a-plain-key-1234567"]);
    assert_eq!(v.ok(&["emit", "Plain"])["formats"], serde_json::json!([]));
}

#[test]
fn emit_writes_the_file_each_tool_reads() {
    let v = Vault::new("emitshapes");
    registry(&v);
    let text = |fmt: &str| {
        let o = v.run_with(&["--reveal"], &["emit", "Reg", "--as", fmt]);
        assert_eq!(o.code, 0, "{fmt}: {}", o.all());
        o.stdout
    };
    assert!(
        text("npmrc").contains("//registry.example.org/:_authToken=npm_secret_value_123456"),
        "npmrc: {}",
        text("npmrc")
    );
    let netrc = text("netrc");
    assert!(
        netrc.contains("machine registry.example.org")
            && netrc.contains("password npm_secret_value_123456"),
        "{netrc}"
    );
    let docker = text("docker-config");
    let doc: serde_json::Value = serde_json::from_str(&docker).unwrap();
    let auth = doc["auths"]["registry.example.org"]["auth"]
        .as_str()
        .unwrap();
    // "user:password" in base64, as the Docker config reads it.
    let expected = {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let input = b"ci:npm_secret_value_123456";
        let mut out = String::new();
        for chunk in input.chunks(3) {
            let n = (chunk[0] as u32) << 16
                | (*chunk.get(1).unwrap_or(&0) as u32) << 8
                | *chunk.get(2).unwrap_or(&0) as u32;
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    };
    assert_eq!(auth, expected, "{docker}");
}

#[test]
fn emit_follows_the_output_rule_and_writes_a_private_file() {
    let v = Vault::new("emitrule");
    registry(&v);
    let refused = v.run(&["emit", "Reg", "--as", "npmrc"]);
    assert_eq!(refused.code, 9, "{}", refused.all());
    assert!(
        !refused.all().contains("npm_secret_value_123456"),
        "{}",
        refused.all()
    );

    let out = v.path("npmrc");
    let o = v.run(&[
        "emit",
        "Reg",
        "--as",
        "npmrc",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(std::fs::read_to_string(&out)
        .unwrap()
        .contains("npm_secret_value_123456"));
    #[cfg(unix)]
    assert_eq!(common::mode_of(&out), 0o600);
}

#[test]
fn emit_refuses_to_write_a_file_with_a_hole_in_it() {
    let v = Vault::new("emithole");
    // A database entry with no host cannot make a DSN: better an error than a broken file.
    v.ok(&[
        "entry",
        "add",
        "Db",
        "--type",
        "database",
        "--key",
        "db-password-1234567",
        "--username",
        "u",
    ]);
    let o = v.run_with(&["--reveal"], &["emit", "Db", "--as", "dsn"]);
    assert_eq!(o.code, 10, "{}", o.all());
    assert!(
        o.all().to_lowercase().contains("host"),
        "name what is missing: {}",
        o.all()
    );
}

#[test]
fn a_wifi_entry_emits_the_qr_string() {
    let v = Vault::new("wifi");
    v.ok(&[
        "entry",
        "add",
        "HomeNet",
        "--type",
        "wifi",
        "--key",
        "wifi-pass-1234567",
    ]);
    let o = v.run_with(&["--reveal"], &["emit", "HomeNet", "--as", "wifi-uri"]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        o.stdout
            .starts_with("WIFI:T:WPA;S:HomeNet;P:wifi-pass-1234567;"),
        "{}",
        o.stdout
    );
}

// ---- CXF

#[test]
fn cxf_export_is_a_materialising_path_and_round_trips() {
    let v = Vault::new("cxf");
    v.ok(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "cxf-secret-value-1234",
        "--username",
        "octo",
        "--type",
        "password",
    ]);
    let refused = v.run(&["cxf", "export"]);
    assert_eq!(refused.code, 2, "--out is required: {}", refused.all());

    let file = v.path("vault.cxf");
    let o = v.run(&["cxf", "export", "--out", file.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains("cxf-secret-value-1234"), "{}", o.all());
    #[cfg(unix)]
    assert_eq!(common::mode_of(&file), 0o600, "it holds every credential");

    let fresh = Vault::new("cxf-in");
    let imported = fresh.run(&["cxf", "import", file.to_str().unwrap()]);
    assert_eq!(imported.code, 0, "{}", imported.all());
    assert_eq!(
        fresh.reveal_field("GitHub", "api_key"),
        "cxf-secret-value-1234"
    );
    let again = fresh.run(&["cxf", "import", file.to_str().unwrap()]);
    assert_eq!(again.code, 0, "{}", again.all());
}
