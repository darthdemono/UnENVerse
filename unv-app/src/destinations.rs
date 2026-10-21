// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Recovery backup transports.
//!
//! The recovery operation deliberately owns the package format, completion
//! manifest and retention policy.  A destination only moves named opaque files.
//! That separation keeps local disks and SFTP subject to one completion protocol
//! and gives every new destination a small, testable conformance surface.

use russh::client;
use russh::keys::{self, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A name and metadata returned by a destination listing.  Names are always
/// basenames: callers cannot accidentally delete an arbitrary destination path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteObject {
    pub name: String,
    pub size: Option<u64>,
}

/// The shared contract used by recovery package and drill operations.
///
/// `put` must create the named object and fail if it already exists.  Recovery
/// always uploads a randomly named `.part` file, then calls
/// [`BackupDestination::rename_no_replace`] after it is durable.  The latter is
/// necessarily best effort on baseline SFTP because it has no no-overwrite
/// rename extension; random UUID final names make a collision impracticable.
pub trait BackupDestination {
    fn probe(&mut self) -> Result<(), String>;
    fn put(&mut self, source: &Path, name: &str) -> Result<(), String>;
    fn list(&mut self) -> Result<Vec<RemoteObject>, String>;
    fn get(&mut self, name: &str, output: &Path) -> Result<(), String>;
    fn delete(&mut self, name: &str) -> Result<(), String>;
    fn rename_no_replace(&mut self, from: &str, to: &str) -> Result<(), String>;
}

fn safe_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.contains("..")
    {
        return Err("Destination object name must be a simple filename".to_string());
    }
    Ok(())
}

/// A filesystem destination.  It is intentionally plain and synchronous: the
/// same six-method contract is used by the async SFTP implementation below.
#[derive(Debug, Clone)]
pub struct LocalDestination {
    root: PathBuf,
}

impl LocalDestination {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, name: &str) -> Result<PathBuf, String> {
        safe_name(name)?;
        Ok(self.root.join(name))
    }
}

impl BackupDestination for LocalDestination {
    fn probe(&mut self) -> Result<(), String> {
        let metadata = fs::metadata(&self.root).map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            Ok(())
        } else {
            Err(format!("{} is not a directory", self.root.display()))
        }
    }

    fn put(&mut self, source: &Path, name: &str) -> Result<(), String> {
        let destination = self.path(name)?;
        let mut input = File::open(source).map_err(|e| e.to_string())?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(destination)
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())
    }

    fn list(&mut self) -> Result<Vec<RemoteObject>, String> {
        Ok(fs::read_dir(&self.root)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry.file_type().ok().filter(|kind| kind.is_file())?;
                let size = entry.metadata().ok().map(|m| m.len());
                Some(RemoteObject {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    size,
                })
            })
            .collect::<Vec<_>>())
    }

    fn get(&mut self, name: &str, output: &Path) -> Result<(), String> {
        let source = self.path(name)?;
        let mut input = File::open(source).map_err(|e| e.to_string())?;
        let mut out = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output)
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut out).map_err(|e| e.to_string())?;
        out.sync_all().map_err(|e| e.to_string())
    }

    fn delete(&mut self, name: &str) -> Result<(), String> {
        fs::remove_file(self.path(name)?).map_err(|e| e.to_string())
    }

    fn rename_no_replace(&mut self, from: &str, to: &str) -> Result<(), String> {
        let from = self.path(from)?;
        let to = self.path(to)?;
        if to.exists() {
            return Err(format!("Destination object `{to:?}` already exists"));
        }
        fs::rename(from, to).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// The authentication material is obtained from a vault entry immediately
/// before connecting.  It is never serialized into a destination record.
#[derive(Debug, Clone)]
pub enum SftpCredential {
    Password(String),
    PrivateKey(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SftpAuthMethod {
    Password,
    PrivateKey,
}

/// Persistable, non-secret SFTP destination configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SftpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub remote_path: String,
    pub credential_entry: String,
    pub auth: SftpAuthMethod,
    /// OpenSSH SHA-256 fingerprint, accepted explicitly when the destination
    /// was created.  A different key is always rejected.
    pub host_key_fingerprint: String,
}

impl SftpConfig {
    fn address(&self) -> (&str, u16) {
        (&self.host, self.port)
    }

    fn remote_path(&self, name: &str) -> Result<String, String> {
        safe_name(name)?;
        let root = self.remote_path.trim_end_matches('/');
        if root.is_empty() || root == "." {
            Ok(name.to_string())
        } else {
            Ok(format!("{root}/{name}"))
        }
    }
}

#[derive(Debug)]
struct HostKeyHandler {
    expected: Option<String>,
    presented: Arc<Mutex<Option<String>>>,
}

impl client::Handler for HostKeyHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = server_public_key
            .public_key()
            .fingerprint(HashAlg::Sha256)
            .to_string();
        if let Ok(mut presented) = self.presented.lock() {
            *presented = Some(fingerprint.clone());
        }
        Ok(self
            .expected
            .as_deref()
            .is_none_or(|expected| expected == fingerprint))
    }
}

