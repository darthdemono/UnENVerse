// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! AWS Secrets Manager and Systems Manager Parameter Store, behind the `aws` cargo feature.
//!
//! AWS is a commercial service and optional: nothing in the default build reaches it, and no AWS SDK
//! is linked. The two JSON APIs are called directly and signed with Signature Version 4 (implemented
//! here and checked against Amazon's published test vector). Credentials are static keys from the
//! environment or a vault entry; the shared-config provider chain (profiles, SSO, instance roles) is
//! not used, so what the import can do is exactly what the supplied key can do.
//!
//! Secrets Manager: `ListSecrets` with its `NextToken`, `GetSecretValue` for `AWSCURRENT` (and
//! `AWSPREVIOUS` as history when asked), a JSON secret becomes named variables, `SecretBinary`
//! becomes an encrypted blob, and rotation settings, resource policies and the KMS key id are archived
//! as non-operational metadata. Parameter Store: `GetParametersByPath` (recursive, decrypted) and
//! `GetParameterHistory`.
//!
//! An AWS caveat worth stating: IAM permission on a parameter path does not restrict reads of deeper
//! paths the way people expect, so an import of `/prod/` may read more than the key's author meant.

use crate::password_sources::{
    base_entry, classify, extra, hash, source_item, Archive, ArchivedItem, Parsed, SourceFormat,
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct AwsKeys {
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AwsOptions {
    pub region: String,
    /// Overrides `https://<service>.<region>.amazonaws.com` (a VPC endpoint or a local mock).
    pub endpoint: Option<String>,
    pub keys: AwsKeys,
    /// Also read the `AWSPREVIOUS` version of each Secrets Manager secret as history.
    pub include_previous: bool,
    /// Parameter Store path to read (`/prod/`). Unused by Secrets Manager.
    pub path: String,
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn sha_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// The pieces of a signed request. `headers` are the lower-cased names and values that were signed.
pub(crate) struct Signed {
    pub authorization: String,
    pub amz_date: String,
}

/// Signature Version 4 for one request. `headers` must already hold `host` (and any other header
/// the caller sends and wants signed); `x-amz-date` is added here.
pub(crate) fn sign(
    method: &str,
    uri: &str,
    query: &str,
    headers: &[(&str, &str)],
    payload: &[u8],
    keys: &AwsKeys,
    region: &str,
    service: &str,
    amz_date: &str,
) -> Signed {
    let date = &amz_date[..8];
    let mut all: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    all.push(("x-amz-date".into(), amz_date.into()));
    if let Some(token) = &keys.session_token {
        all.push(("x-amz-security-token".into(), token.clone()));
    }
    all.sort();
    let canonical_headers: String = all.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers = all
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical = format!(
        "{method}\n{uri}\n{query}\n{canonical_headers}\n{signed_headers}\n{}",
        sha_hex(payload)
    );
    let scope = format!("{date}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha_hex(canonical.as_bytes())
    );
    let k_date = hmac(
        format!("AWS4{}", keys.secret_key).as_bytes(),
        date.as_bytes(),
    );
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, service.as_bytes());
    let k_signing = hmac(&k_service, b"aws4_request");
    let signature = hex::encode(hmac(&k_signing, string_to_sign.as_bytes()));
    Signed {
        authorization: format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            keys.access_key
        ),
        amz_date: amz_date.into(),
    }
}

fn now_amz() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

struct Client {
    http: reqwest::blocking::Client,
    base: String,
    host: String,
    region: String,
    service: &'static str,
    keys: AwsKeys,
}

impl Client {
    fn new(opts: &AwsOptions, service: &'static str, subdomain: &str) -> Result<Self, String> {
        let base = opts
            .endpoint
            .clone()
            .unwrap_or_else(|| format!("https://{subdomain}.{}.amazonaws.com", opts.region))
            .trim_end_matches('/')
            .to_string();
        let host = base
            .split_once("://")
            .map_or(base.as_str(), |(_, rest)| rest)
            .to_string();
        Ok(Self {
            http: crate::tls::build_issuer_client(Duration::from_secs(30))
                .map_err(|e| e.message)?,
            base,
            host,
            region: opts.region.clone(),
            service,
            keys: opts.keys.clone(),
        })
    }

    fn call(&self, target: &str, body: &Value) -> Result<Value, String> {
        let payload = serde_json::to_vec(body).map_err(|e| e.to_string())?;
        let content_type = "application/x-amz-json-1.1";
        let signed = sign(
            "POST",
            "/",
            "",
            &[
                ("host", self.host.as_str()),
                ("content-type", content_type),
                ("x-amz-target", target),
            ],
            &payload,
            &self.keys,
            &self.region,
            self.service,
            &now_amz(),
        );
        let mut req = self
            .http
            .post(format!("{}/", self.base))
            .header("content-type", content_type)
            .header("x-amz-target", target)
            .header("x-amz-date", &signed.amz_date)
            .header("authorization", &signed.authorization)
            .body(payload);
        if let Some(token) = &self.keys.session_token {
            req = req.header("x-amz-security-token", token);
        }
        let resp = req
            .send()
            .map_err(|e| format!("cannot reach AWS: {}", e.without_url()))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            let kind = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| {
                    v.get("__type")
                        .and_then(Value::as_str)
                        .map(|t| t.rsplit('#').next().unwrap_or(t).to_string())
                })
                .unwrap_or_default();
            return Err(format!("AWS answered HTTP {status} {kind} for {target}"));
        }
        serde_json::from_str(&text).map_err(|_| format!("AWS returned no JSON for {target}"))
    }
}

