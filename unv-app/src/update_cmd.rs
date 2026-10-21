// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv update` (Phase 54, roadmap R26): replace a standalone `unv` from a
//! signed release manifest.
//!
//! Trust order, each step refusing before the next runs: the install origin
//! must be a recognised standalone archive; the manifest must verify against
//! the pinned update key; the version must be strictly newer; the archive must
//! match the manifest's size and SHA-256 **before** anything is extracted; the
//! replacement keeps the old executable until the new one answers `--version`
//! with the manifest's version. Nothing here edits a running executable in
//! place: the old file is renamed away (allowed on Windows too) and the new one
//! renamed in. Package-manager and unknown origins are never overwritten.

use crate::error::{CliError, CliResult};
use crate::out;
use clap::Subcommand;
use serde_json::json;
use std::io::Read;
use std::path::{Path, PathBuf};
use vault_core::update as upd;

const MAX_MANIFEST: usize = 1024 * 1024;
const MAX_BINARY: u64 = 256 * 1024 * 1024;

#[derive(Subcommand)]
pub enum UpdateCmd {
    /// Say whether a newer signed release exists. Installs nothing.
    Check {
        #[arg(long, default_value = upd::DEFAULT_URL)]
        url: String,
        /// Read the manifest from disk instead (air-gapped machines).
        #[arg(long, conflicts_with = "url")]
        manifest: Option<PathBuf>,
    },
    /// Download, verify and install the newer release over this executable.
    ///
    /// Asks first; `--yes` skips the question. Only an executable unpacked from a
    /// standalone release archive is replaced.
    Install {
        #[arg(long, default_value = upd::DEFAULT_URL)]
        url: String,
        #[arg(long, conflicts_with = "url")]
        manifest: Option<PathBuf>,
        /// Use this archive instead of downloading it. Verified like a download.
        #[arg(long)]
        archive: Option<PathBuf>,
    },
    /// Publisher: build and sign the manifest for a release's archives.
    Sign {
        #[arg(long)]
        version: String,
        /// Release notes file (Markdown).
        #[arg(long)]
        notes_file: Option<PathBuf>,
        /// `TARGET=ARCHIVE_PATH`, repeatable (for example linux-x86_64=dist/unv-1.2.3-linux-x86_64.tar.gz).
        /// The URL is `--base-url` plus the archive's file name.
        #[arg(long = "asset", required = true)]
        assets: Vec<String>,
        #[arg(long)]
        base_url: String,
        /// File holding the 32-byte seed as 64 hex digits.
        #[arg(long)]
        key_file: PathBuf,
        #[arg(long, short = 'o')]
        out: PathBuf,
    },
}

fn now_utc() -> String {
    let t = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

fn invalid(e: impl ToString) -> CliError {
    CliError::invalid(e.to_string())
}

fn fetch(url: &str, max: u64) -> CliResult<Vec<u8>> {
    if !url.starts_with("https://") {
        return Err(invalid("updates are only fetched over https"));
    }
    let client = crate::tls::build_public_client(std::time::Duration::from_secs(60), "unv-update")?;
    let resp = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| {
            CliError::unavailable(format!(
                "Cannot fetch {url}: {e}. Check the network or proxy, or use --manifest/--archive files."
            ))
        })?;
    let mut buf = Vec::new();
    resp.take(max + 1)
        .read_to_end(&mut buf)
        .map_err(|e| CliError::unavailable(format!("Cannot read {url}: {e}")))?;
    if buf.len() as u64 > max {
        return Err(invalid(format!(
            "{url} is over the {max}-byte limit; refusing"
        )));
    }
    Ok(buf)
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "unv.exe"
    } else {
        "unv"
    }
}

/// Pull the executable out of a `.tar.gz` or `.zip`, matching the exact file
/// name (a leading `./` is fine, any other directory is not), size-capped so a
/// small archive cannot expand without bound.
pub fn extract_binary(archive: &[u8], url: &str, name: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    if url.ends_with(".zip") {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(archive))
            .map_err(|e| format!("not a zip archive: {e}"))?;
        let mut f = z
            .by_name(name)
            .map_err(|_| format!("the archive holds no `{name}`"))?;
        f.by_ref()
            .take(MAX_BINARY + 1)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
    } else if url.ends_with(".tar.gz") {
        let gz = flate2::read::GzDecoder::new(archive);
        let mut tar = tar::Archive::new(gz);
        let mut found = false;
        for e in tar
            .entries()
            .map_err(|e| format!("not a tar.gz archive: {e}"))?
        {
            let mut e = e.map_err(|e| e.to_string())?;
            let p = e.path().map_err(|e| e.to_string())?.into_owned();
            let p = p.strip_prefix("./").unwrap_or(&p);
            if e.header().entry_type().is_file() && p == Path::new(name) {
                e.by_ref()
                    .take(MAX_BINARY + 1)
                    .read_to_end(&mut out)
                    .map_err(|e| e.to_string())?;
                found = true;
                break;
            }
        }
        if !found {
            return Err(format!("the archive holds no `{name}`"));
        }
    } else {
        return Err("the archive must be .tar.gz or .zip".into());
    }
    if out.len() as u64 > MAX_BINARY {
        return Err("the executable in the archive is over 256 MiB; refusing".into());
    }
    if out.is_empty() {
        return Err("the executable in the archive is empty".into());
    }
    Ok(out)
}

