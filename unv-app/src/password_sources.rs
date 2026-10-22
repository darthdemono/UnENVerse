// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Full-fidelity, local password-manager source migration.
//!
//! Unlike the older quick importer, this module keeps source identities and
//! every attachment or unsupported record in R08's encrypted migration store.
//! The readers never modify a source file.  They are deliberately CLI-only for
//! now: the desktop quick-import path cannot atomically save a vault document,
//! source manifest, and binary archives.

use crate::access::Access;
use crate::error::{CliError, CliResult};
use keepass::db::fields as kp_fields;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;
use uuid::Uuid;

const MAX_ZIP_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ZIP_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

type ZipEntry = (String, Vec<u8>);
type ZipEntries = (Vec<u8>, Vec<ZipEntry>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Bitwarden,
    OnePassword,
    KeePass,
    /// Git-based stores, read through the user's own tools (`sops`, `gpg`, `gopass`).
    Sops,
    Pass,
    Gopass,
    /// Secrets services, read over their APIs (or `bws`) with the user's own credential.
    OpenBao,
    Infisical,
    Bws,
    AwsSm,
    AwsSsm,
}

impl SourceFormat {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "bitwarden" => Ok(Self::Bitwarden),
            "onepassword" => Ok(Self::OnePassword),
            "keepass" | "keepassxc" | "kdbx" => Ok(Self::KeePass),
            "sops" => Ok(Self::Sops),
            "pass" | "password-store" => Ok(Self::Pass),
            "gopass" => Ok(Self::Gopass),
            "openbao" | "vault" => Ok(Self::OpenBao),
            "infisical" => Ok(Self::Infisical),
            "bws" => Ok(Self::Bws),
            "aws-sm" => Ok(Self::AwsSm),
            "aws-ssm" => Ok(Self::AwsSsm),
            _ => Err("source must be bitwarden, onepassword, keepass, sops, pass or gopass".into()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Bitwarden => "bitwarden",
            Self::OnePassword => "onepassword",
            Self::KeePass => "keepass",
            Self::Sops => "sops",
            Self::Pass => "pass",
            Self::Gopass => "gopass",
            Self::OpenBao => "openbao",
            Self::Infisical => "infisical",
            Self::Bws => "bws",
            Self::AwsSm => "aws-sm",
            Self::AwsSsm => "aws-ssm",
        }
    }
}

