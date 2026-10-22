// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `backup restore-archive --force`: answering "no" must leave the vault alone. The answer used to
//! be discarded, so "n" overwrote it anyway.

use unv_app::backup::{build_archive, restore_archive_with};

fn pw() -> String {
    format!("archive-{}", vault_core::new_uuid())
}

#[test]
fn a_no_keeps_the_existing_vault_and_a_yes_replaces_it() {
    let password = pw();
    let dir = std::env::temp_dir().join(format!("unv-restore-confirm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("vault.db");
    unv_app::access::set_paths(Some(db.clone()), None);

    let (text, _) = build_archive(
        b"database from the archive",
        &vault_core::new_uuid().as_bytes()[..16],
        &password,
    )
    .unwrap();
    let archive = dir.join("a.vaultarc");
    std::fs::write(&archive, text).unwrap();
    std::fs::write(&db, b"the live vault").unwrap();

    let no = restore_archive_with(&archive, Some(&password), true, &mut |_| Ok(false));
    assert!(no.is_err(), "a refusal is an error, not a success");
    assert_eq!(std::fs::read(&db).unwrap(), b"the live vault");

    restore_archive_with(&archive, Some(&password), true, &mut |_| Ok(true)).unwrap();
    assert_eq!(std::fs::read(&db).unwrap(), b"database from the archive");
    let _ = std::fs::remove_dir_all(&dir);
}