/// Swap `new` in for `exe`. The old file is renamed to `<exe>.rollback` first and
/// put back if the rename-in or `probe` fails, so the previous executable is
/// usable after every failure. On success the rollback is removed (Windows
/// cannot delete a running executable, so it may stay until the next run).
pub fn replace_executable(
    exe: &Path,
    new: &Path,
    probe: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let mut rb = exe.as_os_str().to_owned();
    rb.push(".rollback");
    let rb = PathBuf::from(rb);
    let _ = std::fs::remove_file(&rb);
    std::fs::rename(exe, &rb).map_err(|e| format!("cannot move the old executable aside: {e}"))?;
    if let Err(e) = std::fs::rename(new, exe) {
        let _ = std::fs::rename(&rb, exe);
        return Err(format!("cannot move the new executable into place: {e}"));
    }
    if let Err(e) = probe(exe) {
        let _ = std::fs::remove_file(exe);
        let _ = std::fs::rename(&rb, exe);
        return Err(format!(
            "the new executable failed its version check ({e}); the previous one was restored"
        ));
    }
    let _ = std::fs::remove_file(&rb);
    Ok(())
}

fn probe_version(expect: &str) -> impl FnOnce(&Path) -> Result<(), String> + '_ {
    move |p| {
        let o = std::process::Command::new(p)
            .arg("--version")
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&o.stdout);
        if o.status.success() && text.split_whitespace().any(|w| w == expect) {
            Ok(())
        } else {
            Err(format!("expected version {expect}, got `{}`", text.trim()))
        }
    }
}

