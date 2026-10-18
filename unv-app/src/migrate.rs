// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Lossless, local configuration-file migration.
//!
//! Readers never modify their source. They return semantic fields for the vault
//! plus enough byte spans to build a template whose rendering reproduces the
//! original file. Any construct the small reader cannot operate is retained in
//! an encrypted source archive instead of being quietly discarded.

use crate::access::Access;
use crate::error::{CliError, CliResult};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Env,
    Compose,
    WireGuard,
    Nginx,
}

impl SourceKind {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "env" | "dotenv" => Ok(Self::Env),
            "compose" | "docker-compose" => Ok(Self::Compose),
            "wireguard" | "wg" => Ok(Self::WireGuard),
            "nginx" => Ok(Self::Nginx),
            _ => Err("format must be env, compose, wireguard or nginx".into()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Compose => "compose",
            Self::WireGuard => "wireguard",
            Self::Nginx => "nginx",
        }
    }
}

#[derive(Debug, Clone)]
struct SourceField {
    name: String,
    logical_value: String,
    raw_value: String,
    start: usize,
    end: usize,
    source_id: String,
}

#[derive(Debug, Clone)]
struct ParsedSource {
    kind: SourceKind,
    label: String,
    text: String,
    fields: Vec<SourceField>,
    unsupported: Vec<String>,
    has_comments: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CountReport {
    pub read: usize,
    pub imported: usize,
    pub archived: usize,
    pub skipped: usize,
    pub missing_at_source: usize,
    pub conflicts: usize,
}

impl CountReport {
    /// Counts are intentionally disjoint. A conflict is a skipped item and is
    /// reported separately so a human can distinguish "already current" from
    /// "needs a choice" without breaking the conservation equation.
    pub fn adds_up(&self) -> bool {
        self.read == self.imported + self.archived + self.skipped + self.missing_at_source
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewItem {
    pub name: String,
    pub source_id: String,
    pub status: String,
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParserPreview {
    pub format: String,
    pub fields: Vec<PreviewItem>,
    pub unsupported: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub format: String,
    pub source: String,
    pub template: bool,
    pub counts: CountReport,
    pub items: Vec<PreviewItem>,
    pub unsupported: Vec<String>,
}

#[derive(Debug, Clone)]
struct Planned {
    field: SourceField,
    status: String,
    conflict: Option<String>,
    target_entry_id: String,
}

#[derive(Debug, Clone)]
pub struct Plan {
    parsed: ParsedSource,
    planned: Vec<Planned>,
    pub preview: Preview,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn entry_rev(entry: &Value) -> String {
    sha256_hex(serde_json::to_string(entry).unwrap_or_default().as_bytes())
}

fn label_id(label: &str) -> String {
    sha256_hex(label.as_bytes())[..16].to_string()
}

fn source_id(kind: SourceKind, label: &str, name: &str, ordinal: usize) -> String {
    format!("{}:{}:{}:{}", kind.as_str(), label_id(label), name, ordinal)
}

fn trim_span(text: &str, start: usize, end: usize) -> (usize, usize) {
    let before = &text[start..end];
    let leading = before.len() - before.trim_start_matches([' ', '\t']).len();
    let trimmed_end = start + before.trim_end_matches([' ', '\t']).len();
    (start + leading, trimmed_end)
}

fn env_source(label: &str, text: &str) -> ParsedSource {
    let mut fields = Vec::new();
    let mut offset = 0usize;
    let mut ordinal = 0usize;
    let mut comments = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let trimmed = body.trim();
        if trimmed.starts_with('#') {
            comments = true;
        } else if !trimmed.is_empty() {
            let mut logical_start = offset + (body.len() - body.trim_start().len());
            let logical = body.trim_start();
            let export = logical
                .get(..7)
                .is_some_and(|p| p.eq_ignore_ascii_case("export "));
            let logical = if export { &logical[7..] } else { logical };
            if export {
                logical_start += 7;
            }
            if let Some(eq) = logical.find('=') {
                let name = logical[..eq].trim();
                let value_start = logical_start + eq + 1;
                let (start, end) = trim_span(text, value_start, offset + body.len());
                if !name.is_empty() && start <= end {
                    ordinal += 1;
                    let raw = text[start..end].to_string();
                    fields.push(SourceField {
                        name: name.to_string(),
                        logical_value: crate::envfile::unquote_env_value(&raw),
                        raw_value: raw,
                        start,
                        end,
                        source_id: source_id(SourceKind::Env, label, name, ordinal),
                    });
                }
            }
        }
        offset += line.len();
    }
    ParsedSource {
        kind: SourceKind::Env,
        label: label.into(),
        text: text.into(),
        fields,
        unsupported: Vec::new(),
        has_comments: comments,
    }
}

fn key_value_source(kind: SourceKind, label: &str, text: &str) -> ParsedSource {
    let mut fields = Vec::new();
    let mut unsupported = Vec::new();
    let mut offset = 0usize;
    let mut ordinal = 0usize;
    let mut comments = false;
    let mut section = String::new();
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let trimmed = body.trim();
        if trimmed.starts_with('#') {
            comments = true;
            offset += line.len();
            continue;
        }
        if kind == SourceKind::WireGuard && trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed[1..trimmed.len() - 1].to_ascii_lowercase();
            offset += line.len();
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with(';') {
            offset += line.len();
            continue;
        }
        let separator = match kind {
            SourceKind::WireGuard => trimmed.find('='),
            SourceKind::Nginx => trimmed
                .find(';')
                .and_then(|semi| trimmed[..semi].find(char::is_whitespace)),
            _ => None,
        };
        let Some(separator) = separator else {
            if kind == SourceKind::Nginx && (trimmed.ends_with('{') || trimmed == "}") {
                offset += line.len();
                continue;
            }
            unsupported.push(format!(
                "line {}",
                text[..offset].bytes().filter(|b| *b == b'\n').count() + 1
            ));
            offset += line.len();
            continue;
        };
        let name = trimmed[..separator].trim();
        let value = if kind == SourceKind::Nginx {
            trimmed[separator..].trim().trim_end_matches(';').trim()
        } else {
            trimmed[separator + 1..]
                .trim()
                .split(" #")
                .next()
                .unwrap_or("")
                .trim()
        };
        if name.is_empty() || value.is_empty() {
            unsupported.push(format!(
                "line {}",
                text[..offset].bytes().filter(|b| *b == b'\n').count() + 1
            ));
            offset += line.len();
            continue;
        }
        let value_at = body.find(value).unwrap_or(0);
        let start = offset + value_at;
        let end = start + value.len();
        ordinal += 1;
        let display = if section.is_empty() {
            name.into()
        } else {
            format!("{section}.{name}")
        };
        fields.push(SourceField {
            name: display.clone(),
            logical_value: value.into(),
            raw_value: value.into(),
            start,
            end,
            source_id: source_id(kind, label, &display, ordinal),
        });
        offset += line.len();
    }
    ParsedSource {
        kind,
        label: label.into(),
        text: text.into(),
        fields,
        unsupported,
        has_comments: comments,
    }
}

fn compose_source(label: &str, text: &str) -> ParsedSource {
    let mut fields = Vec::new();
    let mut unsupported = Vec::new();
    let mut offset = 0usize;
    let mut service = String::new();
    let mut ordinal = 0usize;
    let mut in_environment = false;
    let mut comments = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let indent = body.len() - body.trim_start().len();
        let trimmed = body.trim();
        if trimmed.starts_with('#') {
            comments = true;
        } else if indent == 2 && trimmed.ends_with(':') {
            service = trimmed
                .trim_end_matches(':')
                .trim_matches(['"', '\''])
                .into();
            in_environment = false;
        } else if indent == 4 && trimmed == "environment:" {
            in_environment = true;
        } else if indent <= 4 {
            in_environment = false;
        }
        if in_environment && indent >= 6 && !service.is_empty() {
            let item = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            let pair = item.find('=').or_else(|| item.find(": "));
            if let Some(eq) = pair {
                let name = item[..eq].trim().trim_matches(['"', '\'']);
                let value = item[eq
                    + if item.as_bytes().get(eq) == Some(&b'=') {
                        1
                    } else {
                        2
                    }..]
                    .trim()
                    .trim_matches(['"', '\'']);
                if !name.is_empty() && !value.is_empty() {
                    ordinal += 1;
                    let value_at = body.find(value).unwrap_or(0);
                    fields.push(SourceField {
                        name: format!("{service}.{name}"),
                        logical_value: value.into(),
                        raw_value: value.into(),
                        start: offset + value_at,
                        end: offset + value_at + value.len(),
                        source_id: source_id(
                            SourceKind::Compose,
                            label,
                            &format!("{service}.{name}"),
                            ordinal,
                        ),
                    });
                } else if !trimmed.is_empty() {
                    unsupported.push(format!(
                        "line {}",
                        text[..offset].bytes().filter(|b| *b == b'\n').count() + 1
                    ));
                }
            } else if !trimmed.is_empty() {
                unsupported.push(format!(
                    "line {}",
                    text[..offset].bytes().filter(|b| *b == b'\n').count() + 1
                ));
            }
        }
        offset += line.len();
    }
    ParsedSource {
        kind: SourceKind::Compose,
        label: label.into(),
        text: text.into(),
        fields,
        unsupported,
        has_comments: comments,
    }
}

fn read(kind: SourceKind, label: &str, text: &str) -> ParsedSource {
    match kind {
        SourceKind::Env => env_source(label, text),
        SourceKind::Compose => compose_source(label, text),
        SourceKind::WireGuard | SourceKind::Nginx => key_value_source(kind, label, text),
    }
}

/// Secret-free parser result for the desktop preview. The renderer receives
/// names and source identities, never values from the pasted configuration.
pub fn parser_preview(kind: SourceKind, label: &str, text: &str) -> ParserPreview {
    let parsed = read(kind, label, text);
    ParserPreview {
        format: kind.as_str().into(),
        fields: parsed
            .fields
            .into_iter()
            .map(|field| PreviewItem {
                name: field.name,
                source_id: field.source_id,
                status: "ready".into(),
                conflict: None,
            })
            .collect(),
        unsupported: parsed.unsupported,
    }
}

fn find_entry<'a>(vault: &'a Value, id: &str) -> Option<&'a Value> {
    vault
        .get("api_keys")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
}

