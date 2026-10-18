// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The signed release manifest behind `unv update` (Phase 54, roadmap R26).
//!
//! Same shape as the provider catalogue (an Ed25519 signature over the exact
//! payload bytes, then a shape check) but with its **own** key, so a leaked
//! catalogue key cannot ship a binary and the reverse. A manifest names one
//! version and, per platform, one archive with its SHA-256 and size. Nothing
//! here does I/O beyond reading the running executable's path.
//!
//! [`UPDATE_KEY_HEX`] is empty until the maintainer generates the update
//! key and pins its public half; with no key pinned every verify is refused,
//! so a build without one can never self-update by accident.

use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: u32 = 1;

/// Public half of the update-signing key (64 hex digits). Empty = not pinned.
/// Never reuse the catalogue key here.
pub const UPDATE_KEY_HEX: &str = "";

/// Where the release job publishes the signed CLI manifest.
pub const DEFAULT_URL: &str =
    "https://github.com/darthdemono/UnENVerse/releases/latest/download/unv-update.json";

const MAX_ASSETS: usize = 16;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// `linux-x86_64`, `windows-x86_64`, ...
    pub target: String,
    pub url: String,
    /// Lowercase hex SHA-256 of the archive.
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub published_at: String,
    #[serde(default)]
    pub notes: String,
    pub assets: Vec<Asset>,
}

#[derive(Serialize, Deserialize)]
struct Signed {
    payload: String,
    signature: String,
}

/// `x.y.z` as numbers; anything else (pre-release, build metadata) is refused
/// so a manifest cannot smuggle in an ordering this code does not understand.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.split('.');
    let mut n = || {
        let s = it.next()?;
        (!s.is_empty() && s.len() <= 9 && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u64>().ok())
            .flatten()
    };
    let t = (n()?, n()?, n()?);
    it.next().is_none().then_some(t)
}

/// The target string for the running binary, matching the release archive names.
pub fn current_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

pub fn pinned_key() -> Result<[u8; 32], String> {
    let mut key = [0u8; 32];
    if UPDATE_KEY_HEX.len() != 64 || hex::decode_to_slice(UPDATE_KEY_HEX, &mut key).is_err() {
        return Err(
            "this build has no update key pinned, so it cannot verify an update; \
             reinstall from the release page"
                .into(),
        );
    }
    Ok(key)
}

fn validate(m: &Manifest) -> Result<(), String> {
    if m.schema != SCHEMA {
        return Err(format!("unsupported manifest schema {}", m.schema));
    }
    if parse_version(&m.version).is_none() {
        return Err(format!(
            "`{}` is not a plain x.y.z version",
            m.version.escape_debug()
        ));
    }
    if m.published_at.len() != 20 || !m.published_at.ends_with('Z') {
        return Err("published_at must be YYYY-MM-DDTHH:MM:SSZ".into());
    }
    if m.notes.len() > 20_000 {
        return Err("release notes are over 20 kB".into());
    }
    if m.assets.is_empty() || m.assets.len() > MAX_ASSETS {
        return Err("asset count out of range".into());
    }
    for (i, a) in m.assets.iter().enumerate() {
        if m.assets[..i].iter().any(|b| b.target == a.target) {
            return Err(format!("duplicate target `{}`", a.target.escape_debug()));
        }
        let target_ok = !a.target.is_empty()
            && a.target.len() <= 32
            && a.target
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if !target_ok {
            return Err(format!("bad target `{}`", a.target.escape_debug()));
        }
        if !a.url.starts_with("https://") {
            return Err(format!("non-https URL for `{}`", a.target));
        }
        let hex_ok = a.sha256.len() == 64
            && a.sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex_ok {
            return Err(format!("bad sha256 for `{}`", a.target));
        }
        if a.size == 0 || a.size > MAX_ARCHIVE_BYTES {
            return Err(format!("archive size out of range for `{}`", a.target));
        }
    }
    Ok(())
}

/// Check signature, then shape. Does not compare versions.
pub fn verify(raw: &[u8], key: &[u8; 32]) -> Result<Manifest, String> {
    let signed: Signed =
        serde_json::from_slice(raw).map_err(|e| format!("not an update manifest: {e}"))?;
    let sig = base64::engine::general_purpose::STANDARD
        .decode(signed.signature.trim())
        .map_err(|_| "signature is not base64".to_string())?;
    let sig =
        Signature::from_slice(&sig).map_err(|_| "signature has the wrong length".to_string())?;
    let vk = VerifyingKey::from_bytes(key).map_err(|e| e.to_string())?;
    vk.verify(signed.payload.as_bytes(), &sig)
        .map_err(|_| "signature does not match the pinned update key".to_string())?;
    let m: Manifest = serde_json::from_str(&signed.payload).map_err(|e| e.to_string())?;
    validate(&m)?;
    Ok(m)
}