/// Verify `archive` against the manifest asset, extract and install it over
/// `exe`. Split from [`run_with`] so tests drive it with fixtures.
pub fn install(
    m: &upd::Manifest,
    target: &str,
    archive: &[u8],
    exe: &Path,
    probe: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let asset = m
        .asset_for(target)
        .ok_or_else(|| format!("release {} has no build for {target}", m.version))?;
    if archive.len() as u64 != asset.size {
        return Err(format!(
            "archive is {} bytes, the signed manifest says {}",
            archive.len(),
            asset.size
        ));
    }
    if upd::sha256_hex(archive) != asset.sha256 {
        return Err("archive checksum does not match the signed manifest".into());
    }
    let bin = extract_binary(archive, &asset.url, binary_name())?;
    let dir = exe.parent().ok_or("executable has no directory")?;
    let staged = dir.join(format!(".unv-update-{}", std::process::id()));
    std::fs::write(&staged, &bin).map_err(|e| format!("cannot stage the update: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    }
    replace_executable(exe, &staged, probe).inspect_err(|_| {
        let _ = std::fs::remove_file(&staged);
    })
}

fn load_manifest(
    url: &str,
    file: &Option<PathBuf>,
    key: &Result<[u8; 32], String>,
) -> CliResult<upd::Manifest> {
    let key = key.as_ref().map_err(|e| CliError::denied(e.clone()))?;
    let raw = match file {
        Some(p) => std::fs::read(p)
            .map_err(|e| CliError::from(format!("Cannot read {}: {e}", p.display())))?,
        None => fetch(url, MAX_MANIFEST as u64)?,
    };
    upd::verify(&raw, key).map_err(invalid)
}

fn check_origin(exe: &Path) -> CliResult {
    let marker = exe
        .parent()
        .is_some_and(|d| d.join(upd::STANDALONE_MARKER).is_file());
    match upd::classify(exe, marker) {
        upd::Origin::Standalone => Ok(()),
        upd::Origin::Managed(cmd) => Err(CliError::denied(format!(
            "{} is managed by a package manager and is not replaced here. Update it with: {cmd}",
            exe.display()
        ))),
        upd::Origin::Unknown => Err(CliError::denied(format!(
            "{} was not unpacked from a standalone release archive (no {} beside it), so it is \
             not replaced. Install the archive from the release page, or update it the way you installed it.",
            exe.display(),
            upd::STANDALONE_MARKER
        ))),
    }
}

pub fn run(cmd: &UpdateCmd, yes: bool) -> CliResult {
    // nosemgrep: rust.lang.security.current-exe.current-exe (only an executable beside .unv-standalone is replaced, and the new archive is verified before the swap)
    let exe = std::env::current_exe()
        .map_err(|e| CliError::from(format!("Cannot locate this executable: {e}")))?;
    run_with(cmd, yes, upd::pinned_key(), &exe, env!("CARGO_PKG_VERSION"))
}

pub fn run_with(
    cmd: &UpdateCmd,
    yes: bool,
    key: Result<[u8; 32], String>,
    exe: &Path,
    current: &str,
) -> CliResult {
    let target = upd::current_target();
    match cmd {
        UpdateCmd::Check { url, manifest } => {
            let m = load_manifest(url, manifest, &key)?;
            let newer = m.is_newer_than(current);
            out::ok(
                "update.check",
                json!({ "current": current, "latest": m.version, "update_available": newer,
                        "published_at": m.published_at, "notes": m.notes,
                        "has_build_for_this_platform": m.asset_for(&target).is_some() }),
                || {
                    if newer {
                        println!(
                            "Update available: {current} -> {}\n\n{}",
                            m.version, m.notes
                        );
                    } else {
                        println!(
                            "unv {current} is up to date (latest signed release: {}).",
                            m.version
                        );
                    }
                },
            );
            Ok(())
        }
        UpdateCmd::Sign {
            version,
            notes_file,
            assets,
            base_url,
            key_file,
            out: out_path,
        } => {
            let mut list = Vec::new();
            for a in assets {
                let (target, path) = a
                    .split_once('=')
                    .ok_or_else(|| invalid("--asset must be TARGET=ARCHIVE_PATH"))?;
                let path = Path::new(path);
                let bytes = std::fs::read(path)
                    .map_err(|e| CliError::from(format!("Cannot read {}: {e}", path.display())))?;
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| invalid("archive path has no file name"))?;
                list.push(upd::Asset {
                    target: target.into(),
                    url: format!("{}/{name}", base_url.trim_end_matches('/')),
                    sha256: upd::sha256_hex(&bytes),
                    size: bytes.len() as u64,
                });
            }
            let notes = match notes_file {
                Some(p) => std::fs::read_to_string(p)
                    .map_err(|e| CliError::from(format!("Cannot read {}: {e}", p.display())))?,
                None => String::new(),
            };
            let m = upd::Manifest {
                schema: upd::SCHEMA,
                version: version.clone(),
                published_at: now_utc(),
                notes,
                assets: list,
            };
            let mut seed = [0u8; 32];
            let hexed = std::fs::read_to_string(key_file)
                .map_err(|e| CliError::from(format!("Cannot read key file: {e}")))?;
            hex::decode_to_slice(hexed.trim(), &mut seed)
                .map_err(|_| invalid("key file must hold 64 hex digits"))?;
            let signed = upd::sign(&m, &seed).map_err(invalid)?;
            std::fs::write(out_path, signed)
                .map_err(|e| CliError::from(format!("Cannot write {}: {e}", out_path.display())))?;
            out::ok(
                "update.sign",
                json!({ "written": out_path.display().to_string(), "version": version }),
                || println!("Wrote {}", out_path.display()),
            );
            Ok(())
        }
        UpdateCmd::Install {
            url,
            manifest,
            archive,
        } => {
            check_origin(exe)?;
            let m = load_manifest(url, manifest, &key)?;
            if !m.is_newer_than(current) {
                out::ok(
                    "update.install",
                    json!({ "current": current, "latest": m.version, "updated": false }),
                    || {
                        println!(
                            "unv {current} is already the newest signed release ({}).",
                            m.version
                        )
                    },
                );
                return Ok(());
            }
            let asset = m.asset_for(&target).ok_or_else(|| {
                CliError::not_found(format!("release {} has no build for {target}", m.version))
            })?;
            if !crate::fmt::confirm(
                &format!(
                    "Replace unv {current} with {} at {}?",
                    m.version,
                    exe.display()
                ),
                yes,
            )? {
                return Err(CliError::new(
                    crate::error::Code::NeedsConfirmation,
                    "Cancelled.",
                ));
            }
            let bytes = match archive {
                Some(p) => std::fs::read(p)
                    .map_err(|e| CliError::from(format!("Cannot read {}: {e}", p.display())))?,
                None => fetch(&asset.url, asset.size)?,
            };
            install(&m, &target, &bytes, exe, probe_version(&m.version)).map_err(invalid)?;
            out::ok(
                "update.install",
                json!({ "current": current, "latest": m.version, "updated": true }),
                || println!("Updated unv {current} -> {}.", m.version),
            );
            Ok(())
        }
    }
}