fn reconcile(
    source_hash: &str,
    previous: Option<vault_core::migration::PreviousItem>,
    current_rev: Option<String>,
) -> (String, Option<String>, String) {
    match previous {
        None => ("imported".into(), None, Uuid::new_v4().to_string()),
        Some(previous) if previous.source_hash == source_hash => (
            "skipped".into(),
            None,
            previous
                .target_entry_id
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
        ),
        Some(previous) if current_rev.is_some() && current_rev == previous.target_rev => (
            "updated".into(),
            None,
            previous
                .target_entry_id
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
        ),
        Some(previous) => (
            "conflict".into(),
            Some("source and vault entry changed since the last import".into()),
            previous
                .target_entry_id
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
        ),
    }
}

/// Builds a redacted plan. The only values held by this type stay private; its
/// serializable preview contains names, statuses and counts only.
pub fn plan(
    access: &Access,
    kind: SourceKind,
    label: &str,
    text: &str,
    template: bool,
) -> CliResult<Plan> {
    let parsed = read(kind, label, text);
    let vault = access.load_vault_or_empty()?;
    let conn = access.conn()?;
    let mut planned = Vec::new();
    let mut report = CountReport {
        read: parsed.fields.len() + parsed.unsupported.len(),
        imported: 0,
        archived: parsed.unsupported.len(),
        skipped: 0,
        missing_at_source: 0,
        conflicts: 0,
    };
    for field in &parsed.fields {
        let source_hash = sha256_hex(field.logical_value.as_bytes());
        let previous = vault_core::migration::previous_item(&conn, kind.as_str(), &field.source_id)
            .map_err(CliError::from)?;
        let current_rev = previous
            .as_ref()
            .and_then(|item| item.target_entry_id.as_deref())
            .and_then(|id| find_entry(&vault, id))
            .map(entry_rev);
        let (status, conflict, target_entry_id) = reconcile(&source_hash, previous, current_rev);
        match status.as_str() {
            "imported" | "updated" => report.imported += 1,
            "conflict" => {
                report.skipped += 1;
                report.conflicts += 1;
            }
            _ => report.skipped += 1,
        }
        planned.push(Planned {
            field: field.clone(),
            status,
            conflict,
            target_entry_id,
        });
    }
    let items = planned
        .iter()
        .map(|item| PreviewItem {
            name: item.field.name.clone(),
            source_id: item.field.source_id.clone(),
            status: item.status.clone(),
            conflict: item.conflict.clone(),
        })
        .collect();
    let preview = Preview {
        format: kind.as_str().into(),
        source: label.into(),
        template,
        counts: report,
        items,
        unsupported: parsed.unsupported.clone(),
    };
    debug_assert!(preview.counts.adds_up());
    Ok(Plan {
        parsed,
        planned,
        preview,
    })
}

