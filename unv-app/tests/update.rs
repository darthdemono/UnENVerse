//! Phase 54: `unv update` refuses everything unverified and never loses the
//! working executable. All fixtures are local; no network.

use ed25519_dalek::SigningKey;
use std::io::Write;
use std::path::{Path, PathBuf};
use unv_app::update_cmd::{extract_binary, install, replace_executable, run_with, UpdateCmd};
use vault_core::update::{self as upd, Asset, Manifest};

const SEED: [u8; 32] = [11; 32];

fn key() -> [u8; 32] {
    SigningKey::from_bytes(&SEED).verifying_key().to_bytes()
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("unv-update-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn targz(name: &str, body: &[u8]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_size(body.len() as u64);
    h.set_mode(0o755);
    h.set_cksum();
    tar.append_data(&mut h, name, body).unwrap();
    let raw = tar.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&raw).unwrap();
    gz.finish().unwrap()
}

fn manifest_for(archive: &[u8], version: &str) -> Manifest {
    Manifest {
        schema: upd::SCHEMA,
        version: version.into(),
        published_at: "2026-10-17T12:00:00Z".into(),
        notes: "n".into(),
        assets: vec![Asset {
            target: upd::current_target(),
            url: "https://example.test/unv.tar.gz".into(),
            sha256: upd::sha256_hex(archive),
            size: archive.len() as u64,
        }],
    }
}

fn script(version: &str) -> Vec<u8> {
    format!("#!/bin/sh\necho unv {version}\n").into_bytes()
}

fn old_exe(dir: &Path) -> PathBuf {
    let p = dir.join("unv");
    std::fs::write(&p, script("0.1.0")).unwrap();
    p
}

fn ok_probe(_: &Path) -> Result<(), String> {
    Ok(())
}

#[test]
fn installs_a_matching_archive_and_keeps_no_rollback() {
    let d = tmp("ok");
    let exe = old_exe(&d);
    let a = targz("./unv", &script("0.2.0"));
    install(
        &manifest_for(&a, "0.2.0"),
        &upd::current_target(),
        &a,
        &exe,
        ok_probe,
    )
    .unwrap();
    assert!(std::fs::read_to_string(&exe).unwrap().contains("0.2.0"));
    assert!(!d.join("unv.rollback").exists());
}

#[test]
fn wrong_checksum_and_truncated_archive_leave_the_old_executable() {
    let d = tmp("bad");
    let exe = old_exe(&d);
    let a = targz("unv", &script("0.2.0"));
    let mut m = manifest_for(&a, "0.2.0");
    m.assets[0].sha256 = upd::sha256_hex(b"other");
    let e = install(&m, &upd::current_target(), &a, &exe, ok_probe).unwrap_err();
    assert!(e.contains("checksum"), "{e}");
    let m = manifest_for(&a, "0.2.0");
    let e = install(
        &m,
        &upd::current_target(),
        &a[..a.len() / 2],
        &exe,
        ok_probe,
    )
    .unwrap_err();
    assert!(e.contains("bytes"), "{e}");
    assert!(std::fs::read_to_string(&exe).unwrap().contains("0.1.0"));
}

#[test]
fn a_failed_probe_restores_the_previous_executable() {
    let d = tmp("probe");
    let exe = old_exe(&d);
    let a = targz("unv", &script("0.2.0"));
    let e = install(
        &manifest_for(&a, "0.2.0"),
        &upd::current_target(),
        &a,
        &exe,
        |_| Err("boom".into()),
    )
    .unwrap_err();
    assert!(e.contains("previous one was restored"), "{e}");
    assert!(std::fs::read_to_string(&exe).unwrap().contains("0.1.0"));
    assert!(!d.join("unv.rollback").exists());
}

#[test]
fn a_failed_rename_in_restores_the_previous_executable() {
    let d = tmp("rename");
    let exe = old_exe(&d);
    let e = replace_executable(&exe, &d.join("does-not-exist"), ok_probe).unwrap_err();
    assert!(e.contains("new executable"), "{e}");
    assert!(std::fs::read_to_string(&exe).unwrap().contains("0.1.0"));
}

#[test]
fn extraction_takes_only_the_exact_name() {
    let a = targz("sub/unv", b"x");
    assert!(extract_binary(&a, "u.tar.gz", "unv").is_err());
    assert!(extract_binary(b"junk", "u.tar.gz", "unv").is_err());
    assert!(extract_binary(b"x", "u.rar", "unv").is_err());
}

#[test]
fn a_missing_platform_is_refused() {
    let d = tmp("plat");
    let exe = old_exe(&d);
    let a = targz("unv", b"x");
    let e = install(&manifest_for(&a, "0.2.0"), "plan9-mips", &a, &exe, ok_probe).unwrap_err();
    assert!(e.contains("no build"), "{e}");
}

