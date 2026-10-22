// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv rotate plan|run|status`: drives `vault_core::rotation` against an issuer connector and the
//! vault entry. The engine owns the safety rules; this module only supplies the entry store and
//! the connector.
//!
//! An entry is rotated in place: its secret field takes the new value as the next generation (the
//! old value stays in the entry's version history) and `rotation.token_id` follows the credential.
//! With no consumers named the old credential is left valid (`revocation_pending`) unless
//! `--revoke-now` says nothing else uses it.

use crate::access::Access;
use crate::data;
use crate::error::{CliError, CliResult};
use crate::issuer_cloudflare::Cloudflare;
use crate::out;
use serde_json::{json, Value};
use vault_core::rotation::{
    self, Class, Consumers, Credential, Engine, EntryStore, IssuerConnector, Outcome, Policy,
    RevokeRule, RollbackRule, Run, RunStore, SqlStore, Status,
};

pub const CLOUDFLARE_BASE: &str = "https://api.cloudflare.com/client/v4";

pub struct Opts<'a> {
    pub entry: &'a str,
    pub issuer: &'a str,
    pub field: &'a str,
    pub token_id: Option<&'a str>,
    pub base_url: Option<&'a str>,
    pub allow_roll: bool,
    pub revoke_now: bool,
    pub resume: Option<&'a str>,
}

struct VaultEntry<'a> {
    access: &'a Access,
    query: String,
    field: String,
    token_id: String,
    previous: Option<Credential>,
}

impl VaultEntry<'_> {
    fn edit(&self, change: impl FnOnce(&mut Value)) -> Result<(), String> {
        let mut vault = self.access.load_vault().map_err(|e| e.message)?;
        let idx = data::find_entry_index(&vault, &self.query).map_err(|e| e.message)?;
        let entry = &mut data::entries_mut(&mut vault)[idx];
        change(entry);
        self.access.save(&vault).map_err(|e| e.message)
    }
}