/// Options intentionally exclude a password argument.  Supplying a database
/// password on a command line leaks it to shell history and process listings.
pub struct SourceOpts<'a> {
    pub project: Option<&'a str>,
    pub category: Option<&'a str>,
    pub keep_folders: bool,
    pub key_file: Option<&'a Path>,
    pub no_password: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Archive {
    pub(crate) source_id: Option<String>,
    pub(crate) reason: String,
    pub(crate) media_type: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct Item {
    pub(crate) source_id: String,
    pub(crate) source_hash: String,
    /// The complete source record is held only while planning so a change in
    /// an unsupported source field still reaches reconciliation.
    pub(crate) source: Value,
    pub(crate) name: String,
    pub(crate) entry: Value,
    pub(crate) archives: Vec<Archive>,
    pub(crate) reports: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ArchivedItem {
    pub(crate) source_id: String,
    pub(crate) source_hash: String,
    pub(crate) name: String,
    pub(crate) archive: Archive,
}

#[derive(Debug)]
pub(crate) struct Parsed {
    pub(crate) format: SourceFormat,
    pub(crate) label: String,
    pub(crate) source_bytes: Vec<u8>,
    pub(crate) items: Vec<Item>,
    pub(crate) archived: Vec<ArchivedItem>,
    pub(crate) warnings: Vec<String>,
    pub(crate) missing_at_source: Vec<String>,
}

#[derive(Debug)]
struct Planned {
    item: Item,
    status: String,
    conflict: Option<String>,
    target_entry_id: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Counts {
    pub read: usize,
    pub imported: usize,
    pub archived: usize,
    pub skipped: usize,
    pub missing_at_source: usize,
    pub conflicts: usize,
}

impl Counts {
    fn adds_up(&self) -> bool {
        self.read == self.imported + self.archived + self.skipped + self.missing_at_source
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PreviewItem {
    pub name: String,
    pub source_id: String,
    pub status: String,
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Preview {
    pub format: String,
    pub source: String,
    pub counts: Counts,
    pub items: Vec<PreviewItem>,
    pub warnings: Vec<String>,
    pub missing_at_source: Vec<String>,
}

struct Plan {
    parsed: Parsed,
    planned: Vec<Planned>,
    archived: Vec<(ArchivedItem, String)>,
    preview: Preview,
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

pub(crate) fn source_hash(value: &Value) -> String {
    hash(&serde_json::to_vec(value).unwrap_or_default())
}

pub(crate) fn item_source_hash(source: &Value, archives: &[Archive]) -> String {
    let mut attachments: Vec<_> = archives
        .iter()
        .map(|archive| {
            serde_json::to_string(&json!({
                "source_id": archive.source_id,
                "reason": archive.reason,
                "media_type": archive.media_type,
                "sha256": hash(&archive.bytes),
            }))
            .unwrap_or_default()
        })
        .collect();
    attachments.sort();
    source_hash(&json!({ "source": source, "attachments": attachments }))
}

fn refresh_item_hash(item: &mut Item) {
    item.source_hash = item_source_hash(&item.source, &item.archives);
}

pub(crate) fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.trim().is_empty() => Some(value.trim().to_owned()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(crate) fn extra(key: impl Into<String>, value: impl Into<String>, public: bool) -> Value {
    json!({ "key": key.into(), "value": value.into(), "public": public })
}

pub(crate) fn base_entry(name: String, secret: String, secret_type: &str) -> Value {
    json!({
        "provider": name,
        "api_key": secret,
        "secretType": secret_type,
        "price_type": "local",
        "categories": [],
        "projectIds": ["Universal"],
        "scopes": [],
    })
}

pub(crate) fn classify(secret: &str) -> &'static str {
    let secret = secret.trim();
    if secret.starts_with("-----BEGIN") {
        return if secret.contains("CERTIFICATE") {
            "certificate"
        } else {
            "ssh_key"
        };
    }
    if [
        "postgres://",
        "postgresql://",
        "mysql://",
        "mongodb://",
        "redis://",
    ]
    .iter()
    .any(|prefix| secret.starts_with(prefix))
    {
        return "connection_string";
    }
    if [
        "ghp_",
        "gho_",
        "github_pat_",
        "glpat-",
        "xoxb-",
        "xoxp-",
        "sk_live_",
        "sk_test_",
        "sk-ant-",
        "sk-",
        "dop_v1_",
        "npm_",
        "AKIA",
    ]
    .iter()
    .any(|prefix| secret.starts_with(prefix))
    {
        return "api_key";
    }
    "password"
}

fn apply_item_metadata(
    entry: &mut Value,
    categories: Vec<String>,
    tags: Vec<String>,
    extra_vars: Vec<Value>,
) {
    if !categories.is_empty() {
        entry["categories"] = json!(categories);
    }
    if !tags.is_empty() {
        entry["tags"] = json!(tags);
    }
    if !extra_vars.is_empty() {
        entry["extra_vars"] = json!(extra_vars);
    }
}

pub(crate) fn add_totp(entry: &mut Value, totp: Option<String>, reports: &mut Vec<String>) {
    let Some(totp) = totp else { return };
    match vault_core::totp::parse_seed(&totp) {
        Ok(stored) => vault_core::totp_import::write_fields(entry, &stored),
        Err(_) => {
            reports.push("TOTP value retained in the encrypted source archive because it was not a usable seed".into());
        }
    }
}

pub(crate) fn source_item(
    source_id: String,
    name: String,
    source: Value,
    mut entry: Value,
    categories: Vec<String>,
    tags: Vec<String>,
    extra_vars: Vec<Value>,
    totp: Option<String>,
    history: Vec<Value>,
    archives: Vec<Archive>,
    mut reports: Vec<String>,
) -> Item {
    apply_item_metadata(&mut entry, categories, tags, extra_vars);
    add_totp(&mut entry, totp, &mut reports);
    if !history.is_empty() {
        entry["version_history"] = json!(history);
    }
    Item {
        source_id,
        source_hash: item_source_hash(&source, &archives),
        source,
        name,
        entry,
        archives,
        reports,
    }
}

fn ref_reports(value: &str, field: &str, reports: &mut Vec<String>) {
    if value.contains("{REF:") {
        reports.push(format!(
            "KeePass field '{field}' contains a {{REF:…}} reference and was not resolved"
        ));
    }
}

fn media_type(name: &str) -> &'static str {
    let name = name.to_ascii_lowercase();
    if name.ends_with(".json") {
        "application/json"
    } else if name.ends_with(".txt") {
        "text/plain"
    } else if name.ends_with(".pdf") {
        "application/pdf"
    } else if name.ends_with(".png") {
        "image/png"
    } else if name.ends_with(".jpg") || name.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}

fn zip_entries(bytes: &[u8], needed: &str) -> Result<ZipEntries, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Corrupt ZIP: {error}"))?;
    let mut total = 0u64;
    let mut primary = None;
    let mut others = Vec::new();
    for index in 0..zip.len() {
        let mut file = zip
            .by_index(index)
            .map_err(|error| format!("Corrupt ZIP entry: {error}"))?;
        let name = file
            .name()
            .map(|n| n.into_owned())
            .map_err(|error| format!("Unusable ZIP entry name: {error}"))?;
        if file.is_dir() {
            continue;
        }
        if file.size() > MAX_ZIP_ENTRY_BYTES {
            return Err(format!(
                "ZIP entry '{name}' exceeds the 64 MiB safety limit"
            ));
        }
        total = total.saturating_add(file.size());
        if total > MAX_ZIP_TOTAL_BYTES {
            return Err("ZIP exceeds the 256 MiB expanded-data safety limit".into());
        }
        let mut data = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut data)
            .map_err(|error| format!("Cannot read ZIP entry '{name}': {error}"))?;
        if name == needed {
            primary = Some(data);
        } else {
            others.push((name, data));
        }
    }
    primary
        .ok_or_else(|| format!("ZIP does not contain required '{needed}'"))
        .map(|primary| (primary, others))
}

fn bitwarden_history(item: &Value) -> Vec<Value> {
    item.get("passwordHistory")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| {
            let value = text(record.get("password"))?;
            let saved_at = text(record.get("lastUsedDate"))
                .or_else(|| text(record.get("lastUsedDateUtc")))
                .unwrap_or_else(vault_core::iso_now);
            Some(json!({ "value": value, "saved_at": saved_at, "field": "api_key" }))
        })
        .collect()
}

fn bitwarden_fields(item: &Value, extra_vars: &mut Vec<Value>, reports: &mut Vec<String>) {
    for field in item
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = text(field.get("name")).unwrap_or_else(|| "custom field".into());
        let field_type = field.get("type").and_then(Value::as_u64).unwrap_or(0);
        match field_type {
            0 | 1 => {
                if let Some(value) = scalar(field.get("value").unwrap_or(&Value::Null)) {
                    // Text can be just as credential-bearing as hidden data, so
                    // only booleans and URLs become public by declaration.
                    extra_vars.push(extra(name, value, false));
                }
            }
            2 => {
                if let Some(value) = scalar(field.get("value").unwrap_or(&Value::Null)) {
                    extra_vars.push(extra(name, value, true));
                }
            }
            3 => reports.push(format!(
                "linked Bitwarden custom field '{name}' was reported but not resolved"
            )),
            other => reports.push(format!(
                "unknown Bitwarden custom field type {other} was retained in the source archive"
            )),
        }
    }
}

fn bitwarden_item(item: &Value, folders: &HashMap<String, String>) -> Result<Item, ArchivedItem> {
    let raw = serde_json::to_vec(item).unwrap_or_default();
    let id = match text(item.get("id")) {
        Some(id) => id,
        None => {
            return Err(ArchivedItem {
                source_id: format!("missing-id:{}", hash(&raw)),
                source_hash: hash(&raw),
                name: text(item.get("name")).unwrap_or_else(|| "Bitwarden item without id".into()),
                archive: Archive {
                    source_id: None,
                    reason: "Bitwarden item has no stable id and cannot be reconciled".into(),
                    media_type: "application/json".into(),
                    bytes: raw,
                },
            })
        }
    };
    let name = text(item.get("name")).unwrap_or_else(|| format!("Bitwarden {id}"));
    let folder = text(item.get("folderId")).and_then(|id| folders.get(&id).cloned());
    let kind = item.get("type").and_then(Value::as_u64).unwrap_or(0);
    let mut extra_vars = Vec::new();
    let mut reports = Vec::new();
    bitwarden_fields(item, &mut extra_vars, &mut reports);
    let mut entry = match kind {
        1 => {
            let login = item.get("login").unwrap_or(&Value::Null);
            let password = text(login.get("password")).unwrap_or_default();
            let mut entry = base_entry(name.clone(), password.clone(), classify(&password));
            if let Some(username) = text(login.get("username")) {
                entry["account_name"] = json!(username);
            }
            let uris: Vec<String> = login
                .get("uris")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|uri| text(uri.get("uri")))
                .collect();
            if let Some(url) = uris.first() {
                entry["api_url"] = json!(url);
            }
            for (index, uri) in uris.iter().enumerate() {
                extra_vars.push(extra(format!("url_{}", index + 1), uri, true));
            }
            add_totp(&mut entry, text(login.get("totp")), &mut reports);
            entry
        }
        2 => base_entry(
            name.clone(),
            text(item.get("notes")).unwrap_or_default(),
            "secure_note",
        ),
        3 => {
            let card = item.get("card").unwrap_or(&Value::Null);
            let number = text(card.get("number")).unwrap_or_default();
            for (key, public) in [
                ("cardholderName", true),
                ("brand", true),
                ("expMonth", false),
                ("expYear", false),
                ("code", false),
            ] {
                if let Some(value) = scalar(card.get(key).unwrap_or(&Value::Null)) {
                    extra_vars.push(extra(key, value, public));
                }
            }
            base_entry(name.clone(), number, "api_key")
        }
        4 => {
            let identity = item.get("identity").unwrap_or(&Value::Null);
            if let Some(values) = identity.as_object() {
                for (key, value) in values {
                    if let Some(value) = scalar(value) {
                        extra_vars.push(extra(key, value, false));
                    }
                }
            }
            base_entry(name.clone(), String::new(), "identity_document")
        }
        5 => {
            let key = item.get("sshKey").unwrap_or(&Value::Null);
            let private = text(key.get("privateKey")).unwrap_or_default();
            if let Some(public) = text(key.get("publicKey")) {
                extra_vars.push(extra("public_key", public, true));
            }
            if let Some(fingerprint) = text(key.get("keyFingerprint")) {
                extra_vars.push(extra("fingerprint", fingerprint, true));
            }
            base_entry(name.clone(), private, "ssh_key")
        }
        6 => {
            let account = item.get("bankAccount").unwrap_or(&Value::Null);
            let number = text(account.get("accountNumber")).unwrap_or_default();
            if let Some(values) = account.as_object() {
                for (key, value) in values {
                    if key != "accountNumber" {
                        if let Some(value) = scalar(value) {
                            extra_vars.push(extra(key, value, false));
                        }
                    }
                }
            }
            base_entry(name.clone(), number, "api_key")
        }
        7 | 8 => {
            let document = if kind == 7 {
                item.get("driverLicense")
            } else {
                item.get("passport")
            }
            .unwrap_or(&Value::Null);
            let number = text(document.get("number"))
                .or_else(|| text(document.get("licenseNumber")))
                .unwrap_or_default();
            if let Some(values) = document.as_object() {
                for (key, value) in values {
                    if let Some(value) = scalar(value) {
                        extra_vars.push(extra(key, value, false));
                    }
                }
            }
            base_entry(name.clone(), number, "identity_document")
        }
        _ => {
            return Err(ArchivedItem {
                source_id: id,
                source_hash: hash(&raw),
                name,
                archive: Archive {
                    source_id: None,
                    reason: format!("unsupported Bitwarden cipher type {kind}"),
                    media_type: "application/json".into(),
                    bytes: raw,
                },
            })
        }
    };
    if let Some(notes) = text(item.get("notes")) {
        if kind != 2 {
            entry["notes"] = json!(notes);
        }
    }
    Ok(source_item(
        id,
        name,
        item.clone(),
        entry,
        folder.into_iter().collect(),
        Vec::new(),
        extra_vars,
        None,
        bitwarden_history(item),
        Vec::new(),
        reports,
    ))
}

fn parse_bitwarden(label: String, source_bytes: Vec<u8>) -> Result<Parsed, String> {
    let (data, attachments) = if source_bytes.starts_with(b"PK\x03\x04") {
        zip_entries(&source_bytes, "data.json")?
    } else {
        (source_bytes.clone(), Vec::new())
    };
    let document: Value = serde_json::from_slice(&data)
        .map_err(|error| format!("Not a Bitwarden JSON export: {error}"))?;
    if document.get("items").and_then(Value::as_array).is_none() {
        if document.get("encrypted").is_some()
            || document.get("encKeyValidation_DO_NOT_EDIT").is_some()
        {
            return Err("Bitwarden encrypted or password-protected exports are refused by ADR-0093; export plaintext JSON or ZIP-with-attachments instead".into());
        }
        return Err("Bitwarden export has no items array".into());
    }
    let folders = document
        .get("folders")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|folder| Some((text(folder.get("id"))?, text(folder.get("name"))?)))
        .collect();
    let mut parsed = Parsed {
        format: SourceFormat::Bitwarden,
        label,
        source_bytes,
        items: Vec::new(),
        archived: Vec::new(),
        warnings: vec!["Bitwarden support is implemented from the published export format and is not verified against a sanitized real export.".into(), "Bitwarden individual-vault exports omit organization-owned and trashed items; those are missing at source.".into()],
        missing_at_source: vec!["organization-owned items (individual export)".into(), "trashed items".into()],
    };
    for item in document
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match bitwarden_item(item, &folders) {
            Ok(item) => parsed.items.push(item),
            Err(archived) => parsed.archived.push(archived),
        }
    }
    for (path, bytes) in attachments {
        if !path.starts_with("attachments/") {
            continue;
        }
        let source_id = parsed
            .items
            .iter()
            .find(|item| path.contains(&item.source_id))
            .map(|item| item.source_id.clone());
        if source_id.is_none() {
            parsed.warnings.push(format!(
                "Bitwarden attachment '{path}' was retained but could not be matched to an item id"
            ));
        }
        if let Some(item) = source_id
            .as_ref()
            .and_then(|id| parsed.items.iter_mut().find(|item| &item.source_id == id))
        {
            item.archives.push(Archive {
                source_id: source_id.clone(),
                reason: format!("Bitwarden attachment {path}"),
                media_type: media_type(&path).into(),
                bytes,
            });
        } else {
            parsed.archived.push(ArchivedItem {
                source_id: format!("attachment:{}", hash(path.as_bytes())),
                source_hash: hash(&bytes),
                name: path.clone(),
                archive: Archive {
                    source_id: None,
                    reason: format!("unmatched Bitwarden attachment {path}"),
                    media_type: media_type(&path).into(),
                    bytes,
                },
            });
        }
    }
    for item in &mut parsed.items {
        refresh_item_hash(item);
    }
    Ok(parsed)
}

