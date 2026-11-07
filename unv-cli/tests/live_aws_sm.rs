// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv import-service aws-sm` against LocalStack (R11), the check that the importer's own SigV4
//! requests and response parsing work against a real server rather than a mock built from the
//! documentation. It runs only with the `aws` feature and `UNV_LIVE_LOCALSTACK_ENDPOINT` set, which
//! the `Live services` workflow does after seeding a secret `live/db`; a plain `cargo test` skips it.
#![cfg(feature = "aws")]

mod common;
use common::Vault;

#[test]
fn a_real_secrets_manager_secret_is_imported_with_its_value() {
    let Ok(endpoint) = std::env::var("UNV_LIVE_LOCALSTACK_ENDPOINT") else {
        eprintln!("skipped: UNV_LIVE_LOCALSTACK_ENDPOINT not set");
        return;
    };
    let v = Vault::new("live-aws");
    let env = [
        ("AWS_ACCESS_KEY_ID", "test"),
        ("AWS_SECRET_ACCESS_KEY", "test"),
    ];
    let o = v.run_env(
        &env,
        &[
            "import-service",
            "aws-sm",
            "--region",
            "us-east-1",
            "--endpoint",
            endpoint.as_str(),
            "--apply",
        ],
    );
    assert_eq!(o.code, 0, "{}", o.all());
    assert!(
        !o.all().contains("live-aws-secret-value-0001"),
        "{}",
        o.all()
    );
    let out = v.db().with_file_name("live-aws.env");
    let e = v.run(&["export", "--out", out.to_str().unwrap()]);
    assert_eq!(e.code, 0, "{}", e.all());
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("live-aws-secret-value-0001"), "{text}");
}