fn template_bytes(
    parsed: &ParsedSource,
    planned: &[Planned],
) -> Result<(Vec<u8>, BTreeMap<String, Value>), CliError> {
    let mut spans: Vec<(&SourceField, &Planned)> =
        planned.iter().map(|item| (&item.field, item)).collect();
    spans.sort_by_key(|(field, _)| field.start);
    let mut output = String::new();
    let mut bindings = BTreeMap::new();
    let mut cursor = 0usize;
    for (index, (field, item)) in spans.into_iter().enumerate() {
        if field.start < cursor || field.end > parsed.text.len() {
            return Err(CliError::invalid(
                "parser returned overlapping template spans",
            ));
        }
        output.push_str(&parsed.text[cursor..field.start].replace("${", "$${"));
        let reference = format!("UNV_MIG_{}_{}", label_id(&parsed.label), index + 1);
        output.push_str("${");
        output.push_str(&reference);
        output.push('}');
        bindings.insert(
            reference,
            json!({
                "entry_id": item.target_entry_id,
                "render_value": field.raw_value,
            }),
        );
        cursor = field.end;
    }
    output.push_str(&parsed.text[cursor..].replace("${", "$${"));
    Ok((output.into_bytes(), bindings))
}

/// Renders a persisted template. `$${` is a literal `${`; every unescaped
/// placeholder must have a binding, otherwise verification fails closed.
pub fn render_template(
    template: &str,
    bindings: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut output = String::new();
    let mut cursor = 0usize;
    while cursor < template.len() {
        let rest = &template[cursor..];
        if let Some(tail) = rest.strip_prefix("$${") {
            output.push_str("${");
            cursor = template.len() - tail.len();
            continue;
        }
        if let Some(tail) = rest.strip_prefix("${") {
            let Some(end) = tail.find('}') else {
                return Err("unterminated template reference".into());
            };
            let name = &tail[..end];
            let value = bindings
                .get(name)
                .ok_or_else(|| format!("unresolved template reference {name}"))?;
            output.push_str(value);
            cursor = template.len() - tail[end + 1..].len();
            continue;
        }
        let Some(ch) = rest.chars().next() else { break };
        output.push(ch);
        cursor += ch.len_utf8();
    }
    Ok(output)
}