/// A .1pux section field's value is an object keyed by its type
/// (`{"concealed": "…"}`, `{"totp": "otpauth://…"}`, `{"url": "…"}`), not a bare string.
const ONEPASSWORD_TYPED_VALUES: [&str; 10] = [
    "string",
    "concealed",
    "totp",
    "url",
    "phone",
    "reference",
    "menu",
    "gender",
    "creditCardNumber",
    "creditCardType",
];

fn onepassword_field_value(field: &Value) -> Option<String> {
    text(field.get("value"))
        .or_else(|| text(field.get("v")))
        .or_else(|| scalar(field.get("value").unwrap_or(&Value::Null)))
        .or_else(|| {
            let typed = field.get("value")?.as_object()?;
            ONEPASSWORD_TYPED_VALUES
                .iter()
                .find_map(|kind| text(typed.get(*kind)))
                .or_else(|| typed.get("monthYear").and_then(scalar))
                .or_else(|| text(typed.get("email").and_then(|e| e.get("email_address"))))
        })
}

fn onepassword_fields(item: &Value) -> Vec<&Value> {
    let mut fields = Vec::new();
    fields.extend(
        item.get("fields")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
    );
    if let Some(details) = item.get("details") {
        // The login's own username and password sit here in a real .1pux, marked by
        // `designation`, apart from the sections.
        fields.extend(
            details
                .get("loginFields")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        );
        fields.extend(
            details
                .get("fields")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        );
        for section in details
            .get("sections")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            fields.extend(
                section
                    .get("fields")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            );
        }
    }
    fields
}

