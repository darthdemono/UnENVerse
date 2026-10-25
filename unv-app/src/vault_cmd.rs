// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! `unv vault status|activate`: which instance of the vault this is, and the explicit step that
//! makes a restored or copied one the live one (see `vault_core::identity`).

use crate::access::Access;
use crate::error::{CliError, CliResult};
use crate::fmt;
use crate::out;
use serde_json::json;
use vault_core::identity::{self, Role};

fn conn(access: &Access) -> CliResult<vault_core::SqlConnection> {
    require_local(access.local_conn()?)
}

/// The identity lives in the local database; a remote vault has none to show or change here.
fn require_local(conn: Option<vault_core::SqlConnection>) -> CliResult<vault_core::SqlConnection> {
    conn.ok_or_else(|| {
        CliError::invalid(
            "Instance identity lives in the local database; this command cannot run in remote mode",
        )
    })
}

pub fn status(access: &Access) -> CliResult {
    let c = conn(access)?;
    let id = identity::ensure(&c).map_err(CliError::from)?;
    let db = crate::access::default_db_path();
    let synced = identity::sync_folder_on_disk(&db);
    out::ok(
        "vault.status",
        json!({ "identity": id, "sync_folder": synced }),
        || {
            println!("vault      {}", id.vault_id);
            println!("instance   {} (epoch {})", id.instance_id, id.epoch);
            println!(
                "role       {}",
                if id.role == Role::Active {
                    "active"
                } else {
                    "standby: restored or copied; it will not push to nodes or run schedules"
                }
            );
            if let Some(name) = synced {
                println!("warning    the vault is inside a {name} folder; two machines opening one synced file can corrupt or fork it");
            }
        },
    );
    Ok(())
}

/// Makes this instance the live one. The person running it says the old instance is retired.
pub fn activate(access: &Access, yes: bool) -> CliResult {
    activate_with(access, &mut |question| fmt::confirm(question, yes))
}

/// [`activate`] with the question put to `ask`, so a "no" is as testable as a "yes".
pub fn activate_with(access: &Access, ask: &mut dyn FnMut(&str) -> CliResult<bool>) -> CliResult {
    let c = conn(access)?;
    let before = identity::ensure(&c).map_err(CliError::from)?;
    let question = format!(
        "Make this the live instance of the vault (epoch {} -> {})? Nodes will refuse any other instance from now on. Is the old one retired?",
        before.epoch,
        before.epoch + 1
    );
    if !ask(&question)? {
        return Err(CliError::invalid("Not activated"));
    }
    let after = if crate::out::dry_run() {
        before.clone()
    } else {
        identity::activate(&c).map_err(CliError::from)?
    };
    out::ok(
        "vault.activate",
        json!({ "before": before, "after": after }),
        || println!("Active at epoch {}.", after.epoch),
    );
    Ok(())
}

/// `unv vault rekey`: change the master password of the local vault. The current password comes
/// from the global `--password` or a prompt; the new one from `UNV_NEW_PASSWORD` or a prompt
/// typed twice (never a flag, which `ps` would show).
pub fn rekey(current: Option<&str>, yes: bool, dry_run: bool) -> CliResult {
    let new_env = std::env::var("UNV_NEW_PASSWORD").ok();
    rekey_with(current, new_env.as_deref(), yes, dry_run)
}

pub fn rekey_with(
    current: Option<&str>,
    new_password: Option<&str>,
    yes: bool,
    dry_run: bool,
) -> CliResult {
    let db = crate::access::default_db_path();
    let salt = crate::access::default_salt_path();
    if !db.exists() {
        return Err(CliError::not_found(format!(
            "No local vault at {}",
            db.display()
        )));
    }
    if dry_run {
        out::ok(
            "vault.rekey",
            json!({ "dry_run": true, "database": db.display().to_string() }),
            || println!("would change the master password of {}", db.display()),
        );
        return Ok(());
    }
    let question = "Change the master password? Backups made earlier still need the old one.";
    if !fmt::confirm(question, yes)? {
        return Err(CliError::new(
            crate::error::Code::NeedsConfirmation,
            "Password change cancelled",
        ));
    }
    let old = crate::access::get_password(current);
    let new = match new_password {
        Some(p) => p.to_string(),
        None => {
            let a = rpassword::prompt_password("New master password: ")
                .map_err(|e| CliError::from(e.to_string()))?;
            let b = rpassword::prompt_password("Repeat: ")
                .map_err(|e| CliError::from(e.to_string()))?;
            if a != b {
                return Err(CliError::invalid("The two passwords differ"));
            }
            a
        }
    };
    let report = vault_core::rekey::rekey(&db, &salt, &old, &new).map_err(CliError::from)?;
    out::ok(
        "vault.rekey",
        json!({ "rows": report.rows, "kept_database": report.kept_database.display().to_string() }),
        || {
            println!(
                "Master password changed ({} rows re-encrypted).",
                report.rows
            );
            println!("The old files are kept until the next successful unlock.");
            println!("Recovery packages are unaffected: they are encrypted to the recovery key, not to this password.");
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_vault_has_no_local_identity_to_show_or_change() {
        let err = require_local(None).err().unwrap();
        assert!(err.message.contains("remote mode"), "{}", err.message);
        let c = vault_core::SqlConnection::open_in_memory().unwrap();
        assert!(require_local(Some(c)).is_ok());
    }
}