/// Applies a conflict-free plan and stores the document, manifest and encrypted
/// source archive in the same SQLCipher transaction.
pub fn apply(access: &Access, plan: Plan) -> CliResult<Preview> {
    if plan.preview.counts.conflicts > 0 {
        return Err(CliError::conflict(format!(
            "{} migration item(s) conflict; nothing was written",
            plan.preview.counts.conflicts
        )));
    }
    if crate::out::dry_run() {
        return Ok(plan.preview);
    }
    // A repeated source with the same item hashes changes neither the vault nor
    // its migration ledger. Recording another manifest would make the command
    // look idempotent while still growing encrypted state on every invocation.
    if !plan
        .planned
        .iter()
        .any(|item| item.status == "imported" || item.status == "updated")
    {
        return Ok(plan.preview);
    }
    let mut vault = access.load_vault_or_empty()?;
    let entries = vault
        .get_mut("api_keys")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| CliError::invalid("vault has no api_keys list"))?;
    let migration_id = format!("mig_{}", Uuid::new_v4().simple());
    let now = vault_core::iso_now();
    let mut items = Vec::new();
    for item in &plan.planned {
        let mut entry = match entries.iter().position(|entry| {
            entry.get("id").and_then(Value::as_str) == Some(item.target_entry_id.as_str())
        }) {
            Some(index) => entries[index].clone(),
            None => json!({
                "id": item.target_entry_id,
                "provider": item.field.name,
                "secretType": "env_var",
                "projectIds": ["Universal"],
                "categories": [],
                "scopes": [],
                "price_type": "local",
                "created_at": now,
            }),
        };
        if item.status == "imported" || item.status == "updated" {
            entry["api_key"] = json!(item.field.logical_value);
            entry["provider"] = json!(item.field.name);
            entry["authority"] = json!({
                "owner": "source", "source_kind": plan.parsed.kind.as_str(),
                "source_id": item.field.source_id, "migration_id": migration_id, "since": now,
            });
            entry["provenance"] = json!({
                "kind": "import", "source_kind": plan.parsed.kind.as_str(),
                "migration_id": migration_id, "imported_at": now, "precision": "exact",
            });
            match entries.iter().position(|old| {
                old.get("id").and_then(Value::as_str) == Some(item.target_entry_id.as_str())
            }) {
                Some(index) => entries[index] = entry.clone(),
                None => entries.push(entry.clone()),
            }
        }
        items.push(vault_core::migration::MigrationItem {
            source_kind: plan.parsed.kind.as_str().into(),
            source_id: item.field.source_id.clone(),
            source_hash: sha256_hex(item.field.logical_value.as_bytes()),
            target_entry_id: Some(item.target_entry_id.clone()),
            target_rev: Some(entry_rev(&entry)),
            status: item.status.clone(),
            conflict: item.conflict.clone(),
        });
    }
    let mut archives = Vec::new();
    let mut manifest = json!({
        "counts": plan.preview.counts,
        "template": plan.preview.template,
        "source_sha256": sha256_hex(plan.parsed.text.as_bytes()),
    });
    if plan.preview.template {
        let (bytes, bindings) = template_bytes(&plan.parsed, &plan.planned)?;
        let archive_id = format!("arc_{}", Uuid::new_v4().simple());
        manifest["template_archive_id"] = json!(archive_id);
        manifest["template_bindings"] =
            serde_json::to_value(bindings).map_err(|e| CliError::from(e.to_string()))?;
        archives.push(vault_core::migration::SourceArchive {
            id: archive_id,
            source_kind: plan.parsed.kind.as_str().into(),
            source_id: None,
            reason: "lossless migration template".into(),
            media_type: "text/plain".into(),
            bytes,
        });
    } else {
        // The source is the recovery record for constructs a small reader does
        // not understand today. Keeping every non-template source also means a
        // later, richer reader can make an explicit migration decision rather
        // than relying on an external path that may no longer exist.
        let reason = if plan.parsed.has_comments || !plan.parsed.unsupported.is_empty() {
            "comments or unsupported constructs preserved, not operational"
        } else {
            "original source retained for provenance and future parser compatibility"
        };
        archives.push(vault_core::migration::SourceArchive {
            id: format!("arc_{}", Uuid::new_v4().simple()),
            source_kind: plan.parsed.kind.as_str().into(),
            source_id: None,
            reason: reason.into(),
            media_type: "text/plain".into(),
            bytes: plan.parsed.text.as_bytes().to_vec(),
        });
    }
    let record = vault_core::migration::MigrationRecord {
        id: migration_id,
        source_kind: plan.parsed.kind.as_str().into(),
        source_label: plan.parsed.label,
        created_at: now,
        status: "applied".into(),
        manifest,
        items,
        archives,
    };
    access.save_migration(&vault, &record)?;
    Ok(plan.preview)
}