fn onepassword_title(item: &Value) -> String {
    text(item.get("title"))
        .or_else(|| text(item.get("overview").and_then(|value| value.get("title"))))
        .or_else(|| text(item.get("attrs").and_then(|value| value.get("title"))))
        .unwrap_or_else(|| "1Password item".into())
}

fn onepassword_item(item: &Value, vault_name: Option<String>) -> Result<Item, ArchivedItem> {
    let raw = serde_json::to_vec(item).unwrap_or_default();
    let source_id = text(item.get("uuid")).or_else(|| text(item.get("id")));
    let Some(source_id) = source_id else {
        return Err(ArchivedItem {
            source_id: format!("missing-uuid:{}", hash(&raw)),
            source_hash: hash(&raw),
            name: onepassword_title(item),
            archive: Archive {
                source_id: None,
                reason: "1Password item has no UUID and cannot be reconciled".into(),
                media_type: "application/json".into(),
                bytes: raw,
            },
        });
    };
    let name = onepassword_title(item);
    let mut password = None;
    let mut username = None;
    let mut notes = None;
    let mut url = text(item.get("url"))
        .or_else(|| text(item.get("overview").and_then(|value| value.get("url"))));
    let mut totp = None;
    let mut extra_vars = Vec::new();
    let mut reports = Vec::new();
    for field in onepassword_fields(item) {
        let label = text(field.get("label"))
            .or_else(|| text(field.get("title")))
            .or_else(|| text(field.get("name")))
            .unwrap_or_else(|| "field".into());
        let id = text(field.get("id")).unwrap_or_default();
        let purpose = text(field.get("purpose")).unwrap_or_default();
        let designation = text(field.get("designation")).unwrap_or_default();
        let value = onepassword_field_value(field);
        let is_password = purpose.eq_ignore_ascii_case("PASSWORD")
            || id == "password"
            || designation.eq_ignore_ascii_case("password");
        let is_username = purpose.eq_ignore_ascii_case("USERNAME")
            || id == "username"
            || designation.eq_ignore_ascii_case("username");
        let is_notes = purpose.eq_ignore_ascii_case("NOTES") || id == "notesPlain";
        let is_url = purpose.eq_ignore_ascii_case("URL")
            || id == "website"
            || label.eq_ignore_ascii_case("website");
        let is_totp = purpose.eq_ignore_ascii_case("OTP")
            || id.to_ascii_lowercase().contains("otp")
            || label.to_ascii_lowercase().contains("one-time")
            || label.to_ascii_lowercase().contains("otp");
        if is_password {
            password = password.or(value);
        } else if is_username {
            username = username.or(value);
        } else if is_notes {
            notes = notes.or(value);
        } else if is_url {
            url = url.or(value);
        } else if is_totp {
            totp = totp.or(value);
        } else if let Some(value) = value {
            // A 1Password field's type is not a declaration of safety.  Keep it
            // redacted unless its role is an explicit URL.
            extra_vars.push(extra(label, value, false));
        }
    }
    let password = password
        .or_else(|| text(item.get("details").and_then(|value| value.get("password"))))
        .unwrap_or_default();
    let mut entry = base_entry(name.clone(), password.clone(), classify(&password));
    if let Some(username) = username {
        entry["account_name"] = json!(username);
    }
    if let Some(url) = url {
        entry["api_url"] = json!(url);
    }
    if let Some(notes) = notes.or_else(|| {
        text(
            item.get("details")
                .and_then(|value| value.get("notesPlain")),
        )
    }) {
        entry["notes"] = json!(notes);
    }
    if password.is_empty() {
        reports.push("item has no dedicated password; its remaining fields were retained as redacted extra variables".into());
    }
    Ok(source_item(
        source_id,
        name,
        item.clone(),
        entry,
        vault_name.into_iter().collect(),
        Vec::new(),
        extra_vars,
        totp,
        Vec::new(),
        Vec::new(),
        reports,
    ))
}

fn onepassword_walk(document: &Value) -> Vec<(&Value, Option<String>)> {
    let mut items = Vec::new();
    if let Some(array) = document.as_array() {
        items.extend(array.iter().map(|item| (item, None)));
    }
    for account in document
        .get("accounts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for vault in account
            .get("vaults")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = text(vault.get("name"))
                .or_else(|| text(vault.get("attrs").and_then(|value| value.get("name"))));
            for outer in vault
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                items.push((outer.get("item").unwrap_or(outer), name.clone()));
            }
        }
    }
    if let Some(array) = document.get("items").and_then(Value::as_array) {
        items.extend(array.iter().map(|item| (item, None)));
    }
    items
}

fn parse_onepassword_document(
    label: String,
    source_bytes: Vec<u8>,
    document: Value,
    files: Vec<(String, Vec<u8>)>,
) -> Result<Parsed, String> {
    let mut parsed = Parsed {
        format: SourceFormat::OnePassword,
        label,
        source_bytes,
        items: Vec::new(),
        archived: Vec::new(),
        warnings: vec!["1Password support is implemented from the published .1pux structure and is not verified against a sanitized real export.".into()],
        // The published .1pux structure carries no exported passkey material.
        missing_at_source: vec!["passkeys (not represented in .1pux exports)".into()],
    };
    for (item, vault_name) in onepassword_walk(&document) {
        match onepassword_item(item, vault_name) {
            Ok(item) => parsed.items.push(item),
            Err(archived) => parsed.archived.push(archived),
        }
    }
    if parsed.items.is_empty() && parsed.archived.is_empty() {
        return Err("1Password export contains no items".into());
    }
    for (path, bytes) in files {
        if !path.starts_with("files/") {
            continue;
        }
        let source_id = parsed
            .items
            .iter()
            .find(|item| path.contains(&item.source_id))
            .map(|item| item.source_id.clone());
        if let Some(item) = source_id
            .as_ref()
            .and_then(|id| parsed.items.iter_mut().find(|item| &item.source_id == id))
        {
            item.archives.push(Archive {
                source_id: source_id.clone(),
                reason: format!("1Password document {path}"),
                media_type: media_type(&path).into(),
                bytes,
            });
        } else {
            parsed.warnings.push(format!(
                "1Password document '{path}' was retained but could not be matched to an item UUID"
            ));
            parsed.archived.push(ArchivedItem {
                source_id: format!("file:{}", hash(path.as_bytes())),
                source_hash: hash(&bytes),
                name: path.clone(),
                archive: Archive {
                    source_id: None,
                    reason: format!("unmatched 1Password document {path}"),
                    media_type: media_type(&path).into(),
                    bytes,
                },
            });
        }
    }
    for item in &mut parsed.items {
        refresh_item_hash(item);
    }
    Ok(parsed)
}

