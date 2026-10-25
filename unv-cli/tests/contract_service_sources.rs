// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Service migration ("OpenBao, Infisical, Bitwarden Secrets Manager"): `unv import-service` reads a
//! service with the user's own credential. The services here are local mock servers and a stub
//! `bws`, so no account and no network are needed; the `unv` binary, its HTTP client, the planner,
//! the durable migration record and the encrypted archive are real.

mod common;
use common::{http, Vault};

const TOKEN: &str = "bao-token-for-the-test-9f3a";

fn bao() -> String {
    http::serve(|req| {
        let path = req.target.split('?').next().unwrap_or("");
        if req.header("x-vault-token") != Some(TOKEN) {
            return (403, r#"{"errors":["permission denied"]}"#.into());
        }
        match path {
            "/v1/sys/health" => (200, r#"{"version":"2.1.0"}"#.into()),
            "/v1/secret/metadata/" => (200, r#"{"data":{"keys":["db"]}}"#.into()),
            "/v1/secret/metadata/db" => (
                200,
                r#"{"data":{"current_version":2,"versions":{
                    "1":{"created_time":"2026-01-01T00:00:00Z","deletion_time":"","destroyed":false},
                    "2":{"created_time":"2026-02-01T00:00:00Z","deletion_time":"","destroyed":false}}}}"#
                    .into(),
            ),
            "/v1/secret/data/db" if req.target.contains("version=1") => {
                (200, r#"{"data":{"data":{"password":"first-password-0001"}}}"#.into())
            }
            "/v1/secret/data/db" => {
                (200, r#"{"data":{"data":{"password":"second-password-0002"}}}"#.into())
            }
            _ => (404, "{}".into()),
        }
    })
}

fn names(v: &Vault) -> Vec<String> {
    let o = v.run(&["list"]);
    if o.code != 0 {
        return Vec::new();
    }
    o.json()["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["provider"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn openbao_previews_without_values_applies_with_history_and_a_rerun_is_a_no_op() {
    let v = Vault::new("bao");
    let base = bao();
    let env = [("BAO_TOKEN", TOKEN)];
    let args = ["import-service", "openbao", "--addr", base.as_str()];

    let o = v.run_env(&env, &args);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(o.json()["data"]["counts"]["imported"], 1, "{}", o.stdout);
    assert!(names(&v).is_empty(), "nothing is written without --apply");
    assert!(
        !o.all().contains("second-password") && !o.all().contains(TOKEN),
        "{}",
        o.all()
    );

    let mut apply = args.to_vec();
    apply.push("--apply");
    let o = v.run_env(&env, &apply);
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(names(&v), vec!["db"]);
    assert_eq!(v.reveal_field("db", "password"), "second-password-0002");
    let shown = v.ok(&["get", "db"])["entries"][0].clone();
    assert_eq!(
        shown["version_history"].as_array().map(Vec::len),
        Some(1),
        "the first version is kept as history: {shown}"
    );
    let again = v.run_env(&env, &apply);
    assert_eq!(
        again.json()["data"]["counts"]["imported"],
        0,
        "{}",
        again.stdout
    );
}

#[test]
fn a_wrong_token_fails_clearly_without_echoing_it_and_writes_nothing() {
    let v = Vault::new("baobad");
    let base = bao();
    let o = v.run_env(
        &[("BAO_TOKEN", "a-wrong-token-never-print-this")],
        &[
            "import-service",
            "openbao",
            "--addr",
            base.as_str(),
            "--apply",
        ],
    );
    assert_ne!(o.code, 0, "{}", o.all());
    assert!(!o.all().contains("never-print-this"), "{}", o.all());
    assert!(names(&v).is_empty());
}

#[test]
fn a_missing_credential_says_where_to_put_it() {
    let v = Vault::new("noauth");
    let o = v.run_env(
        &[("BAO_TOKEN", ""), ("VAULT_TOKEN", "")],
        &["import-service", "openbao", "--addr", "http://127.0.0.1:1"],
    );
    assert_ne!(o.code, 0);
    assert!(o.all().contains("BAO_TOKEN"), "{}", o.all());
}

#[test]
fn infisical_expands_references_and_never_stores_the_reference_syntax() {
    let v = Vault::new("infisical");
    let base = http::serve(|req| {
        let path = req.target.split('?').next().unwrap_or("");
        match path {
            "/api/v4/secrets" if req.header("authorization") == Some("Bearer inf-token-1") => {
                if req.target.contains("expandSecretReferences=true") {
                    (200, r#"{"secrets":[
                        {"secretKey":"HOST","secretValue":"db.example.test","secretPath":"/"},
                        {"secretKey":"URL","secretValue":"postgres://db.example.test/app","secretPath":"/"}]}"#.into())
                } else {
                    (200, r#"{"secrets":[
                        {"secretKey":"HOST","secretValue":"db.example.test","secretPath":"/"},
                        {"secretKey":"URL","secretValue":"postgres://${HOST}/app","secretPath":"/"}]}"#.into())
                }
            }
            _ => (401, "{}".into()),
        }
    });
    let o = v.run_env(
        &[("INFISICAL_TOKEN", "inf-token-1")],
        &[
            "import-service",
            "infisical",
            "--addr",
            base.as_str(),
            "--project-id",
            "p1",
            "--environment",
            "prod",
            "--apply",
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(names(&v), vec!["p1/prod"]);
    assert_eq!(
        v.reveal_field("p1/prod", "URL"),
        "postgres://db.example.test/app"
    );
    assert!(o.all().contains("URL refers to HOST"), "{}", o.all());
}

#[cfg(unix)]
#[test]
fn bitwarden_secrets_manager_runs_the_users_bws_with_the_token_in_its_environment_only() {
    use std::os::unix::fs::PermissionsExt;
    let v = Vault::new("bws");
    let bin = v.path("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("bws");
    std::fs::write(
        &script,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "bws 1.0.0"; exit 0; fi
[ "$BWS_ACCESS_TOKEN" = "bws-token-1" ] || exit 1
case "$1 $2" in
  "project list") printf '[{"id":"p1","name":"Infra"}]';;
  "secret list") printf '[{"id":"s1","projectId":"p1","key":"API_TOKEN","value":"value-from-bws-0123456789","note":"n"}]';;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let o = v.run_env(
        &[("PATH", path.as_str()), ("BWS_ACCESS_TOKEN", "bws-token-1")],
        &["import-service", "bws", "--apply"],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert_eq!(names(&v), vec!["API_TOKEN"]);
    assert_eq!(
        v.reveal_field("API_TOKEN", "api_key"),
        "value-from-bws-0123456789"
    );
}

#[cfg(not(feature = "aws"))]
#[test]
fn aws_without_the_feature_says_the_build_has_no_aws_support() {
    let v = Vault::new("awsoff");
    let o = v.run_env(
        &[
            ("AWS_ACCESS_KEY_ID", "AKIDEXAMPLE"),
            ("AWS_SECRET_ACCESS_KEY", "secret"),
        ],
        &["import-service", "aws-sm", "--region", "eu-west-1"],
    );
    assert_ne!(o.code, 0);
    assert!(o.all().contains("no AWS support"), "{}", o.all());
}