pub fn plan_file(
    access: &Access,
    kind: SourceKind,
    file: &Path,
    template: bool,
) -> CliResult<Plan> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| CliError::not_found(format!("Cannot read {}: {e}", file.display())))?;
    plan(access, kind, &file.to_string_lossy(), &text, template)
}

pub fn archives(access: &Access) -> CliResult<Vec<vault_core::migration::ArchiveInfo>> {
    let conn = access.conn()?;
    vault_core::migration::archives(&conn).map_err(CliError::from)
}

pub fn reports(access: &Access) -> CliResult<Vec<vault_core::migration::MigrationInfo>> {
    let conn = access.conn()?;
    vault_core::migration::migrations(&conn).map_err(CliError::from)
}

/// Verifies a saved lossless template without exposing source content. It uses
/// the encrypted raw representation captured at import time, then compares only
/// full SHA-256 digests in the result.
pub fn verify(access: &Access, migration_id: &str) -> CliResult<Value> {
    let conn = access.conn()?;
    let record = vault_core::migration::migration(&conn, migration_id)
        .map_err(CliError::from)?
        .ok_or_else(|| CliError::not_found(format!("No migration {migration_id}")))?;
    let archive_id = record
        .manifest
        .get("template_archive_id")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::invalid("migration was not imported in template mode"))?;
    let template = vault_core::migration::archive_bytes(&conn, archive_id)
        .map_err(CliError::from)?
        .ok_or_else(|| CliError::not_found(format!("No template archive {archive_id}")))?;
    let values = record
        .manifest
        .get("template_bindings")
        .and_then(Value::as_object)
        .ok_or_else(|| CliError::invalid("migration template has no bindings"))?
        .iter()
        .filter_map(|(name, binding)| {
            binding
                .get("render_value")
                .and_then(Value::as_str)
                .map(|value| (name.clone(), value.to_string()))
        })
        .collect::<BTreeMap<_, _>>();
    let rendered = render_template(
        std::str::from_utf8(&template).map_err(|e| CliError::invalid(e.to_string()))?,
        &values,
    )
    .map_err(CliError::invalid)?;
    let actual = sha256_hex(rendered.as_bytes());
    let expected = record
        .manifest
        .get("source_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(json!({"migration_id": migration_id, "verified": actual == expected, "sha256": actual}))
}