fn parse_onepassword(label: String, source_bytes: Vec<u8>) -> Result<Parsed, String> {
    if source_bytes.starts_with(b"PK\x03\x04") {
        let (data, files) = zip_entries(&source_bytes, "export.data")?;
        let document: Value = serde_json::from_slice(&data)
            .map_err(|error| format!("Invalid 1Password export.data: {error}"))?;
        parse_onepassword_document(label, source_bytes, document, files)
    } else {
        let document: Value = serde_json::from_slice(&source_bytes)
            .map_err(|error| format!("Not a 1Password JSON export: {error}"))?;
        parse_onepassword_document(label, source_bytes, document, Vec::new())
    }
}

fn kee_pass_timestamp(time: Option<chrono::NaiveDateTime>) -> String {
    time.map(|time| format!("{time}Z"))
        .unwrap_or_else(vault_core::iso_now)
}

fn kee_pass_group_path(entry: &keepass::db::EntryRef<'_>) -> (Vec<String>, Vec<String>) {
    let mut names = Vec::new();
    let mut tags = entry.tags.clone();
    let mut group_id = Some(entry.parent().id());
    while let Some(id) = group_id {
        let Some(current) = entry.database().group(id) else {
            break;
        };
        if !current.name.is_empty() {
            names.push(current.name.clone());
        }
        tags.extend(current.tags.clone());
        group_id = current.parent().map(|parent| parent.id());
    }
    names.reverse();
    tags.sort();
    tags.dedup();
    let categories = names.last().cloned().into_iter().collect();
    (categories, tags)
}

fn open_keepass(bytes: &[u8], key: keepass::DatabaseKey) -> Result<keepass::Database, String> {
    keepass::Database::parse(bytes, key).map_err(|error| {
        let message = error.to_string();
        if message.to_ascii_lowercase().contains("incorrect") {
            "Cannot unlock KeePass database: incorrect password or key file".into()
        } else {
            format!("Cannot read KeePass database: {message}")
        }
    })
}

fn parse_keepass(
    label: String,
    source_bytes: Vec<u8>,
    key_file: Option<&Path>,
    no_password: bool,
) -> Result<Parsed, String> {
    let mut key = keepass::DatabaseKey::new();
    if !no_password {
        let password = rpassword::prompt_password(
            "KeePass database password (leave empty for key-file only): ",
        )
        .map_err(|error| format!("Cannot read KeePass password: {error}"))?;
        if !password.is_empty() {
            key = key.with_password(&password);
        }
    }
    if let Some(path) = key_file {
        let mut file = std::fs::File::open(path)
            .map_err(|error| format!("Cannot read KeePass key file {}: {error}", path.display()))?;
        key = key
            .with_keyfile(&mut file)
            .map_err(|error| format!("Cannot read KeePass key file {}: {error}", path.display()))?;
    }
    if key.is_empty() {
        return Err("KeePass needs a password, a key file, or both".into());
    }
    let database = open_keepass(&source_bytes, key)?;
    let mut parsed = Parsed {
        format: SourceFormat::KeePass,
        label,
        source_bytes,
        items: Vec::new(),
        archived: Vec::new(),
        warnings: vec!["KeePass support is implemented from keepass-rs 0.15 and is not verified against a sanitized real KDBX export.".into()],
        missing_at_source: Vec::new(),
    };
    for entry_ref in database.iter_all_entries() {
        let source_id = entry_ref.id().to_string();
        let name = entry_ref
            .get(kp_fields::TITLE)
            .filter(|value| !value.is_empty())
            .unwrap_or("KeePass entry")
            .to_owned();
        let password = entry_ref
            .get(kp_fields::PASSWORD)
            .unwrap_or_default()
            .to_owned();
        let mut entry = base_entry(name.clone(), password.clone(), classify(&password));
        if let Some(username) = entry_ref
            .get(kp_fields::USERNAME)
            .filter(|value| !value.is_empty())
        {
            entry["account_name"] = json!(username);
        }
        if let Some(url) = entry_ref
            .get(kp_fields::URL)
            .filter(|value| !value.is_empty())
        {
            entry["api_url"] = json!(url);
        }
        if let Some(notes) = entry_ref
            .get(kp_fields::NOTES)
            .filter(|value| !value.is_empty())
        {
            entry["notes"] = json!(notes);
        }
        let (categories, tags) = kee_pass_group_path(&entry_ref);
        let mut reports = Vec::new();
        let mut extra_vars = Vec::new();
        let mut totp = entry_ref.get(kp_fields::OTP).map(str::to_owned);
        for (field, value) in &entry_ref.fields {
            if [
                kp_fields::TITLE,
                kp_fields::USERNAME,
                kp_fields::PASSWORD,
                kp_fields::URL,
                kp_fields::NOTES,
                kp_fields::OTP,
            ]
            .contains(&field.as_str())
            {
                continue;
            }
            let value = value.get().to_owned();
            if field == "TOTP Seed" && totp.is_none() {
                totp = Some(value);
                continue;
            }
            if field == "TOTP Settings" {
                extra_vars.push(extra(field, value, false));
                continue;
            }
            ref_reports(&value, field, &mut reports);
            // Protected source fields remain secret. Unprotected fields are also
            // fail-closed unless a later explicit format mapping proves public.
            extra_vars.push(extra(field, value, false));
        }
        let history = entry_ref.history.as_ref().map(|history| history.get_entries().iter().filter_map(|old| {
            let value = old.get(kp_fields::PASSWORD)?.to_owned();
            (!value.is_empty()).then(|| json!({ "value": value, "saved_at": kee_pass_timestamp(old.times.last_modification), "field": "api_key" }))
        }).collect()).unwrap_or_default();
        let mut archives = Vec::new();
        for (attachment_name, attachment) in entry_ref.attachments_named() {
            archives.push(Archive {
                source_id: Some(source_id.clone()),
                reason: format!("KeePass attachment {attachment_name}"),
                media_type: media_type(attachment_name).into(),
                bytes: attachment.data.get().clone(),
            });
        }
        if entry_ref.autotype.is_some()
            || entry_ref.icon().is_some()
            || !entry_ref.custom_data.is_empty()
        {
            archives.push(Archive { source_id: Some(source_id.clone()), reason: "KeePass auto-type, icon and custom data metadata".into(), media_type: "application/json".into(), bytes: serde_json::to_vec(&json!({ "auto_type": format!("{:?}", entry_ref.autotype), "icon": format!("{:?}", entry_ref.icon()), "custom_data": format!("{:?}", entry_ref.custom_data) })).unwrap_or_default() });
        }
        parsed.items.push(source_item(
            source_id,
            name,
            json!({
                "entry": entry,
                "categories": categories,
                "tags": tags,
                "history": history,
            }),
            entry,
            categories,
            tags,
            extra_vars,
            totp,
            history,
            archives,
            reports,
        ));
    }
    if parsed.items.is_empty() {
        return Err("KeePass database contains no entries".into());
    }
    Ok(parsed)
}

