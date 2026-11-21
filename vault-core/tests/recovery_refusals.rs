// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! R39 slice 1, `vault-core/src/recovery.rs`: what a recovery package and its keys refuse.
//!
//! A recovery package is read back when something has already gone wrong, so every way a file can
//! fail to be one must end in a plain error, and a key that is not an age key must never be stored
//! as if it were one.

use age::secrecy::ExposeSecret;
use vault_core::recovery::{
    drill_identity, generate_identity, inspect_package, recipient, set_drill_identity,
    set_recipient,
};
use vault_core::SqlConnection as Conn;

fn db() -> Conn {
    let c = Conn::open_in_memory().unwrap();
    vault_core::init_schema(&c).unwrap();
    c
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "unv-recovery-refusals-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_recipient_and_a_drill_identity_are_stored_only_when_they_are_age_keys() {
    let c = db();
    assert_eq!(recipient(&c).unwrap(), None);
    assert_eq!(drill_identity(&c).unwrap(), None);

    let e = set_recipient(&c, "not-a-key").unwrap_err();
    assert!(e.contains("not a valid age X25519 recipient"), "{e}");
    let e = set_drill_identity(&c, "AGE-SECRET-KEY-nonsense").unwrap_err();
    assert!(e.contains("not a valid age X25519 identity"), "{e}");
    // The refused values were not stored.
    assert_eq!(recipient(&c).unwrap(), None);
    assert_eq!(drill_identity(&c).unwrap(), None);

    let (public, secret) = generate_identity();
    set_recipient(&c, &public).unwrap();
    // Padding around a pasted identity is trimmed for the check; the value is stored as given.
    set_drill_identity(&c, &format!("  {secret}\n")).unwrap();
    assert_eq!(recipient(&c).unwrap().as_deref(), Some(public.as_str()));
    assert!(drill_identity(&c).unwrap().unwrap().contains(secret.trim()));
    // The two generated halves belong together.
    let id: age::x25519::Identity = secret.parse().unwrap();
    assert_eq!(id.to_public().to_string(), public);
    assert!(!id.to_string().expose_secret().is_empty());
}

#[test]
fn a_file_that_is_not_a_package_is_refused_by_name() {
    let d = scratch("header");
    let write = |name: &str, bytes: &[u8]| {
        let p = d.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    };
    let e = inspect_package(&d.join("missing.unvpkg")).unwrap_err();
    assert!(e.contains("open package"), "{e}");
    // Too short to hold even the magic.
    assert!(inspect_package(&write("short", b"UNV")).is_err());
    // Long enough, wrong magic: someone's photo or an old backup.
    let e = inspect_package(&write("magic", b"NOTAPKG!\0\0\0\0")).unwrap_err();
    assert!(e.contains("Not an UnENVerse recovery package"), "{e}");
    // A header length of zero, and one far beyond what a manifest needs.
    let mut zero = b"UNVPKG1\n".to_vec();
    zero.extend_from_slice(&0u32.to_le_bytes());
    let e = inspect_package(&write("zero", &zero)).unwrap_err();
    assert!(e.contains("header is invalid"), "{e}");
    let mut huge = b"UNVPKG1\n".to_vec();
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    let e = inspect_package(&write("huge", &huge)).unwrap_err();
    assert!(e.contains("header is invalid"), "{e}");
    // A header that promises more bytes than the file has.
    let mut cut = b"UNVPKG1\n".to_vec();
    cut.extend_from_slice(&50u32.to_le_bytes());
    cut.extend_from_slice(b"{}");
    let e = inspect_package(&write("cut", &cut)).unwrap_err();
    assert!(e.contains("read package header"), "{e}");
    // A full-length header that is not a manifest.
    let mut junk = b"UNVPKG1\n".to_vec();
    junk.extend_from_slice(&5u32.to_le_bytes());
    junk.extend_from_slice(b"hello");
    let e = inspect_package(&write("junk", &junk)).unwrap_err();
    assert!(e.contains("parse package manifest"), "{e}");
}
