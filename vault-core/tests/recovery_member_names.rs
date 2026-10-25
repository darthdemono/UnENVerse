// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Restoring a recovery package must not let a member name leave its staging directory. The
//! manifest is not authenticated and anyone who knows the recipient can build a package, so
//! `../x` and absolute names used to be written wherever they pointed.

use age::secrecy::ExposeSecret;
use sha2::{Digest, Sha256};
use std::io::Write;

fn package(recipient: &age::x25519::Recipient, members: &[(&str, &[u8])]) -> Vec<u8> {
    let files: Vec<_> = members
        .iter()
        .map(|(n, b)| serde_json::json!({ "name": n, "sha256": hex::encode(Sha256::digest(b)), "size": b.len() }))
        .collect();
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format_version": 1, "app_version": "x", "created_at": "2026-01-01T00:00:00Z",
        "vault_version": null, "entry_count": 0, "files": files
    }))
    .unwrap();
    let mut out = b"UNVPKG1\n".to_vec();
    out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
    out.extend_from_slice(&manifest);
    let enc =
        age::Encryptor::with_recipients(std::iter::once(recipient as &dyn age::Recipient)).unwrap();
    let mut tar = tar::Builder::new(enc.wrap_output(&mut out).unwrap());
    let mut add = |name: &str, data: &[u8]| {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_entry_type(tar::EntryType::Regular);
        // The raw name field: `set_path` would refuse `..` itself.
        let gnu = h.as_gnu_mut().unwrap();
        gnu.name = [0; 100];
        gnu.name[..name.len()].copy_from_slice(name.as_bytes());
        h.set_cksum();
        tar.append(&h, data).unwrap();
    };
    add("manifest.json", &manifest);
    for (n, b) in members {
        add(n, b);
    }
    tar.into_inner().unwrap().finish().unwrap();
    out
}

#[test]
fn member_names_that_escape_the_staging_directory_are_refused_and_nothing_is_written() {
    let base = std::env::temp_dir().join(format!("unv-pkg-names-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("elsewhere")).unwrap();
    let identity = age::x25519::Identity::generate();
    let abs = base.join("elsewhere").join("absolute.bin");
    let abs_name = abs.to_string_lossy().to_string();
    assert!(
        abs_name.len() < 100,
        "temp path too long for the tar name field"
    );

    for bad in [
        "../escaped.bin",
        abs_name.as_str(),
        "a/../../escaped.bin",
        "a//b",
        "a\\b",
    ] {
        let pkg = package(
            &identity.to_public(),
            &[("vault.db", b"x"), ("vault.salt", b"y"), (bad, b"PROBE")],
        );
        let path = base.join("evil.unvpkg");
        std::fs::File::create(&path)
            .unwrap()
            .write_all(&pkg)
            .unwrap();
        let r = vault_core::recovery::restore_package_with_identity(
            &path,
            &base.join("target"),
            identity.to_string().expose_secret(),
        );
        assert!(r.is_err(), "`{bad}` was accepted");
        assert!(!base.join("escaped.bin").exists(), "`{bad}` escaped");
        assert!(!abs.exists(), "`{bad}` wrote an absolute path");
    }
    let _ = std::fs::remove_dir_all(&base);
}