impl EntryStore for VaultEntry<'_> {
    fn current(&self) -> Credential {
        let value = self
            .access
            .load_vault()
            .ok()
            .and_then(|v| {
                let idx = data::find_entry_index(&v, &self.query).ok()?;
                data::entries(&v)[idx]
                    .get(&self.field)?
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_default();
        Credential {
            id: self.token_id.clone(),
            value,
        }
    }

    fn commit(&mut self, new: &Credential, _generation: u64) -> Result<(), String> {
        self.previous = Some(self.current());
        let (field, now) = (self.field.clone(), vault_core::iso_now());
        let (value, id) = (new.value.clone(), new.id.clone());
        let token = id.clone();
        self.edit(move |e| {
            e[&field] = json!(value);
            e["rotation"] = json!({ "token_id": token });
            e["last_rotated_at"] = json!(now);
            if let Some(o) = e.as_object_mut() {
                o.remove("compromised");
            }
        })?;
        self.token_id = id;
        Ok(())
    }

    fn restore(&mut self, _old: &Credential) -> Result<(), String> {
        let prev = self
            .previous
            .clone()
            .ok_or("the previous value is not available in this process")?;
        let field = self.field.clone();
        self.edit(move |e| {
            e[&field] = json!(prev.value);
            e["rotation"] = json!({ "token_id": prev.id });
        })
    }
}

/// Consumers are not wired to node targets yet: nothing is deployed, so nothing can be checked.
struct NoConsumers;

impl Consumers for NoConsumers {
    fn required(&self) -> Vec<String> {
        Vec::new()
    }
    fn deploy(&mut self, _: &str, _: &Credential, _: u64) -> Outcome {
        Outcome::Ok
    }
    fn check(&mut self, _: &str) -> Outcome {
        Outcome::Ok
    }
    fn redeploy_previous(&mut self, _: &str) -> Outcome {
        Outcome::Ok
    }
}

fn connector(o: &Opts<'_>, manager: Credential) -> CliResult<Box<dyn IssuerConnector>> {
    match o.issuer {
        "cloudflare" => Ok(Box::new(
            Cloudflare::new(o.base_url.unwrap_or(CLOUDFLARE_BASE), manager, o.allow_roll)
                .map_err(CliError::invalid)?,
        )),
        other => Err(CliError::invalid(format!(
            "Unknown issuer '{other}' (available: cloudflare)"
        ))),
    }
}

fn class_of(o: &Opts<'_>) -> Class {
    if o.allow_roll {
        Class::RollProviderGenerated
    } else {
        Class::CreateOverlapping
    }
}

fn token_id(vault: &Value, idx: usize, o: &Opts<'_>) -> CliResult<String> {
    o.token_id
        .map(str::to_string)
        .or_else(|| {
            data::entries(vault)[idx]["rotation"]["token_id"]
                .as_str()
                .map(str::to_string)
        })
        .ok_or_else(|| {
            CliError::invalid(
                "The credential's id at the issuer is unknown: pass --token-id once (it is remembered)",
            )
        })
}

fn describe(run: &Run) -> Value {
    json!({
        "run": run.id,
        "entry": run.entry,
        "class": run.class,
        "step": run.step,
        "status": run.status,
        "generation": run.generation,
        "credential_name": run.name,
        "consumers": run.consumers,
        "revoked": run.revoked,
    })
}

pub fn plan(access: &Access, o: &Opts<'_>) -> CliResult {
    let vault = access.load_vault()?;
    let idx = data::find_entry_index(&vault, o.entry)?;
    let id = token_id(&vault, idx, o)?;
    let class = class_of(o);
    let provider = data::provider_of(&data::entries(&vault)[idx]).to_string();
    let mut steps = vec![
        "issue a new credential at the issuer (the old one stays valid)",
        "verify the new credential works",
        "store it in the entry as the next generation",
        "deploy to each consumer, then check each",
    ];
    steps.push(if o.revoke_now {
        "revoke the old credential"
    } else {
        "leave the old credential valid (pass --revoke-now once nothing else uses it)"
    });
    if class == Class::RollProviderGenerated {
        steps[0] = "roll the credential in place (the old value stops working at once; not retried if the answer is lost)";
    }
    out::ok(
        "rotate.plan",
        json!({ "entry": provider, "issuer": o.issuer, "token_id": id, "class": class, "steps": steps }),
        || {
            println!("Rotating '{provider}' at {} ({class:?}):", o.issuer);
            for (n, s) in steps.iter().enumerate() {
                println!("  {}. {s}", n + 1);
            }
        },
    );
    Ok(())
}

pub fn run(access: &Access, o: &Opts<'_>) -> CliResult {
    let conn = access.conn()?;
    rotation::init_schema(&conn).map_err(CliError::from)?;
    let vault = access.load_vault()?;
    let idx = data::find_entry_index(&vault, o.entry)?;
    let entry = &data::entries(&vault)[idx];
    let id = token_id(&vault, idx, o)?;
    let provider = data::provider_of(entry).to_string();
    let value = entry
        .get(o.field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if value.is_empty() {
        return Err(CliError::invalid(format!(
            "'{provider}' has no value in {}",
            o.field
        )));
    }
    let generation = entry["version_history"]
        .as_array()
        .map_or(1, |h| h.len() as u64 + 2);
    let mut run = match o.resume {
        Some(rid) => rotation::load_run(&conn, rid)
            .map_err(CliError::from)?
            .ok_or_else(|| CliError::not_found(format!("No rotation run {rid}")))?,
        None => Run::plan(
            &vault_core::new_uuid()[..8],
            &provider,
            generation,
            class_of(o),
            Policy {
                revoke: if o.revoke_now {
                    RevokeRule::AfterAllChecked
                } else {
                    RevokeRule::Never
                },
                rollback: RollbackRule::RestoreOnFailure,
            },
            &id,
        ),
    };
    if run.is_finished() {
        return Err(CliError::conflict(format!(
            "Run {} already finished ({:?})",
            run.id, run.status
        )));
    }
    let manager = Credential {
        id: id.clone(),
        value,
    };
    let mut issuer = connector(o, manager)?;
    let mut store_entry = VaultEntry {
        access,
        query: o.entry.to_string(),
        field: o.field.to_string(),
        token_id: id,
        previous: None,
    };
    let mut store = SqlStore { conn: &conn };
    let gen = || String::new();
    Engine {
        issuer: issuer.as_mut(),
        consumers: &mut NoConsumers,
        entry: &mut store_entry,
        store: &mut store,
        generate: &gen,
    }
    .run(&mut run)
    .map_err(CliError::from)?;
    let _ = store.save(&run);
    let summary = describe(&run);
    out::ok("rotate.run", summary, || {
        println!("Run {}: {:?}", run.id, run.status);
        println!("  step {:?}, revoked: {}", run.step, run.revoked);
    });
    match run.status {
        Status::Succeeded | Status::RevocationPending(_) => Ok(()),
        Status::RolledBack(m) | Status::Failed(m) | Status::NeedsAttention(m) => {
            Err(CliError::conflict(m))
        }
        Status::Running => Ok(()),
    }
}

pub fn status(access: &Access, run: Option<&str>) -> CliResult {
    let conn = access.conn()?;
    rotation::init_schema(&conn).map_err(CliError::from)?;
    let runs = match run {
        Some(id) => vec![rotation::load_run(&conn, id)
            .map_err(CliError::from)?
            .ok_or_else(|| CliError::not_found(format!("No rotation run {id}")))?],
        None => rotation::list_runs(&conn).map_err(CliError::from)?,
    };
    out::ok(
        "rotate.status",
        json!({ "runs": runs.iter().map(describe).collect::<Vec<_>>() }),
        || {
            if runs.is_empty() {
                println!("No rotation runs.");
            }
            for r in &runs {
                println!("{}  {}  {:?}  {:?}", r.id, r.entry, r.step, r.status);
            }
        },
    );
    Ok(())
}