fn write_manifest(d: &Path, m: &Manifest) -> PathBuf {
    let p = d.join("m.json");
    std::fs::write(&p, upd::sign(m, &SEED).unwrap()).unwrap();
    p
}

#[test]
fn managed_unknown_and_unkeyed_installs_decline() {
    let d = tmp("origin");
    let exe = old_exe(&d);
    let a = targz("unv", b"x");
    let mf = write_manifest(&d, &manifest_for(&a, "0.2.0"));
    let cmd = UpdateCmd::Install {
        url: upd::DEFAULT_URL.into(),
        manifest: Some(mf.clone()),
        archive: None,
    };
    // No marker beside the executable: unknown origin.
    let e = run_with(&cmd, true, Ok(key()), &exe, "0.1.0").unwrap_err();
    assert!(e.to_string().contains(upd::STANDALONE_MARKER), "{e}");
    // A package-manager path wins even with a marker.
    std::fs::write(d.join(upd::STANDALONE_MARKER), "").unwrap();
    let e = run_with(&cmd, true, Ok(key()), Path::new("/usr/bin/unv"), "0.1.0").unwrap_err();
    assert!(e.to_string().contains("package manager"), "{e}");
    // No key pinned: refused before any manifest is read.
    let e = run_with(&cmd, true, Err("no key".into()), &exe, "0.1.0").unwrap_err();
    assert!(e.to_string().contains("no key"), "{e}");
}

#[test]
fn a_bad_manifest_signature_installs_nothing() {
    let d = tmp("sig");
    let exe = old_exe(&d);
    std::fs::write(d.join(upd::STANDALONE_MARKER), "").unwrap();
    let a = targz("unv", &script("0.2.0"));
    let mf = write_manifest(&d, &manifest_for(&a, "0.2.0"));
    let raw = std::fs::read_to_string(&mf)
        .unwrap()
        .replace("0.2.0", "9.9.9");
    std::fs::write(&mf, raw).unwrap();
    let cmd = UpdateCmd::Install {
        url: upd::DEFAULT_URL.into(),
        manifest: Some(mf),
        archive: None,
    };
    assert!(run_with(&cmd, true, Ok(key()), &exe, "0.1.0").is_err());
    assert!(std::fs::read_to_string(&exe).unwrap().contains("0.1.0"));
}

#[cfg(unix)]
#[test]
fn end_to_end_from_files_with_the_real_version_probe() {
    let d = tmp("e2e");
    let exe = old_exe(&d);
    std::fs::write(d.join(upd::STANDALONE_MARKER), "").unwrap();
    let a = targz("./unv", &script("0.2.0"));
    let ap = d.join("unv.tar.gz");
    std::fs::write(&ap, &a).unwrap();
    let mf = write_manifest(&d, &manifest_for(&a, "0.2.0"));
    let cmd = UpdateCmd::Install {
        url: upd::DEFAULT_URL.into(),
        manifest: Some(mf),
        archive: Some(ap),
    };
    run_with(&cmd, true, Ok(key()), &exe, "0.1.0").unwrap();
    assert!(std::fs::read_to_string(&exe).unwrap().contains("0.2.0"));
    // Same version again is a no-op, never a downgrade or reinstall.
    run_with(&cmd, true, Ok(key()), &exe, "0.2.0").unwrap();
}

#[test]
fn the_publisher_command_signs_what_the_checker_verifies() {
    let d = tmp("sign");
    let a = targz("unv", &script("0.3.0"));
    let ap = d.join("unv-0.3.0-test.tar.gz");
    std::fs::write(&ap, &a).unwrap();
    let kf = d.join("seed.hex");
    std::fs::write(&kf, hex::encode(SEED)).unwrap();
    let mf = d.join("unv-update.json");
    let sign = UpdateCmd::Sign {
        version: "0.3.0".into(),
        notes_file: None,
        assets: vec![format!("{}={}", upd::current_target(), ap.display())],
        base_url: "https://example.test/dl/".into(),
        key_file: kf,
        out: mf.clone(),
    };
    run_with(&sign, true, Err("unused".into()), &d.join("unv"), "0.0.0").unwrap();
    let m = upd::verify(&std::fs::read(&mf).unwrap(), &key()).unwrap();
    assert_eq!(m.version, "0.3.0");
    assert_eq!(
        m.assets[0].url,
        "https://example.test/dl/unv-0.3.0-test.tar.gz"
    );
    assert_eq!(m.assets[0].sha256, upd::sha256_hex(&a));
}
