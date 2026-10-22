// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Small platform differences that the rest of the code should not have to know about.
//!
//! - [`replace_file`]: the "write a temp file, rename it over the target" step. On Windows a rename
//!   over a file another process has open (an antivirus scan, a backup agent, a reader) fails with a
//!   sharing violation that clears within moments, so it is retried a bounded number of times.
//! - [`network_fs`]: whether a path is on a network filesystem. SQLite's WAL mode needs shared
//!   memory and locks that NFS and SMB do not provide reliably, so a vault there can corrupt;
//!   `unv doctor` says so instead of letting it happen quietly.

use std::io;
use std::path::Path;
use std::time::Duration;

const ATTEMPTS: u32 = 8;
const PAUSE: Duration = Duration::from_millis(40);

/// `ERROR_ACCESS_DENIED`, `ERROR_SHARING_VIOLATION` and `ERROR_LOCK_VIOLATION`: what Windows
/// reports while another handle blocks the rename.
fn transient_windows(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

/// Runs `op`, repeating up to [`ATTEMPTS`] times while `transient` says the error will pass.
pub fn retry<T>(
    mut op: impl FnMut() -> io::Result<T>,
    transient: impl Fn(&io::Error) -> bool,
    pause: Duration,
) -> io::Result<T> {
    let mut attempt = 1;
    loop {
        match op() {
            Err(e) if attempt < ATTEMPTS && transient(&e) => {
                attempt += 1;
                std::thread::sleep(pause);
            }
            other => return other,
        }
    }
}

/// Renames `from` over `to`. On Windows a sharing violation is retried briefly; elsewhere this is a
/// plain rename, which is already atomic.
pub fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    let transient: fn(&io::Error) -> bool = if cfg!(windows) {
        transient_windows
    } else {
        |_| false
    };
    retry(|| std::fs::rename(from, to), transient, PAUSE)
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
const NETWORK_TYPES: &[&str] = &[
    "nfs",
    "nfs4",
    "cifs",
    "smb",
    "smb2",
    "smb3",
    "smbfs",
    "afpfs",
    "webdav",
    "9p",
    "ceph",
    "glusterfs",
    "lustre",
    "fuse.sshfs",
    "fuse.rclone",
    "fuse.s3fs",
    "fuse.gcsfuse",
];

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn is_network_type(fstype: &str) -> bool {
    NETWORK_TYPES.contains(&fstype.to_ascii_lowercase().as_str())
}

/// The filesystem type of the longest mount point that contains `path`, from `/proc/mounts` text.
pub fn parse_proc_mounts(text: &str, path: &Path) -> Option<String> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let (_dev, mount, fstype) = (f.next()?, f.next()?, f.next()?);
            // /proc/mounts escapes a space in a path as \040.
            let mount = mount.replace("\\040", " ");
            path.starts_with(&mount)
                .then(|| (mount.len(), fstype.to_string()))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, t)| t)
}

/// The same for the output of `mount` on macOS: `host:/export on /Volumes/x (nfs, nodev, ...)`.
pub fn parse_mount_output(text: &str, path: &Path) -> Option<String> {
    text.lines()
        .filter_map(|l| {
            let (_, rest) = l.split_once(" on ")?;
            let (mount, opts) = rest.rsplit_once(" (")?;
            let fstype = opts.split([',', ')']).next()?.trim();
            path.starts_with(mount)
                .then(|| (mount.len(), fstype.to_string()))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, t)| t)
}

/// A Windows UNC path (`\\server\share\...`), which is always a network location.
pub fn is_unc(path: &Path) -> bool {
    let s = path.to_string_lossy();
    (s.starts_with("\\\\") && !s.starts_with("\\\\?\\") && !s.starts_with("\\\\.\\"))
        || s.starts_with("\\\\?\\UNC\\")
}