/// Publisher side: sign `m` with a 32-byte Ed25519 seed.
pub fn sign(m: &Manifest, seed: &[u8; 32]) -> Result<String, String> {
    validate(m)?;
    let payload = serde_json::to_string(m).map_err(|e| e.to_string())?;
    let sig = SigningKey::from_bytes(seed).sign(payload.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    serde_json::to_string_pretty(&Signed { payload, signature }).map_err(|e| e.to_string())
}

impl Manifest {
    /// Is this manifest strictly newer than `current`? A downgrade or the same
    /// version is never an update.
    pub fn is_newer_than(&self, current: &str) -> bool {
        match (parse_version(&self.version), parse_version(current)) {
            (Some(a), Some(b)) => a > b,
            _ => false,
        }
    }

    pub fn asset_for(&self, target: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.target == target)
    }
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// File the release archives ship beside the executable. Only a binary that
/// sits next to it was unpacked from a standalone archive and may replace itself.
pub const STANDALONE_MARKER: &str = ".unv-standalone";

#[derive(Debug, PartialEq, Eq)]
pub enum Origin {
    /// Unpacked from a release archive: may self-replace.
    Standalone,
    /// Owned by something else; the string is the command that updates it.
    Managed(&'static str),
    /// Not recognisably either: declined, with the remedy in the message.
    Unknown,
}

/// Classify an executable path. `marker_dir_has_marker` is whether
/// [`STANDALONE_MARKER`] exists beside it (passed in so this stays pure).
pub fn classify(exe: &Path, marker_dir_has_marker: bool) -> Origin {
    let p = exe.to_string_lossy().replace('\\', "/").to_lowercase();
    let managed: [(&str, &'static str); 9] = [
        (
            "/nix/store/",
            "nix profile upgrade, or update your Nix configuration",
        ),
        ("/homebrew/", "brew upgrade unv"),
        ("/linuxbrew/", "brew upgrade unv"),
        ("/scoop/", "scoop update unv"),
        ("/windowsapps/", "winget upgrade unv"),
        ("/winget/", "winget upgrade unv"),
        ("/.cargo/bin/", "cargo install unv-cli --locked --force"),
        ("/snap/", "sudo snap refresh unv"),
        (
            "/usr/",
            "your package manager (for example dnf upgrade unv or apt upgrade unv)",
        ),
    ];
    // A package manager's directory wins over the marker: a marker copied into
    // /usr/bin must not turn a dnf-owned file into one we overwrite.
    for (needle, cmd) in managed {
        if p.contains(needle) && !p.contains("/usr/local/") {
            return Origin::Managed(cmd);
        }
    }
    if marker_dir_has_marker {
        Origin::Standalone
    } else {
        Origin::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 32] = [5; 32];

    fn key() -> [u8; 32] {
        SigningKey::from_bytes(&SEED).verifying_key().to_bytes()
    }

    fn manifest() -> Manifest {
        Manifest {
            schema: SCHEMA,
            version: "0.55.0".into(),
            published_at: "2026-10-17T12:00:00Z".into(),
            notes: "notes".into(),
            assets: vec![Asset {
                target: "linux-x86_64".into(),
                url: "https://example.test/unv.tar.gz".into(),
                sha256: sha256_hex(b"x"),
                size: 1,
            }],
        }
    }

    #[test]
    fn accepts_a_signed_manifest() {
        let raw = sign(&manifest(), &SEED).unwrap();
        assert_eq!(verify(raw.as_bytes(), &key()).unwrap(), manifest());
    }

    #[test]
    fn refuses_a_wrong_key_and_a_tampered_payload() {
        let raw = sign(&manifest(), &SEED).unwrap();
        assert!(verify(raw.as_bytes(), &[9; 32]).is_err());
        let tampered = raw.replace("0.55.0", "9.9.9");
        assert!(verify(tampered.as_bytes(), &key()).is_err());
    }

    #[test]
    fn refuses_a_signed_but_malformed_manifest() {
        let mut m = manifest();
        m.assets[0].url = "http://example.test/a".into();
        assert!(sign(&m, &SEED).is_err());
        let mut m = manifest();
        m.version = "1.0.0-rc1".into();
        assert!(sign(&m, &SEED).is_err());
        let mut m = manifest();
        m.assets[0].sha256 = "ABC".into();
        assert!(sign(&m, &SEED).is_err());
        let mut m = manifest();
        m.assets.push(m.assets[0].clone());
        assert!(sign(&m, &SEED).is_err());
    }

    #[test]
    fn only_a_strictly_newer_version_is_an_update() {
        let m = manifest();
        assert!(m.is_newer_than("0.54.9"));
        assert!(!m.is_newer_than("0.55.0"));
        assert!(!m.is_newer_than("0.56.0"));
        assert!(m.is_newer_than("0.9.0"));
        assert!(!m.is_newer_than("garbage"));
    }

    #[test]
    fn no_pinned_key_refuses_everything() {
        assert!(pinned_key().is_err());
    }

    #[test]
    fn classifies_install_origins() {
        let p = |s: &str| Path::new(s).to_path_buf();
        assert_eq!(classify(&p("/opt/tools/unv"), true), Origin::Standalone);
        assert_eq!(classify(&p("/opt/tools/unv"), false), Origin::Unknown);
        assert!(matches!(
            classify(&p("/usr/bin/unv"), true),
            Origin::Managed(_)
        ));
        assert!(matches!(
            classify(&p("/home/a/.cargo/bin/unv"), false),
            Origin::Managed(_)
        ));
        assert!(matches!(
            classify(&p("C:\\Users\\a\\scoop\\apps\\unv\\current\\unv.exe"), true),
            Origin::Managed(_)
        ));
        assert_eq!(classify(&p("/usr/local/bin/unv"), true), Origin::Standalone);
    }
}
