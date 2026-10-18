//! "Secrets that are never seen at all", `--dry-run` and idempotency, from the
//! README's CLI section: an orchestrator can create and rotate a secret
//! without ever holding it, can ask what a command would do without it doing
//! anything, and can re-run a provisioning script without overwriting a secret.

mod common;
use common::Vault;
use serde_json::Value;

fn fingerprint(v: &Vault, provider: &str) -> String {
    v.ok(&["get", provider, "--field", "api_key"])["value"]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string()
}

fn names(v: &Vault) -> Vec<String> {
    let mut n: Vec<String> = v.ok(&["list"])["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["provider"].as_str().unwrap().to_string())
        .collect();
    n.sort();
    n
}

#[test]
fn a_generated_secret_is_stored_and_never_printed() {
    let v = Vault::new("gen");
    let added = v.run(&["entry", "add", "Stripe", "--generate"]);
    assert_eq!(added.code, 0, "{}", added.all());
    let fp = added.json()["data"]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        fp.starts_with("sha256:"),
        "the caller gets a fingerprint back: {fp}"
    );

    // The value exists (so it was stored) and what was printed is not it.
    let real = v.reveal_field("Stripe", "api_key");
    assert!(
        real.len() >= 32,
        "a generated secret must be long enough to be one: {real:?}"
    );
    assert!(
        !added.all().contains(&real),
        "the generated value was printed"
    );
    assert_eq!(
        fingerprint(&v, "Stripe"),
        fp,
        "the printed fingerprint is the stored value's"
    );
}

#[test]
fn two_generated_secrets_differ() {
    let v = Vault::new("gen2");
    v.ok(&["entry", "add", "A", "--generate"]);
    v.ok(&["entry", "add", "B", "--generate"]);
    assert_ne!(fingerprint(&v, "A"), fingerprint(&v, "B"));
}

#[test]
fn rotating_with_generate_replaces_the_secret_keeps_the_old_one_in_history_and_prints_neither() {
    let v = Vault::new("rotate");
    v.ok(&["entry", "add", "Stripe", "--generate"]);
    let old_real = v.reveal_field("Stripe", "api_key");
    let old_fp = fingerprint(&v, "Stripe");

    let rotated = v.run(&["entry", "rotate", "Stripe", "--generate"]);
    assert_eq!(rotated.code, 0, "{}", rotated.all());
    let new_real = v.reveal_field("Stripe", "api_key");
    assert_ne!(new_real, old_real, "rotation must change the secret");
    assert_ne!(fingerprint(&v, "Stripe"), old_fp);
    assert_eq!(rotated.json()["data"]["secret_changed"], true);
    assert!(
        !rotated.all().contains(&new_real),
        "the new value was printed"
    );
    assert!(
        !rotated.all().contains(&old_real),
        "the old value was printed"
    );

    // The previous value is kept as a masked history version, and is the old one.
    let history = v.ok(&["entry", "history", "Stripe"]);
    assert_eq!(history["count"], 1);
    assert_eq!(
        history["versions"][0]["value"]["fingerprint"],
        old_fp.as_str()
    );
    assert!(!history.to_string().contains(&old_real));
}

#[test]
fn a_rotation_given_a_value_on_stdin_stores_it_without_echoing_it() {
    let v = Vault::new("rotate-stdin");
    v.ok(&["entry", "add", "Stripe", "--key", "first-value-1234567890"]);
    let o = v.run_stdin(
        &["entry", "rotate", "Stripe", "--stdin"],
        "second-value-0987654321\n",
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains("second-value-0987654321"), "{}", o.all());
    assert_eq!(
        v.reveal_field("Stripe", "api_key"),
        "second-value-0987654321"
    );
}

#[test]
fn a_key_read_from_stdin_never_needs_to_be_on_the_command_line() {
    let v = Vault::new("keystdin");
    let o = v.run_stdin(
        &["entry", "add", "Piped", "--key-stdin"],
        "piped-secret-value-42\n",
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains("piped-secret-value-42"));
    assert_eq!(v.reveal_field("Piped", "api_key"), "piped-secret-value-42");
}

