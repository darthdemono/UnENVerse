// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
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