fn runtime<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    std::thread::spawn(work)
        .join()
        .map_err(|_| "SFTP worker panicked".to_string())?
}

fn async_runtime<T: Send + 'static>(
    work: impl std::future::Future<Output = Result<T, String>> + Send + 'static,
) -> Result<T, String> {
    runtime(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?
            .block_on(work)
    })
}

async fn connect_sftp(
    config: SftpConfig,
    credential: Option<SftpCredential>,
    allow_unpinned: bool,
) -> Result<
    (
        client::Handle<HostKeyHandler>,
        Option<SftpSession>,
        Arc<Mutex<Option<String>>>,
    ),
    String,
> {
    let presented = Arc::new(Mutex::new(None));
    let handler = HostKeyHandler {
        expected: if allow_unpinned {
            None
        } else {
            Some(config.host_key_fingerprint.clone())
        },
        presented: Arc::clone(&presented),
    };
    let mut session = client::connect(
        Arc::new(client::Config::default()),
        config.address(),
        handler,
    )
    .await
    .map_err(|error| host_key_error(error, &config, &presented))?;

    let Some(credential) = credential else {
        return Ok((session, None, presented));
    };
    {
        let authenticated = match credential {
            SftpCredential::Password(password) => session
                .authenticate_password(config.username.clone(), password)
                .await
                .map_err(|e| e.to_string())?
                .success(),
            SftpCredential::PrivateKey(pem) => {
                let key = keys::decode_secret_key(&pem, None).map_err(|_| {
                    "Could not decode SFTP private key from the vault entry".to_string()
                })?;
                let hash = session
                    .best_supported_rsa_hash()
                    .await
                    .map_err(|e| e.to_string())?
                    .flatten();
                session
                    .authenticate_publickey(
                        config.username.clone(),
                        PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                    )
                    .await
                    .map_err(|e| e.to_string())?
                    .success()
            }
        };
        if !authenticated {
            return Err("SFTP authentication was refused".to_string());
        }
    }
    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| e.to_string())?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| e.to_string())?;
    Ok((session, Some(sftp), presented))
}

fn host_key_error(
    error: impl std::fmt::Display,
    config: &SftpConfig,
    presented: &Arc<Mutex<Option<String>>>,
) -> String {
    let actual = presented.lock().ok().and_then(|v| v.clone());
    match actual {
        Some(actual) if actual != config.host_key_fingerprint => format!(
            "SFTP host key changed for {}:{}; pinned {}, presented {}",
            config.host, config.port, config.host_key_fingerprint, actual
        ),
        _ => format!(
            "SFTP connection to {}:{} failed: {error}",
            config.host, config.port
        ),
    }
}

/// Reads the unauthenticated server key for an explicit user confirmation.
/// This function never persists a fingerprint and must not be used for normal
/// package activity.
pub fn probe_sftp_host_key(host: &str, port: u16) -> Result<String, String> {
    let config = SftpConfig {
        host: host.to_string(),
        port,
        username: String::new(),
        remote_path: String::new(),
        credential_entry: String::new(),
        auth: SftpAuthMethod::Password,
        host_key_fingerprint: String::new(),
    };
    async_runtime(async move {
        let (_session, _sftp, presented) = connect_sftp(config, None, true).await?;
        let fingerprint = presented
            .lock()
            .map_err(|_| "SFTP host key lock was poisoned".to_string())?
            .clone()
            .ok_or_else(|| "SFTP server did not present a host key".to_string());
        fingerprint
    })
}

