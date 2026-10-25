// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The Rust half of the edge-case exporter parity check.
//!
//! `tests/cli-parity-edge.test.ts` writes what the desktop app produces for the
//! awkward projects in `tests/fixtures/parity/vault-edge.json` into
//! `edge-exports.json`; the CLI's exporters must produce the same bytes. If this
//! fails after an intended change, regenerate with
//! `PARITY_UPDATE=1 npx vitest run tests/cli-parity-edge.test.ts` and read the diff.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use unv_app::exporters;
use unv_app::refs::Resolver;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("parity")
}

fn load(name: &str) -> Value {
    let raw = std::fs::read_to_string(fixtures().join(name)).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn project<'a>(v: &'a Value, id: &str) -> &'a Value {
    v["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap_or_else(|| panic!("no project {id}"))
}

fn render(v: &Value, r: &Resolver) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut put = |k: &str, s: String| {
        out.insert(k.to_string(), s);
    };
    put(
        "wireguard",
        exporters::export_wireguard(project(v, "vpn"), r),
    );
    let compose = exporters::export_docker_compose(project(v, "stack"), r);
    put("compose.yaml", compose.yaml);
    put("compose.env", compose.env_file);
    put("nginx", exporters::export_nginx(project(v, "edge"), r));
    put("k8s", exporters::export_k8s(project(v, "k8s"), r));
    put("ssh", exporters::export_ssh_config(project(v, "ssh"), r));
    put(
        "traefik",
        exporters::export_traefik(project(v, "traefik"), r),
    );
    put(
        "traefik-empty",
        exporters::export_traefik(project(v, "traefik-none"), r),
    );
    put("apache", exporters::export_apache(project(v, "apache"), r));
    put(
        "haproxy",
        exporters::export_haproxy(project(v, "haproxy"), r),
    );
    put(
        "ansible-tasks",
        exporters::export_ansible(project(v, "ansible-tasks"), r),
    );
    put(
        "ansible-vars",
        exporters::export_ansible(project(v, "ansible-vars"), r),
    );
    put(
        "ansible-none",
        exporters::export_ansible(project(v, "ansible-none"), r),
    );
    put("pg", exporters::export_postgres(project(v, "pg"), r));
    for p in v["projects"].as_array().unwrap() {
        for c in p["chunks"].as_array().into_iter().flatten() {
            put(
                &format!(
                    "chunk:{}:{}",
                    p["id"].as_str().unwrap(),
                    c["id"].as_str().unwrap()
                ),
                exporters::chunk_to_string(c, r),
            );
        }
    }
    out
}

#[test]
fn edge_cases_match_the_app() {
    let v = load("vault-edge.json");
    let r = Resolver::from_parts(
        v["api_keys"].as_array().unwrap().clone(),
        v["projects"].as_array().unwrap().clone(),
        "api_key",
        false,
    );
    let want: BTreeMap<String, String> = load("edge-exports.json")
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, s)| (k.clone(), s.as_str().unwrap().to_string()))
        .collect();
    let got = render(&v, &r);
    assert_eq!(
        got.keys().collect::<Vec<_>>(),
        want.keys().collect::<Vec<_>>(),
        "the CLI and the app render different sets of outputs"
    );
    let mut differing = Vec::new();
    for (k, expected) in &want {
        if &got[k] != expected {
            differing.push(format!(
                "--- {k} (app)\n{expected}\n+++ {k} (cli)\n{}",
                got[k]
            ));
        }
    }
    assert!(
        differing.is_empty(),
        "{} output(s) differ:\n{}",
        differing.len(),
        differing.join("\n\n")
    );
}