fn parse_source(format: SourceFormat, file: &Path, options: &SourceOpts<'_>) -> CliResult<Parsed> {
    if !matches!(
        format,
        SourceFormat::Bitwarden | SourceFormat::OnePassword | SourceFormat::KeePass
    ) {
        if !matches!(
            format,
            SourceFormat::Sops | SourceFormat::Pass | SourceFormat::Gopass
        ) {
            return Err(CliError::invalid(
                "service sources are read with `unv import-service`, not from a file",
            ));
        }
        // These sources are files and folders read through the user's own tool, not one blob.
        return crate::git_sources::parse(format, file, &crate::git_sources::Tools::default())
            .map_err(CliError::invalid);
    }
    let source_bytes = std::fs::read(file)
        .map_err(|error| CliError::not_found(format!("Cannot read {}: {error}", file.display())))?;
    let label = file.to_string_lossy().into_owned();
    match format {
        SourceFormat::Bitwarden => parse_bitwarden(label, source_bytes),
        SourceFormat::OnePassword => parse_onepassword(label, source_bytes),
        SourceFormat::KeePass => {
            parse_keepass(label, source_bytes, options.key_file, options.no_password)
        }
        _ => unreachable!("handled before the file is read"),
    }
    .map_err(CliError::invalid)
}

/// `(source id, source hash)` for every item the source holds right now, read-only. The drift
/// check compares these with what the vault last imported.
pub fn source_state(
    format: SourceFormat,
    file: &Path,
    options: &SourceOpts<'_>,
) -> CliResult<Vec<(String, String)>> {
    let parsed = parse_source(format, file, options)?;
    Ok(parsed
        .items
        .iter()
        .map(|i| (i.source_id.clone(), i.source_hash.clone()))
        .chain(
            parsed
                .archived
                .iter()
                .map(|a| (a.source_id.clone(), a.source_hash.clone())),
        )
        .collect())
}

/// Exercises the untrusted JSON and ZIP reader boundaries from cargo-fuzz.
#[doc(hidden)]
pub fn fuzz_json_zip(bytes: &[u8]) {
    let _ = parse_bitwarden("fuzz".into(), bytes.to_vec());
    let _ = parse_onepassword("fuzz".into(), bytes.to_vec());
}

fn entry_rev(entry: &Value) -> String {
    source_hash(entry)
}