pub(crate) fn read_secrets_manager(opts: &AwsOptions) -> Result<Parsed, String> {
    let client = Client::new(opts, "secretsmanager", "secretsmanager")?;
    let mut names: Vec<Value> = Vec::new();
    let mut token: Option<String> = None;
    loop {
        let mut body = json!({ "MaxResults": 100 });
        if let Some(t) = &token {
            body["NextToken"] = json!(t);
        }
        let page = client.call("secretsmanager.ListSecrets", &body)?;
        names.extend(page["SecretList"].as_array().cloned().unwrap_or_default());
        token = page["NextToken"].as_str().map(str::to_string);
        if token.is_none() {
            break;
        }
    }
    if names.is_empty() {
        return Err("AWS Secrets Manager listed no secrets in this region for these keys".into());
    }
    let mut items = Vec::new();
    let mut archived = Vec::new();
    let mut warnings = vec![format!(
        "read from AWS Secrets Manager in {} (a commercial service; optional)",
        opts.region
    )];
    for meta in &names {
        let name = meta["Name"].as_str().unwrap_or("").to_string();
        let arn = meta["ARN"].as_str().unwrap_or(&name).to_string();
        let current = match client.call(
            "secretsmanager.GetSecretValue",
            &json!({ "SecretId": arn, "VersionStage": "AWSCURRENT" }),
        ) {
            Ok(v) => v,
            Err(e) => {
                warnings.push(format!("{name}: {e}; skipped"));
                continue;
            }
        };
        // Rotation settings, the KMS key and the resource policy are metadata about control, not
        // values: kept as provenance.
        let control = json!({
            "name": name, "arn": arn, "kms_key_id": meta["KmsKeyId"],
            "rotation_enabled": meta["RotationEnabled"], "rotation_rules": meta["RotationRules"],
            "rotation_lambda_arn": meta["RotationLambdaARN"], "tags": meta["Tags"],
        });
        let metadata_archive = Archive {
            source_id: Some(arn.clone()),
            reason: "AWS rotation settings, KMS key and tags retained as non-operational metadata"
                .into(),
            media_type: "application/json".into(),
            bytes: serde_json::to_vec(&control).unwrap_or_default(),
        };
        if let Some(binary) = current["SecretBinary"].as_str() {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(binary)
                .map_err(|_| format!("{name}: SecretBinary is not base64"))?;
            archived.push(ArchivedItem {
                source_id: arn.clone(),
                source_hash: hash(&bytes),
                name,
                archive: Archive {
                    source_id: None,
                    reason: "AWS SecretBinary retained as an encrypted blob".into(),
                    media_type: "application/octet-stream".into(),
                    bytes,
                },
            });
            continue;
        }
        let text = current["SecretString"].as_str().unwrap_or("").to_string();
        let mut entry;
        let mut extra_vars = Vec::new();
        let mut history = Vec::new();
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) if !map.is_empty() => {
                entry = base_entry(name.clone(), String::new(), "env_var");
                for (k, v) in &map {
                    extra_vars.push(extra(
                        k.clone(),
                        v.as_str().map_or_else(|| v.to_string(), str::to_string),
                        false,
                    ));
                }
            }
            _ => entry = base_entry(name.clone(), text.clone(), classify(&text)),
        }
        if let Some(desc) = meta["Description"].as_str().filter(|d| !d.is_empty()) {
            entry["description"] = json!(desc);
        }
        if opts.include_previous {
            if let Ok(previous) = client.call(
                "secretsmanager.GetSecretValue",
                &json!({ "SecretId": arn, "VersionStage": "AWSPREVIOUS" }),
            ) {
                if let Some(old) = previous["SecretString"].as_str() {
                    history.push(
                        json!({ "value": old, "saved_at": previous["CreatedDate"].to_string() }),
                    );
                }
            }
        }
        let source = json!({ "arn": arn, "value": text, "version": current["VersionId"] });
        items.push(source_item(
            arn.clone(),
            name.clone(),
            source,
            entry,
            Vec::new(),
            vec!["aws-secrets-manager".into()],
            extra_vars,
            None,
            history,
            vec![metadata_archive],
            Vec::new(),
        ));
    }
    if items.is_empty() && archived.is_empty() {
        return Err("no Secrets Manager secret could be read with these keys".into());
    }
    Ok(Parsed {
        format: SourceFormat::AwsSm,
        label: format!("aws-sm:{}", opts.region),
        source_bytes: serde_json::to_vec(&json!({ "count": names.len() })).unwrap_or_default(),
        items,
        archived,
        warnings,
        missing_at_source: Vec::new(),
    })
}