/// A pinned, vault-credential-backed SFTP destination.
#[derive(Debug, Clone)]
pub struct SftpDestination {
    config: SftpConfig,
    credential: SftpCredential,
}

impl SftpDestination {
    pub fn new(config: SftpConfig, credential: SftpCredential) -> Self {
        Self { config, credential }
    }

    fn with_session<T: Send + 'static>(
        &self,
        work: impl FnOnce(
                SftpSession,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>
            + Send
            + 'static,
    ) -> Result<T, String> {
        let config = self.config.clone();
        let credential = self.credential.clone();
        async_runtime(async move {
            let (_session, sftp, _presented) =
                connect_sftp(config, Some(credential), false).await?;
            let sftp = sftp.ok_or_else(|| "SFTP session was not opened".to_string())?;
            work(sftp).await
        })
    }
}

impl BackupDestination for SftpDestination {
    fn probe(&mut self) -> Result<(), String> {
        let root = self.config.remote_path.clone();
        self.with_session(move |sftp| {
            Box::pin(async move {
                let metadata = sftp.metadata(root).await.map_err(|e| e.to_string())?;
                if metadata.is_dir() {
                    Ok(())
                } else {
                    Err("SFTP destination path is not a directory".to_string())
                }
            })
        })
    }

    fn put(&mut self, source: &Path, name: &str) -> Result<(), String> {
        let source = source.to_path_buf();
        let remote = self.config.remote_path(name)?;
        self.with_session(move |sftp| {
            Box::pin(async move {
                let mut input = tokio::fs::File::open(source)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut output = sftp
                    .open_with_flags(
                        remote,
                        OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                tokio::io::copy(&mut input, &mut output)
                    .await
                    .map_err(|e| e.to_string())?;
                output.sync_all().await.map_err(|e| e.to_string())?;
                output.close().await.map_err(|e| e.to_string())
            })
        })
    }

    fn list(&mut self) -> Result<Vec<RemoteObject>, String> {
        let root = self.config.remote_path.clone();
        self.with_session(move |sftp| {
            Box::pin(async move {
                let entries = sftp.read_dir(root).await.map_err(|e| e.to_string())?;
                Ok(entries
                    .map(|entry| RemoteObject {
                        name: entry.file_name(),
                        size: entry.metadata().size,
                    })
                    .collect())
            })
        })
    }

    fn get(&mut self, name: &str, output: &Path) -> Result<(), String> {
        let remote = self.config.remote_path(name)?;
        let output = output.to_path_buf();
        self.with_session(move |sftp| {
            Box::pin(async move {
                let mut input = sftp.open(remote).await.map_err(|e| e.to_string())?;
                let mut out = tokio::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(output)
                    .await
                    .map_err(|e| e.to_string())?;
                tokio::io::copy(&mut input, &mut out)
                    .await
                    .map_err(|e| e.to_string())?;
                out.sync_all().await.map_err(|e| e.to_string())
            })
        })
    }

    fn delete(&mut self, name: &str) -> Result<(), String> {
        let remote = self.config.remote_path(name)?;
        self.with_session(move |sftp| {
            Box::pin(async move { sftp.remove_file(remote).await.map_err(|e| e.to_string()) })
        })
    }

    fn rename_no_replace(&mut self, from: &str, to: &str) -> Result<(), String> {
        let from = self.config.remote_path(from)?;
        let to = self.config.remote_path(to)?;
        self.with_session(move |sftp| {
            Box::pin(async move {
                if sftp
                    .try_exists(to.clone())
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Err("SFTP destination already has this package name".to_string());
                }
                sftp.rename(from, to).await.map_err(|e| e.to_string())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    fn scratch() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("unenverse-destination-{}", vault_core::new_uuid()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn local_destination_conforms_to_the_shared_file_contract() {
        let root = scratch();
        let source = root.join("source");
        let output = root.join("output");
        fs::write(&source, b"package bytes").unwrap();
        let mut destination = LocalDestination::new(&root);

        destination.probe().unwrap();
        destination.put(&source, "run.part").unwrap();
        assert!(destination.put(&source, "run.part").is_err());
        destination
            .rename_no_replace("run.part", "run.unvpkg")
            .unwrap();
        // The scratch root also holds `source`, and read_dir order is unspecified: asserting
        // on `list()[0]` failed on CI whenever the filesystem returned `source` first.
        let names: Vec<String> = destination
            .list()
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(names.contains(&"run.unvpkg".to_string()), "{names:?}");
        destination.get("run.unvpkg", &output).unwrap();
        assert_eq!(fs::read(output).unwrap(), b"package bytes");
        destination.delete("run.unvpkg").unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn destination_names_cannot_escape_the_destination_root() {
        assert!(safe_name("../outside").is_err());
        assert!(safe_name("folder/file").is_err());
    }

    #[test]
    fn docker_sftp_conforms_and_refuses_a_changed_host_key() {
        // This deliberately uses a real OpenSSH SFTP server, but is opt-in so
        // ordinary unit-test runs do not pull an image or need Docker. CI can
        // enable UNV_SFTP_CONTAINER_TEST=1 where Docker is available.
        let docker_ready = Command::new("docker")
            .args(["info", "--format", "{{.ServerVersion}}"])
            .output()
            .is_ok_and(|output| output.status.success());
        if std::env::var("UNV_SFTP_CONTAINER_TEST").as_deref() != Ok("1") || !docker_ready {
            return;
        }
        let name = format!("unenverse-sftp-{}", vault_core::new_uuid());
        let started = Command::new("docker")
            .args([
                "run",
                "--detach",
                "--rm",
                "--name",
                &name,
                "--publish",
                "127.0.0.1::22",
                "atmoz/sftp:latest",
                "unv:fixture-password:1001:1001:upload",
            ])
            .output()
            .unwrap();
        assert!(started.status.success(), "could not start atmoz/sftp");
        let cleanup = || {
            let _ = Command::new("docker")
                .args(["rm", "--force", &name])
                .status();
        };
        let result = (|| -> Result<(), String> {
            let port = (0..40)
                .find_map(|attempt| {
                    if attempt != 0 {
                        thread::sleep(Duration::from_millis(250));
                    }
                    let output = Command::new("docker")
                        .args(["port", &name, "22/tcp"])
                        .output()
                        .ok()?;
                    if !output.status.success() {
                        return None;
                    }
                    let line = String::from_utf8_lossy(&output.stdout);
                    let port = line.rsplit(':').next()?.trim().parse::<u16>().ok()?;
                    probe_sftp_host_key("127.0.0.1", port)
                        .ok()
                        .map(|key| (port, key))
                })
                .ok_or_else(|| "SFTP container did not become ready".to_string())?;
            let config = SftpConfig {
                host: "127.0.0.1".to_string(),
                port: port.0,
                username: "unv".to_string(),
                remote_path: "upload".to_string(),
                credential_entry: "fixture".to_string(),
                auth: SftpAuthMethod::Password,
                host_key_fingerprint: port.1.clone(),
            };
            let root = scratch();
            let source = root.join("source");
            let output = root.join("output");
            fs::write(&source, b"SFTP fixture bytes").map_err(|e| e.to_string())?;
            let mut destination = SftpDestination::new(
                config.clone(),
                SftpCredential::Password("fixture-password".into()),
            );
            destination.probe()?;
            destination.put(&source, "fixture.part")?;
            destination.rename_no_replace("fixture.part", "fixture.unvpkg")?;
            if !destination
                .list()?
                .iter()
                .any(|object| object.name == "fixture.unvpkg")
            {
                return Err("SFTP list did not return transferred package".to_string());
            }
            destination.get("fixture.unvpkg", &output)?;
            if fs::read(&output).map_err(|e| e.to_string())? != b"SFTP fixture bytes" {
                return Err("SFTP read-back differed".to_string());
            }
            destination.delete("fixture.unvpkg")?;
            let mut changed = config;
            changed.host_key_fingerprint = "SHA256:not-the-pinned-key".to_string();
            let error =
                SftpDestination::new(changed, SftpCredential::Password("fixture-password".into()))
                    .probe()
                    .unwrap_err();
            if !error.contains("pinned SHA256:not-the-pinned-key") || !error.contains("presented") {
                return Err(format!("host-key mismatch did not name both pins: {error}"));
            }
            let _ = fs::remove_dir_all(root);
            Ok(())
        })();
        cleanup();
        if let Err(error) = result {
            panic!("SFTP container conformance failed: {error}");
        }
    }
}