fn created_at_from(existing: Option<&Value>, now: &str) -> Value {
    existing
        .and_then(|entry| entry.get("created_at"))
        .cloned()
        .unwrap_or_else(|| json!(now))
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

fn with_import_options(mut entry: Value, options: &SourceOpts<'_>) -> Value {
    if let Some(project) = options.project {
        entry["projectIds"] = json!([project]);
    }
    if let Some(category) = options.category {
        entry["categories"] = json!([category]);
    } else if !options.keep_folders {
        entry["categories"] = json!([]);
    }
    entry
}

fn plan(access: &Access, parsed: Parsed, options: &SourceOpts<'_>) -> CliResult<Plan> {
    let vault = access.load_vault_or_empty()?;
    let conn = access.conn()?;
    let mut planned = Vec::new();
    let mut archived = Vec::new();
    let mut counts = Counts {
        read: parsed.items.len() + parsed.archived.len() + parsed.missing_at_source.len(),
        imported: 0,
        archived: 0,
        skipped: 0,
        missing_at_source: parsed.missing_at_source.len(),
        conflicts: 0,
    };
    for mut item in parsed.items.iter().cloned() {
        item.entry = with_import_options(item.entry, options);
        let previous =
            vault_core::migration::previous_item(&conn, parsed.format.as_str(), &item.source_id)
                .map_err(CliError::from)?;
        let current_rev = previous
            .as_ref()
            .and_then(|previous| previous.target_entry_id.as_deref())
            .and_then(|id| find_entry(&vault, id))
            .map(entry_rev);
        let (status, conflict, target_entry_id) =
            reconcile(&item.source_hash, previous, current_rev);
        match status.as_str() {
            "imported" | "updated" => counts.imported += 1,
            "conflict" => {
                counts.skipped += 1;
                counts.conflicts += 1;
            }
            _ => counts.skipped += 1,
        }
        planned.push(Planned {
            item,
            status,
            conflict,
            target_entry_id,
        });
    }
    for item in parsed.archived.iter().cloned() {
        let previous =
            vault_core::migration::previous_item(&conn, parsed.format.as_str(), &item.source_id)
                .map_err(CliError::from)?;
        let status = match previous {
            Some(previous) if previous.source_hash == item.source_hash => "skipped".to_string(),
            _ => "archived".to_string(),
        };
        if status == "archived" {
            counts.archived += 1;
        } else {
            counts.skipped += 1;
        }
        archived.push((item, status));
    }
    let mut items: Vec<PreviewItem> = planned
        .iter()
        .map(|item| PreviewItem {
            name: item.item.name.clone(),
            source_id: item.item.source_id.clone(),
            status: item.status.clone(),
            conflict: item.conflict.clone(),
        })
        .collect();
    items.extend(archived.iter().map(|(item, status)| PreviewItem {
        name: item.name.clone(),
        source_id: item.source_id.clone(),
        status: status.clone(),
        conflict: None,
    }));
    for missing in &parsed.missing_at_source {
        items.push(PreviewItem {
            name: missing.clone(),
            source_id: "missing-at-source".into(),
            status: "missing_at_source".into(),
            conflict: None,
        });
    }
    let mut warnings = parsed.warnings.clone();
    warnings.extend(planned.iter().flat_map(|planned| {
        planned
            .item
            .reports
            .iter()
            .map(|report| format!("{}: {report}", planned.item.name))
    }));
    let preview = Preview {
        format: parsed.format.as_str().into(),
        source: parsed.label.clone(),
        counts,
        items,
        warnings,
        missing_at_source: parsed.missing_at_source.clone(),
    };
    debug_assert!(preview.counts.adds_up());
    Ok(Plan {
        parsed,
        planned,
        archived,
        preview,
    })
}

fn record_item(
    kind: SourceFormat,
    item: &Item,
    planned: &Planned,
    entry: &Value,
) -> vault_core::migration::MigrationItem {
    vault_core::migration::MigrationItem {
        source_kind: kind.as_str().into(),
        source_id: item.source_id.clone(),
        source_hash: item.source_hash.clone(),
        target_entry_id: Some(planned.target_entry_id.clone()),
        target_rev: Some(entry_rev(entry)),
        status: planned.status.clone(),
        conflict: planned.conflict.clone(),
    }
}

pub(crate) fn source_archive(bytes: Vec<u8>) -> Archive {
    Archive {
        source_id: None,
        reason: "original source retained for provenance and future parser compatibility".into(),
        media_type: "application/octet-stream".into(),
        bytes,
    }
}

/// What [`import`] did, so a caller can print it ([`run`]) or just record it (the scheduler).
pub(crate) struct Imported {
    pub(crate) preview: Preview,
    pub(crate) mode: ImportMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportMode {
    /// `apply` was not set: nothing was written.
    Preview,
    /// `--dry-run`: the write was stopped at the write point.
    DryRun,
    /// Applied, but the source held nothing new.
    Nothing,
    /// Applied and written.
    Written,
}

/// Imports one full-fidelity source.  Preview is the default; the caller chooses application by
/// setting `apply` and this routine then saves the vault, source identities and blobs in one
/// transaction. It prints nothing.
pub(crate) fn import(
    access: &Access,
    format: SourceFormat,
    file: &Path,
    options: &SourceOpts<'_>,
    apply: bool,
) -> CliResult<Imported> {
    let parsed = parse_source(format, file, options)?;
    import_parsed(access, parsed, options, apply)
}

/// The same, for a source a caller already read (a service API rather than a file).
pub(crate) fn import_parsed(
    access: &Access,
    parsed: Parsed,
    options: &SourceOpts<'_>,
    apply: bool,
) -> CliResult<Imported> {
    let plan = plan(access, parsed, options)?;
    if !apply {
        return Ok(Imported {
            preview: plan.preview,
            mode: ImportMode::Preview,
        });
    }
    if plan.preview.counts.conflicts > 0 {
        return Err(CliError::conflict(format!(
            "{} source item(s) conflict; nothing was written",
            plan.preview.counts.conflicts
        )));
    }
    if crate::out::dry_run() {
        return Ok(Imported {
            preview: plan.preview,
            mode: ImportMode::DryRun,
        });
    }
    if !plan
        .planned
        .iter()
        .any(|item| matches!(item.status.as_str(), "imported" | "updated"))
        && !plan.archived.iter().any(|(_, status)| status == "archived")
    {
        return Ok(Imported {
            preview: plan.preview,
            mode: ImportMode::Nothing,
        });
    }
    let mut vault = access.load_vault_or_empty()?;
    let entries = vault
        .get_mut("api_keys")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| CliError::invalid("vault has no api_keys list"))?;
    let migration_id = format!("mig_{}", Uuid::new_v4().simple());
    let now = vault_core::iso_now();
    let mut records = Vec::new();
    let mut archives = vec![source_archive(plan.parsed.source_bytes.clone())];
    for planned in &plan.planned {
        let existing = entries.iter().find(|entry| {
            entry.get("id").and_then(Value::as_str) == Some(planned.target_entry_id.as_str())
        });
        let mut entry = existing
            .cloned()
            .unwrap_or_else(|| planned.item.entry.clone());
        if matches!(planned.status.as_str(), "imported" | "updated") {
            entry = planned.item.entry.clone();
            entry["id"] = json!(planned.target_entry_id);
            entry["created_at"] = created_at_from(existing, &now);
            entry["authority"] = json!({ "owner": "source", "source_kind": plan.parsed.format.as_str(), "source_id": planned.item.source_id, "migration_id": migration_id, "since": now });
            entry["provenance"] = json!({ "kind": "import", "source_kind": plan.parsed.format.as_str(), "migration_id": migration_id, "imported_at": now, "precision": "format-derived-unverified" });
            match entries.iter().position(|old| {
                old.get("id").and_then(Value::as_str) == Some(planned.target_entry_id.as_str())
            }) {
                Some(index) => entries[index] = entry.clone(),
                None => entries.push(entry.clone()),
            }
            for archive in &planned.item.archives {
                archives.push(archive.clone());
            }
        }
        records.push(record_item(
            plan.parsed.format,
            &planned.item,
            planned,
            &entry,
        ));
    }
    for (archived, status) in &plan.archived {
        records.push(vault_core::migration::MigrationItem {
            source_kind: plan.parsed.format.as_str().into(),
            source_id: archived.source_id.clone(),
            source_hash: archived.source_hash.clone(),
            target_entry_id: None,
            target_rev: None,
            status: status.clone(),
            conflict: None,
        });
        if status == "archived" {
            archives.push(archived.archive.clone());
        }
    }
    let record = vault_core::migration::MigrationRecord {
        id: migration_id,
        source_kind: plan.parsed.format.as_str().into(),
        source_label: plan.parsed.label.clone(),
        created_at: now,
        status: "applied".into(),
        manifest: json!({
            "counts": plan.preview.counts, "format_verification": "implemented from published format, not verified against a real export",
            "source_sha256": hash(&plan.parsed.source_bytes), "warnings": plan.preview.warnings,
            "missing_at_source": plan.preview.missing_at_source,
            // What the user chose, so a later sync from this source places entries the same way.
            "options": { "project": options.project, "category": options.category, "keep_folders": options.keep_folders },
        }),
        items: records,
        archives: archives
            .into_iter()
            .map(|archive| vault_core::migration::SourceArchive {
                id: format!("arc_{}", Uuid::new_v4().simple()),
                source_kind: plan.parsed.format.as_str().into(),
                source_id: archive.source_id,
                reason: archive.reason,
                media_type: archive.media_type,
                bytes: archive.bytes,
            })
            .collect(),
    };
    access.save_migration(&vault, &record)?;
    Ok(Imported {
        preview: plan.preview,
        mode: ImportMode::Written,
    })
}

/// [`import_parsed`], then print the result as the command line shows it.
pub(crate) fn run_parsed(
    access: &Access,
    parsed: Parsed,
    options: &SourceOpts<'_>,
    apply: bool,
) -> CliResult {
    let format = parsed.format;
    print_imported(format, import_parsed(access, parsed, options, apply)?)
}

fn print_imported(format: SourceFormat, imported: Imported) -> CliResult {
    let preview = &imported.preview;
    let show = |applied: bool| {
        crate::out::ok(
            &format!("import.{}", format.as_str()),
            serde_json::to_value(preview).unwrap_or_default(),
            || print_preview(preview, applied),
        )
    };
    match imported.mode {
        ImportMode::DryRun => {}
        ImportMode::Preview => show(false),
        ImportMode::Nothing | ImportMode::Written => show(true),
    }
    Ok(())
}

/// `import`, then print the result as the command line shows it.
pub fn run(
    access: &Access,
    format: SourceFormat,
    file: &Path,
    options: &SourceOpts<'_>,
    apply: bool,
) -> CliResult {
    print_imported(format, import(access, format, file, options, apply)?)
}

fn print_preview(preview: &Preview, applied: bool) {
    for warning in &preview.warnings {
        println!("warning: {warning}");
    }
    println!(
        "{} read: {} import, {} archive, {} skipped, {} missing at source.",
        preview.format,
        preview.counts.imported,
        preview.counts.archived,
        preview.counts.skipped,
        preview.counts.missing_at_source
    );
    for item in &preview.items {
        println!("  {:<28} {}", item.name, item.status);
    }
    if preview.counts.conflicts > 0 {
        println!(
            "{} conflict(s); nothing was written.",
            preview.counts.conflicts
        );
    }
    if !applied {
        println!("Nothing was written. Re-run with --apply.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip(parts: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in parts {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn bitwarden_preserves_every_known_type_and_archives_an_unknown_type() {
        let document = json!({
            "folders": [{"id":"f","name":"Personal"}],
            "items": [
                {"id":"login","type":1,"name":"Login","folderId":"f","login":{"username":"me","password":"EXAMPLE_password","uris":[{"uri":"https://example.test"}],"totp":"JBSWY3DPEHPK3PXP"},"passwordHistory":[{"password":"EXAMPLE_old","lastUsedDate":"2024-01-01T00:00:00Z"}]},
                {"id":"note","type":2,"name":"Note","notes":"EXAMPLE_note"},
                {"id":"card","type":3,"name":"Card","card":{"number":"EXAMPLE_card","code":"EXAMPLE_cvv"}},
                {"id":"identity","type":4,"name":"Identity","identity":{"firstName":"Taylor","ssn":"EXAMPLE_ssn"}},
                {"id":"ssh","type":5,"name":"SSH","sshKey":{"privateKey":"-----BEGIN PRIVATE KEY-----\nEXAMPLE\n-----END PRIVATE KEY-----","publicKey":"ssh-ed25519 EXAMPLE"}},
                {"id":"bank","type":6,"name":"Bank","bankAccount":{"accountNumber":"EXAMPLE_account","routingNumber":"EXAMPLE_routing"}},
                {"id":"licence","type":7,"name":"Licence","driverLicense":{"number":"EXAMPLE_licence"}},
                {"id":"passport","type":8,"name":"Passport","passport":{"number":"EXAMPLE_passport"}},
                {"id":"future","type":99,"name":"Future","data":"EXAMPLE_future"}
            ]
        });
        let parsed = parse_bitwarden(
            "fixture.json".into(),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.items.len(), 8);
        assert_eq!(parsed.archived.len(), 1);
        assert_eq!(parsed.archived[0].source_id, "future");
        let login = parsed
            .items
            .iter()
            .find(|item| item.source_id == "login")
            .unwrap();
        assert_eq!(login.entry["totp_secret"], "JBSWY3DPEHPK3PXP");
        assert_eq!(login.entry["version_history"].as_array().unwrap().len(), 1);
        assert_eq!(
            parsed
                .items
                .iter()
                .find(|item| item.source_id == "ssh")
                .unwrap()
                .entry["secretType"],
            "ssh_key"
        );
    }

    #[test]
    fn zip_attachment_bytes_are_retained_without_extraction() {
        let attachment = b"\x00EXAMPLE attachment bytes\xff";
        let data = br#"{"folders":[],"items":[{"id":"item-1","type":1,"name":"Login","login":{"password":"EXAMPLE_password"}}]}"#;
        let archive = zip(&[
            ("data.json", data),
            ("attachments/item-1/proof.bin", attachment),
        ]);
        let parsed = parse_bitwarden("fixture.zip".into(), archive).unwrap();
        assert_eq!(parsed.items[0].archives.len(), 1);
        assert_eq!(parsed.items[0].archives[0].bytes, attachment);
    }

    // A source update that changed only attachment bytes used to be skipped,
    // because reconciliation hashed the entry and excluded its archives.
    #[test]
    fn attachment_change_updates_the_source_identity() {
        let data = br#"{"folders":[],"items":[{"id":"item-1","type":1,"name":"Login","login":{"password":"EXAMPLE_password"}}]}"#;
        let original = parse_bitwarden(
            "original.zip".into(),
            zip(&[
                ("data.json", data),
                ("attachments/item-1/proof.bin", b"EXAMPLE original"),
            ]),
        )
        .unwrap();
        let changed = parse_bitwarden(
            "changed.zip".into(),
            zip(&[
                ("data.json", data),
                ("attachments/item-1/proof.bin", b"EXAMPLE changed"),
            ]),
        )
        .unwrap();
        assert_ne!(original.items[0].source_hash, changed.items[0].source_hash);
    }

    // An update used to overwrite the original creation time with its import
    // time, making a source refresh look like a newly-created vault entry.
    #[test]
    fn source_update_preserves_the_original_creation_time() {
        let existing = json!({ "created_at": "2024-01-01T00:00:00Z" });
        assert_eq!(
            created_at_from(Some(&existing), "2026-10-14T00:00:00Z"),
            "2024-01-01T00:00:00Z"
        );
        assert_eq!(
            created_at_from(None, "2026-10-14T00:00:00Z"),
            "2026-10-14T00:00:00Z"
        );
    }

    #[test]
    fn json_and_zip_reader_fuzz_smoke_is_panic_free() {
        // This deterministic, time-bounded corpus exercises the same entry
        // point as the cargo-fuzz target when cargo-fuzz or its nightly
        // toolchain is unavailable in a contributor environment.
        for case in 0_u64..512 {
            let mut state = case.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let mut bytes = Vec::with_capacity((case as usize % 1024) + 4);
            if case % 3 == 0 {
                bytes.extend_from_slice(b"PK\x03\x04");
            }
            for _ in 0..(case as usize % 1024) {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                bytes.push(state as u8);
            }
            fuzz_json_zip(&bytes);
        }
    }

    #[test]
    fn onepassword_zip_retains_documents_and_reports_missing_passkeys() {
        let data = br#"{"accounts":[{"vaults":[{"attrs":{"name":"Personal"},"items":[{"item":{"uuid":"op-1","overview":{"title":"Login"},"details":{"fields":[{"id":"password","value":"EXAMPLE_password"},{"id":"username","value":"me"},{"label":"One-time password","value":"JBSWY3DPEHPK3PXP"}]}}}]}]}]}"#;
        let file = b"EXAMPLE 1Password document";
        let archive = zip(&[
            ("export.data", data),
            ("export.attributes", b"{}"),
            ("files/op-1/document.txt", file),
        ]);
        let parsed = parse_onepassword("fixture.1pux".into(), archive).unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].archives[0].bytes, file);
        assert!(parsed
            .missing_at_source
            .iter()
            .any(|value| value.contains("passkeys")));
        assert_eq!(parsed.items[0].entry["totp_secret"], "JBSWY3DPEHPK3PXP");
    }

    // A corrupt ZIP used to bubble a panic from the container reader instead of
    // a usable import error, leaving the caller with no way to retry safely.
    #[test]
    fn corrupt_zip_is_a_clean_error() {
        let error = parse_bitwarden("broken.zip".into(), b"PK\x03\x04broken".to_vec()).unwrap_err();
        assert!(error.contains("Corrupt ZIP"), "{error}");
    }

    // A wrong KDBX password used to be indistinguishable from a parser failure.
    // The reader now returns a narrow, secret-free unlock error.
    #[test]
    fn wrong_kdbx_password_is_a_clean_error() {
        let database = keepass::Database::new();
        let mut bytes = Vec::new();
        database
            .save(
                &mut bytes,
                keepass::DatabaseKey::new().with_password("correct-password"),
            )
            .unwrap();
        let error = open_keepass(
            &bytes,
            keepass::DatabaseKey::new().with_password("wrong-password"),
        )
        .unwrap_err();
        assert_eq!(
            error,
            "Cannot unlock KeePass database: incorrect password or key file"
        );
        assert!(!error.contains("wrong-password"), "{error}");
    }

    #[test]
    fn count_report_conserves_every_source_item() {
        let counts = Counts {
            read: 7,
            imported: 3,
            archived: 2,
            skipped: 1,
            missing_at_source: 1,
            conflicts: 1,
        };
        assert!(counts.adds_up());
    }
}