#[test]
fn generators_report_a_fingerprint_not_the_value() {
    let v = Vault::new("gens");
    let pw = v.ok(&["gen", "password"]);
    assert_eq!(pw["value"]["redacted"], true);
    assert!(pw["entropy_bits"].as_f64().unwrap() >= 100.0, "{pw}");
    let sec = v.ok(&["gen", "secret"]);
    assert_eq!(sec["value"]["redacted"], true);
    assert!(sec["value"]["length"].as_u64().unwrap() >= 32);
}

// ---- --dry-run: "a command that forgets to check the flag still cannot write".

#[test]
fn dry_run_reports_what_would_happen_and_writes_nothing() {
    let v = Vault::new("dry");
    v.ok(&["entry", "add", "Keep", "--key", "keep-value-1"]);
    let before = (names(&v), fingerprint(&v, "Keep"));

    let attempts: Vec<Vec<&str>> = vec![
        vec!["entry", "add", "Dry", "--key", "dry-value"],
        vec!["entry", "set", "Keep", "--key", "changed-value"],
        vec!["entry", "rotate", "Keep", "--generate"],
        vec!["entry", "rm", "Keep"],
        vec!["project", "add", "Ghost"],
    ];
    for args in attempts {
        let o = v.run_with(&["--dry-run"], &args);
        assert_eq!(o.code, 0, "{args:?}: {}", o.all());
        assert_eq!(o.json()["dry_run"], true, "{args:?}: {}", o.stdout);
    }

    assert_eq!(
        (names(&v), fingerprint(&v, "Keep")),
        before,
        "a dry run changed the vault"
    );
    let projects = v.ok(&["project", "ls"]);
    assert!(!projects.to_string().contains("Ghost"), "{projects}");
}

// ---- Idempotency: re-running a provisioning script is not an error.

#[test]
fn add_if_missing_does_not_overwrite_an_existing_secret() {
    let v = Vault::new("ifmissing");
    v.ok(&["entry", "add", "GitHub", "--key", "original-value"]);
    let o = v.run(&[
        "entry",
        "add",
        "GitHub",
        "--key",
        "replacement",
        "--if-missing",
    ]);
    assert_eq!(
        o.code,
        0,
        "re-running a provisioning script is not an error: {}",
        o.all()
    );
    assert_eq!(o.json()["data"]["created"], false);
    assert_eq!(v.reveal_field("GitHub", "api_key"), "original-value");
    // And when it is missing, it is created.
    let fresh = v.run(&[
        "entry",
        "add",
        "Fresh",
        "--key",
        "fresh-value",
        "--if-missing",
    ]);
    assert_eq!(fresh.json()["data"]["created"], true);
    assert_eq!(v.reveal_field("Fresh", "api_key"), "fresh-value");
}

#[test]
fn set_with_create_creates_when_missing_and_changes_when_present() {
    let v = Vault::new("setcreate");
    let first = v.run(&["entry", "set", "Brand", "--key", "one-value", "--create"]);
    assert_eq!(first.code, 0, "{}", first.all());
    assert_eq!(v.reveal_field("Brand", "api_key"), "one-value");
    let second = v.run(&["entry", "set", "Brand", "--key", "two-value", "--create"]);
    assert_eq!(second.code, 0, "{}", second.all());
    assert_eq!(v.reveal_field("Brand", "api_key"), "two-value");
    assert_eq!(
        names(&v),
        vec!["Brand".to_string()],
        "no duplicate was made"
    );
}

#[test]
fn project_add_if_missing_is_safe_to_repeat() {
    let v = Vault::new("projmissing");
    v.ok(&["project", "add", "Web"]);
    let again = v.run(&["project", "add", "Web", "--if-missing"]);
    assert_eq!(again.code, 0, "{}", again.all());
    let plain = v.run(&["project", "add", "Web"]);
    assert_eq!(
        plain.code,
        6,
        "without the flag a repeat is a conflict: {}",
        plain.all()
    );
    let list: Value = v.ok(&["project", "ls"]);
    let count = list.to_string().matches("\"Web\"").count();
    assert_eq!(count, 1, "{list}");
}
