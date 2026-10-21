// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv oauth refresh` — Phase 24.5. Exchanges an `oauth_client` entry's refresh
//! token for a new access token at the issuer's `token_url`.
//!
//! **This is an online act, opt-in and named as such**: it sends the refresh
//! token and client secret to the entry's own `token_url`, the same trade as
//! `enrich --online` (a credential goes only to its issuer). It confirms first.
//!
//! **Order is the whole design.** Some issuers (Slack's `xoxe`, GitHub Apps'
//! `ghr_`) *rotate* the refresh token: the moment the response is sent, the old
//! one is dead. So the new refresh token is written to the vault **before** the
//! access token is shown or used. If that write fails, the new token is dumped
//! to a `0600` recovery file and the command says where, because a crash between
//! "issuer rotated it" and "we stored it" is a permanent lockout.

use crate::access::Access;
use crate::data::{self, entries_mut, find_entry_index};
use crate::error::{CliError, CliResult};
use crate::fmt::confirm;
use crate::out;
use serde_json::{json, Value};
use vault_core::oauth::{apply_grant, parse_grant};

/// Stores a rotated refresh token that the vault could not take, so the user can put it back.
///
/// The file holds a live credential in a directory other users can list, so it is created
/// owner-only from the start (`fmt::write_secret_file`), not written and then restricted:
/// the old `fs::write` plus `chmod` left the token world-readable for the instant between.
fn write_recovery_file(
    dir: &std::path::Path,
    query: &str,
    refresh_token: &str,
) -> Result<std::path::PathBuf, String> {
    let path = dir.join(format!(
        "unv-oauth-recovery-{}.json",
        vault_core::new_uuid()
    ));
    let body = json!({ "entry": query, "refresh_token": refresh_token }).to_string();
    crate::fmt::write_secret_file(&path, &body).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn refresh(access: &Access, query: &str, yes: bool) -> CliResult {
    let mut vault = access.load_vault()?;
    let idx = find_entry_index(&vault, query)?;
    let entry = data::entries(&vault)[idx].clone();
    if data::secret_type_of(&entry) != "oauth_client" {
        return Err(CliError::invalid(format!(
            "'{query}' is not an oauth_client entry"
        )));
    }
    let (token_url, form) =
        vault_core::oauth::refresh_request(&entry).map_err(CliError::invalid)?;
    let host = vault_core::oauth::refresh_host(&token_url);
    if !confirm(
        &format!("Send this entry's refresh token and client secret to {host}?"),
        yes,
    )? {
        println!("Cancelled.");
        return Ok(());
    }

    let resp = crate::tls::build_issuer_client(std::time::Duration::from_secs(20))?
        .post(&token_url)
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .map_err(|e| {
            CliError::unavailable(format!("could not reach {host}: {}", e.without_url()))
        })?;
    let status = resp.status();
    let body: Value = resp.json().unwrap_or(Value::Null);
    if !status.is_success() && body.get("error").is_none() {
        return Err(CliError::denied(format!("{host} answered HTTP {status}")));
    }
    let grant = parse_grant(&body).map_err(CliError::denied)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let rotated = apply_grant(&mut entries_mut(&mut vault)[idx], &grant, now);

    // Store first. Only after the vault holds the new refresh token is the
    // access token reported.
    if let Err(e) = access.save(&vault) {
        if let Some(r) = grant.refresh_token.as_deref().filter(|_| rotated) {
            let wrote = write_recovery_file(&std::env::temp_dir(), query, r);
            let path = wrote.as_ref().ok().cloned().unwrap_or_default();
            return Err(CliError::from(match wrote {
                Ok(_) => format!(
                    "The issuer ROTATED the refresh token but the vault write failed ({e}). \
                     The new token is in {} (0600). Put it back into the entry, then delete the file.",
                    path.display()
                ),
                Err(_) => format!(
                    "The issuer rotated the refresh token and neither the vault nor a recovery file \
                     could be written ({e}). Re-authorise the app at the issuer."
                ),
            }));
        }
        return Err(e);
    }
    out::ok(
        "oauth.refresh",
        json!({
            "entry": query, "rotated_refresh_token": rotated,
            "expires_in": grant.expires_in,
            "access_token": out::masked_json(&grant.access_token),
        }),
        || {
            println!(
                "Refreshed '{query}'{}; access token stored{}",
                if rotated {
                    " (refresh token rotated and stored first)"
                } else {
                    ""
                },
                grant
                    .expires_in
                    .map_or(String::new(), |s| format!(", valid {s}s"))
            )
        },
    );
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::write_recovery_file;

    /// The recovery file held a live refresh token and was written with the default mode
    /// before a chmod, so another user could read it in the shared temp directory in between.
    /// It must exist with owner-only permissions from the first byte.
    #[test]
    fn the_recovery_file_is_owner_only_and_holds_the_token() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("unv-oauth-test-{}", vault_core::new_uuid()));
        vault_core::create_private_dir(&dir).unwrap();
        let path = write_recovery_file(&dir, "Spotify", "rt-new").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let body: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(body["entry"], "Spotify");
        assert_eq!(body["refresh_token"], "rt-new");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