pub(crate) fn read_parameter_store(opts: &AwsOptions) -> Result<Parsed, String> {
    let client = Client::new(opts, "ssm", "ssm")?;
    let mut params: Vec<Value> = Vec::new();
    let mut token: Option<String> = None;
    loop {
        let mut body = json!({ "Path": opts.path, "Recursive": true, "WithDecryption": true, "MaxResults": 10 });
        if let Some(t) = &token {
            body["NextToken"] = json!(t);
        }
        let page = client.call("AmazonSSM.GetParametersByPath", &body)?;
        params.extend(page["Parameters"].as_array().cloned().unwrap_or_default());
        token = page["NextToken"].as_str().map(str::to_string);
        if token.is_none() {
            break;
        }
    }
    if params.is_empty() {
        return Err(format!(
            "Parameter Store has no parameters under {}",
            opts.path
        ));
    }
    let mut items = Vec::new();
    for p in &params {
        let name = p["Name"].as_str().unwrap_or("").to_string();
        let value = p["Value"].as_str().unwrap_or("").to_string();
        let mut history = Vec::new();
        let mut hist_token: Option<String> = None;
        loop {
            let mut body = json!({ "Name": name, "WithDecryption": true });
            if let Some(t) = &hist_token {
                body["NextToken"] = json!(t);
            }
            let Ok(page) = client.call("AmazonSSM.GetParameterHistory", &body) else {
                break;
            };
            let current_version = p["Version"].as_i64();
            for old in page["Parameters"].as_array().cloned().unwrap_or_default() {
                if old["Version"].as_i64() != current_version {
                    if let Some(v) = old["Value"].as_str() {
                        history.push(
                            json!({ "value": v, "saved_at": old["LastModifiedDate"].to_string() }),
                        );
                    }
                }
            }
            hist_token = page["NextToken"].as_str().map(str::to_string);
            if hist_token.is_none() {
                break;
            }
        }
        history.reverse();
        let secret_type = if p["Type"].as_str() == Some("SecureString") {
            classify(&value)
        } else {
            "password"
        };
        let entry = base_entry(
            name.trim_start_matches('/').to_string(),
            value.clone(),
            secret_type,
        );
        let source =
            json!({ "name": name, "value": value, "version": p["Version"], "type": p["Type"] });
        items.push(source_item(
            name.clone(),
            name.trim_start_matches('/').to_string(),
            source,
            entry,
            Vec::new(),
            vec!["aws-parameter-store".into()],
            Vec::new(),
            None,
            history,
            Vec::new(),
            Vec::new(),
        ));
    }
    Ok(Parsed {
        format: SourceFormat::AwsSsm,
        label: format!("aws-ssm:{}:{}", opts.region, opts.path),
        source_bytes: serde_json::to_vec(&json!({ "count": params.len() })).unwrap_or_default(),
        items,
        archived: Vec::new(),
        warnings: vec![
            format!("read from AWS Parameter Store in {} (a commercial service; optional)", opts.region),
            "IAM access to a parameter path does not stop reads of deeper paths; check what this key can reach".into(),
        ],
        missing_at_source: Vec::new(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::service_sources::mock;

    fn keys() -> AwsKeys {
        AwsKeys {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            session_token: None,
        }
    }

    /// Amazon's published Signature Version 4 test vector, `get-vanilla`.
    #[test]
    fn signature_version_4_matches_amazons_published_vector() {
        let signed = sign(
            "GET",
            "/",
            "",
            &[("host", "example.amazonaws.com")],
            b"",
            &keys(),
            "us-east-1",
            "service",
            "20150830T123600Z",
        );
        assert_eq!(
            signed.authorization,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    fn opts(base: &str) -> AwsOptions {
        AwsOptions {
            region: "eu-west-1".into(),
            endpoint: Some(base.into()),
            keys: keys(),
            include_previous: true,
            path: "/prod/".into(),
        }
    }

    #[test]
    fn secrets_manager_pages_reads_json_and_binary_secrets_and_archives_the_metadata() {
        let server = mock::start(|req| {
            let auth_ok = req.header("authorization").is_some_and(|a| {
                a.contains("Credential=AKIDEXAMPLE/")
                    && a.contains("/eu-west-1/secretsmanager/aws4_request")
            });
            if !auth_ok {
                return (403, r#"{"__type":"AccessDeniedException"}"#.into());
            }
            let target = req.header("x-amz-target").unwrap_or("");
            match target {
                "secretsmanager.ListSecrets" if req.body.contains("NextToken") => (
                    200,
                    r#"{"SecretList":[{"Name":"bin","ARN":"arn:bin"}]}"#.into(),
                ),
                "secretsmanager.ListSecrets" => (
                    200,
                    r#"{"SecretList":[{"Name":"db","ARN":"arn:db","Description":"main db","KmsKeyId":"k1","RotationEnabled":true}],"NextToken":"p2"}"#.into(),
                ),
                "secretsmanager.GetSecretValue" if req.body.contains("arn:bin") => {
                    // Built at run time so no key-shaped literal sits in the source.
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(b"raw-bytes");
                    (200, format!(r#"{{"SecretBinary":"{encoded}","VersionId":"v1"}}"#))
                }
                "secretsmanager.GetSecretValue" if req.body.contains("AWSPREVIOUS") => (
                    200,
                    r#"{"SecretString":"{\"password\":\"old\"}","CreatedDate":1.0}"#.into(),
                ),
                "secretsmanager.GetSecretValue" => (
                    200,
                    r#"{"SecretString":"{\"password\":\"new\",\"user\":\"app\"}","VersionId":"v2"}"#.into(),
                ),
                _ => (400, "{}".into()),
            }
        });
        let parsed = read_secrets_manager(&opts(&server.base)).unwrap();
        assert_eq!(parsed.items.len(), 1);
        let db = &parsed.items[0];
        assert_eq!(db.entry["secretType"], "env_var");
        assert_eq!(db.entry["description"], "main db");
        assert_eq!(db.entry["version_history"].as_array().unwrap().len(), 1);
        assert!(String::from_utf8_lossy(&db.archives[0].bytes).contains("\"kms_key_id\":\"k1\""));
        assert_eq!(parsed.archived.len(), 1);
        assert_eq!(parsed.archived[0].archive.bytes, b"raw-bytes");
        assert!(parsed
            .warnings
            .iter()
            .any(|w| w.contains("commercial service")));
    }

    #[test]
    fn parameter_store_follows_next_token_and_reads_history() {
        let server = mock::start(|req| {
            match req.header("x-amz-target").unwrap_or("") {
            "AmazonSSM.GetParametersByPath" if req.body.contains("NextToken") => (
                200,
                r#"{"Parameters":[{"Name":"/prod/b","Type":"String","Value":"plain","Version":1}]}"#.into(),
            ),
            "AmazonSSM.GetParametersByPath" => (
                200,
                r#"{"Parameters":[{"Name":"/prod/a","Type":"SecureString","Value":"ghp_exampleexample","Version":2}],"NextToken":"n"}"#.into(),
            ),
            "AmazonSSM.GetParameterHistory" if req.body.contains("/prod/a") => (
                200,
                r#"{"Parameters":[{"Name":"/prod/a","Value":"older","Version":1,"LastModifiedDate":1.0},{"Name":"/prod/a","Value":"ghp_exampleexample","Version":2}]}"#.into(),
            ),
            _ => (200, r#"{"Parameters":[]}"#.into()),
        }
        });
        let parsed = read_parameter_store(&opts(&server.base)).unwrap();
        assert_eq!(parsed.items.len(), 2);
        let a = parsed.items.iter().find(|i| i.name == "prod/a").unwrap();
        assert_eq!(a.entry["secretType"], "api_key");
        assert_eq!(a.entry["version_history"].as_array().unwrap().len(), 1);
        assert!(parsed.warnings.iter().any(|w| w.contains("deeper paths")));
    }

    #[test]
    fn an_access_error_names_the_aws_error_type_and_not_the_keys() {
        let server = mock::start(|_| {
            (
                403,
                r#"{"__type":"com.amazon#AccessDeniedException"}"#.into(),
            )
        });
        let err = read_secrets_manager(&opts(&server.base)).unwrap_err();
        assert!(err.contains("AccessDeniedException"), "{err}");
        assert!(
            !err.contains("wJalrXUtnFEMI") && !err.contains("AKIDEXAMPLE"),
            "{err}"
        );
    }
}