/// The name of the network filesystem `path` lives on, or `None` when it is local or unknown.
pub fn network_fs(path: &Path) -> Option<String> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if is_unc(&path) {
        return Some("a network share".into());
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/mounts").ok()?;
        return parse_proc_mounts(&text, &path).filter(|t| is_network_type(t));
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/sbin/mount").output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        return parse_mount_output(&text, &path).filter(|t| is_network_type(t));
    }
    #[allow(unreachable_code)]
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_transient_error_is_retried_until_it_passes_and_then_stops() {
        let calls = Cell::new(0);
        let r = retry(
            || {
                calls.set(calls.get() + 1);
                if calls.get() < 4 {
                    Err(io::Error::from_raw_os_error(32))
                } else {
                    Ok("renamed")
                }
            },
            transient_windows,
            Duration::ZERO,
        );
        assert_eq!(r.unwrap(), "renamed");
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn retrying_is_bounded_and_a_real_error_is_not_retried() {
        let calls = Cell::new(0);
        let r: io::Result<()> = retry(
            || {
                calls.set(calls.get() + 1);
                Err(io::Error::from_raw_os_error(32))
            },
            transient_windows,
            Duration::ZERO,
        );
        assert!(r.is_err());
        assert_eq!(calls.get(), ATTEMPTS);
        calls.set(0);
        let r: io::Result<()> = retry(
            || {
                calls.set(calls.get() + 1);
                Err(io::Error::from(io::ErrorKind::NotFound))
            },
            transient_windows,
            Duration::ZERO,
        );
        assert!(r.is_err());
        assert_eq!(calls.get(), 1, "a missing file will not appear by waiting");
    }

    #[test]
    fn replace_file_overwrites_atomically_and_reports_a_missing_source() {
        let d = std::env::temp_dir().join(format!("vc-plat-{}", crate::new_uuid()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a"), "new").unwrap();
        std::fs::write(d.join("b"), "old").unwrap();
        replace_file(&d.join("a"), &d.join("b")).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("b")).unwrap(), "new");
        assert!(replace_file(&d.join("a"), &d.join("b")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_longest_mount_wins_and_only_network_types_count() {
        let mounts = "/dev/sda1 / ext4 rw 0 0\nnas:/export /mnt/nas nfs4 rw 0 0\n/dev/sdb1 /mnt/nas/local ext4 rw 0 0\n//srv/share /mnt/with\\040space cifs rw 0 0\n";
        let t = |p: &str| parse_proc_mounts(mounts, Path::new(p));
        assert_eq!(t("/home/me/vault.db").as_deref(), Some("ext4"));
        assert_eq!(t("/mnt/nas/vault.db").as_deref(), Some("nfs4"));
        assert_eq!(t("/mnt/nas/local/vault.db").as_deref(), Some("ext4"));
        assert_eq!(t("/mnt/with space/x").as_deref(), Some("cifs"));
        assert!(
            is_network_type("nfs4") && is_network_type("CIFS") && is_network_type("fuse.sshfs")
        );
        assert!(!is_network_type("ext4") && !is_network_type("tmpfs"));
    }

    #[test]
    fn the_macos_mount_listing_is_read_the_same_way() {
        let out = "/dev/disk3s1 on / (apfs, local, journaled)\nnas.local:/export on /Volumes/nas (nfs, nodev, nosuid)\n//me@srv/share on /Volumes/share (smbfs, nodev)\n";
        let t = |p: &str| parse_mount_output(out, Path::new(p));
        assert_eq!(t("/Users/me/x").as_deref(), Some("apfs"));
        assert_eq!(t("/Volumes/nas/v.db").as_deref(), Some("nfs"));
        assert_eq!(t("/Volumes/share/v.db").as_deref(), Some("smbfs"));
        assert_eq!(parse_mount_output("garbage", Path::new("/x")), None);
    }

    #[test]
    fn unc_paths_are_network_and_device_paths_are_not() {
        assert!(is_unc(Path::new("\\\\server\\share\\v.db")));
        assert!(is_unc(Path::new("\\\\?\\UNC\\server\\share\\v.db")));
        assert!(!is_unc(Path::new("\\\\?\\C:\\v.db")));
        assert!(!is_unc(Path::new("C:\\v.db")));
        assert!(!is_unc(Path::new("/home/me")));
    }

    #[test]
    fn a_local_temp_directory_is_not_reported_as_a_network_one() {
        assert_eq!(network_fs(&std::env::temp_dir()), None);
    }
}