pub fn export_archive(access: &Access, id: &str, out: &Path) -> CliResult {
    let conn = access.conn()?;
    let bytes = vault_core::migration::archive_bytes(&conn, id)
        .map_err(CliError::from)?
        .ok_or_else(|| CliError::not_found(format!("No migration archive {id}")))?;
    std::fs::write(out, &bytes).map_err(|e| CliError::from(e.to_string()))?;
    vault_core::restrict_to_owner(out).map_err(CliError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_mode_preserves_bom_crlf_tabs_trailing_space_and_literal_dollar() {
        let original = "\u{feff}# comment\r\nexport TOKEN=\"value\"  \r\nTABS=\tthree\t\r\nLITERAL=$${HOME}\r\n";
        let parsed = env_source("fixture.env", original);
        let planned = parsed
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| Planned {
                field: field.clone(),
                status: "imported".into(),
                conflict: None,
                target_entry_id: format!("entry-{index}"),
            })
            .collect::<Vec<_>>();
        let (bytes, raw) = template_bytes(&parsed, &planned).unwrap();
        let bindings = raw
            .into_iter()
            .map(|(name, value)| (name, value["render_value"].as_str().unwrap().to_string()))
            .collect::<BTreeMap<_, _>>();
        let rendered = render_template(std::str::from_utf8(&bytes).unwrap(), &bindings).unwrap();
        assert_eq!(
            sha256_hex(rendered.as_bytes()),
            sha256_hex(original.as_bytes())
        );
    }

    #[test]
    fn same_name_in_two_sources_has_distinct_stable_ids() {
        let one = env_source("one.env", "TOKEN=one\n");
        let two = env_source("two.env", "TOKEN=two\n");
        assert_ne!(one.fields[0].source_id, two.fields[0].source_id);
    }

    #[test]
    fn count_report_always_adds_up_with_conflicts_counted_as_skipped() {
        let report = CountReport {
            read: 4,
            imported: 1,
            archived: 1,
            skipped: 2,
            missing_at_source: 0,
            conflicts: 1,
        };
        assert!(report.adds_up());
    }

    #[test]
    fn repeat_import_is_a_noop_then_updates_or_conflicts_by_three_way_comparison() {
        let previous = vault_core::migration::PreviousItem {
            source_hash: "old-source".into(),
            target_entry_id: Some("entry".into()),
            target_rev: Some("old-target".into()),
        };
        assert_eq!(
            reconcile(
                "old-source",
                Some(previous.clone()),
                Some("changed-target".into())
            )
            .0,
            "skipped"
        );
        assert_eq!(
            reconcile(
                "new-source",
                Some(previous.clone()),
                Some("old-target".into())
            )
            .0,
            "updated"
        );
        let conflict = reconcile("new-source", Some(previous), Some("changed-target".into()));
        assert_eq!(conflict.0, "conflict");
        assert!(conflict.1.unwrap().contains("source and vault"));
    }

    #[test]
    fn readers_cover_env_compose_wireguard_and_nginx_without_values_in_preview() {
        for (kind, text, expected) in [
            (SourceKind::Env, "TOKEN=value\n", "TOKEN"),
            (
                SourceKind::Compose,
                "services:\n  app:\n    environment:\n      TOKEN: value\n",
                "app.TOKEN",
            ),
            (
                SourceKind::WireGuard,
                "[Interface]\nPrivateKey = value\n",
                "interface.PrivateKey",
            ),
            (
                SourceKind::Nginx,
                "server {\nlisten 443 ssl;\n}\n",
                "listen",
            ),
        ] {
            let preview = parser_preview(kind, "fixture", text);
            assert!(preview.fields.iter().any(|field| field.name == expected));
            assert!(serde_json::to_string(&preview).unwrap().contains(expected));
            assert!(!serde_json::to_string(&preview).unwrap().contains("value"));
        }
    }

    #[test]
    fn rust_readers_share_the_sanitised_parity_fixture_with_typescript() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/parity/config-readers.json"
        ))
        .unwrap();
        for (format, expected) in fixture.as_object().unwrap() {
            let kind = SourceKind::parse(format).unwrap();
            let text = expected["text"].as_str().unwrap();
            let preview = parser_preview(kind, "fixture", text);
            let names = preview
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>();
            for name in expected["rust_fields"].as_array().unwrap() {
                assert!(
                    names.contains(&name.as_str().unwrap()),
                    "{format} must preserve {name}"
                );
            }
        }
    }
}
