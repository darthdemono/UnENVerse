// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! `unv exec --sandbox` (R34): run the command in a bubblewrap sandbox on Linux.
//!
//! UnENVerse is not a sandbox and cannot tell a backdoored model from a clean one. This is an
//! opt-in launcher that limits what the child can read and reach, so a stolen or fooled process
//! finds less: the whole file system is read-only, the home directory is replaced by an empty one,
//! `.env*` files in the working directory are shadowed by empty files, and the network is off.
//! The working directory stays readable, because a build needs its sources.
//!
//! It **fails closed**. Where the sandbox cannot be enforced (not Linux, no `bwrap`, or the kernel
//! refuses user namespaces) `exec --sandbox` stops with exit 7 and runs nothing. It never falls
//! back to running unsandboxed. There is no host allowlist: egress control is not content
//! control, and a trusted host can still serve a payload, so the network is either off or on
//! (`--sandbox-net`). macOS and Windows are not supported and say so.

use crate::error::{CliError, CliResult, Code};
use std::path::{Path, PathBuf};

/// The `bwrap` arguments for one run, ending in `--` and the command. Pure, so the policy can be
/// read and tested without a kernel.
///
/// Mounts apply in order and a later one wins: the home directory is emptied first, then the
/// working directory is put back read-only (it may live under home), then each `.env*` file in it
/// is covered with `/dev/null`.
pub fn bwrap_args(
    cwd: &Path,
    home: Option<&Path>,
    env_files: &[PathBuf],
    scratch: Option<&Path>,
    net: bool,
    argv: &[String],
) -> Vec<String> {
    let s = |p: &Path| p.display().to_string();
    let mut a: Vec<String> = [
        "--die-with-parent",
        "--new-session",
        "--unshare-user",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
    ]
    .iter()
    .map(|x| x.to_string())
    .collect();
    if !net {
        a.push("--unshare-net".into());
    }
    for x in [
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
    ] {
        a.push(x.into());
    }
    if let Some(h) = home {
        a.extend(["--tmpfs".into(), s(h)]);
    }
    if cwd != Path::new("/") {
        a.extend(["--ro-bind".into(), s(cwd), s(cwd)]);
    }
    for f in env_files {
        a.extend(["--ro-bind".into(), "/dev/null".into(), s(f)]);
    }
    if let Some(dir) = scratch {
        // File-shaped credentials are written here before the command starts.
        a.extend(["--ro-bind".into(), s(dir), s(dir)]);
    }
    a.push("--".into());
    a.extend(argv.iter().cloned());
    a
}

/// `.env` and `.env.*` files directly inside `dir` (templates such as `.env.example` included:
/// shadowing a placeholder file costs nothing).
pub fn env_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n == ".env" || n.starts_with(".env."))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Whether this machine can enforce the sandbox, with the reason when it cannot.
pub fn available() -> Result<(), String> {
    #[cfg(not(target_os = "linux"))]
    {
        Err(format!(
            "the sandbox needs Linux and bubblewrap; {} is not supported yet",
            std::env::consts::OS
        ))
    }
    #[cfg(target_os = "linux")]
    {
        let probe = std::process::Command::new("bwrap")
            .args([
                "--unshare-user",
                "--unshare-net",
                "--ro-bind",
                "/",
                "/",
                "--",
                "true",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match probe {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err("bubblewrap is installed but the kernel refused to start a sandbox (user namespaces may be disabled)".into()),
            Err(_) => Err("bubblewrap (bwrap) is not installed".into()),
        }
    }
}

/// The command that runs `argv` inside the sandbox, or the refusal. Exit 7: the sandbox is
/// unavailable, so nothing runs.
pub fn command(
    argv: &[String],
    net: bool,
    scratch: Option<&Path>,
) -> CliResult<std::process::Command> {
    available().map_err(|why| {
        CliError::new(
            Code::Unavailable,
            format!("Refusing to run unsandboxed: {why}"),
        )
    })?;
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::from(format!("Cannot read the working directory: {e}")))?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let files = env_files_in(&cwd);
    let mut c = std::process::Command::new("bwrap");
    c.args(bwrap_args(
        &cwd,
        home.as_deref(),
        &files,
        scratch,
        net,
        argv,
    ));
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn pos(a: &[String], needle: &[&str]) -> usize {
        a.windows(needle.len())
            .position(|w| w.iter().zip(needle).all(|(x, y)| x == y))
            .unwrap_or_else(|| panic!("{needle:?} not in {a:?}"))
    }

    #[test]
    fn the_network_is_off_unless_asked_and_the_command_comes_last_after_a_double_dash() {
        let off = bwrap_args(
            Path::new("/w"),
            None,
            &[],
            None,
            false,
            &argv(&["make", "-j2"]),
        );
        assert!(off.contains(&"--unshare-net".to_string()));
        assert_eq!(&off[off.len() - 3..], &["--", "make", "-j2"]);
        let on = bwrap_args(Path::new("/w"), None, &[], None, true, &argv(&["make"]));
        assert!(!on.contains(&"--unshare-net".to_string()));
    }

    #[test]
    fn the_file_system_is_read_only_and_home_is_emptied_before_the_project_is_put_back() {
        let a = bwrap_args(
            Path::new("/home/me/app"),
            Some(Path::new("/home/me")),
            &[],
            None,
            false,
            &argv(&["x"]),
        );
        assert!(pos(&a, &["--ro-bind", "/", "/"]) < pos(&a, &["--tmpfs", "/home/me"]));
        // The working directory returns after home was emptied, or it would be hidden too.
        assert!(
            pos(&a, &["--tmpfs", "/home/me"])
                < pos(&a, &["--ro-bind", "/home/me/app", "/home/me/app"])
        );
        // Nothing writable is mounted from the host except the temp directory, which is empty.
        assert!(!a.contains(&"--bind".to_string()));
    }

    #[test]
    fn env_files_are_covered_with_an_empty_file_after_the_project_is_mounted() {
        let files = vec![
            PathBuf::from("/w/.env"),
            PathBuf::from("/w/.env.production"),
        ];
        let a = bwrap_args(Path::new("/w"), None, &files, None, false, &argv(&["x"]));
        let project = pos(&a, &["--ro-bind", "/w", "/w"]);
        let one = pos(&a, &["--ro-bind", "/dev/null", "/w/.env"]);
        let two = pos(&a, &["--ro-bind", "/dev/null", "/w/.env.production"]);
        assert!(project < one && one < two, "{a:?}");
    }

    #[test]
    fn the_scratch_directory_for_file_credentials_is_visible_inside() {
        let a = bwrap_args(
            Path::new("/w"),
            None,
            &[],
            Some(Path::new("/tmp/unv-1-2")),
            false,
            &argv(&["x"]),
        );
        assert!(
            pos(&a, &["--tmpfs", "/tmp"]) < pos(&a, &["--ro-bind", "/tmp/unv-1-2", "/tmp/unv-1-2"])
        );
    }

    #[test]
    fn a_working_directory_of_root_is_not_bound_over_itself() {
        let a = bwrap_args(Path::new("/"), None, &[], None, false, &argv(&["x"]));
        assert!(!a
            .windows(3)
            .any(|w| w == ["--ro-bind", "/", "/"][..] && false));
        assert_eq!(a.iter().filter(|x| *x == "--ro-bind").count(), 1, "{a:?}");
    }

    #[test]
    fn only_env_files_in_the_directory_are_found() {
        let d = std::env::temp_dir().join(format!("unv-sbx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        for f in [".env", ".env.local", "env", "x.env", ".envrc"] {
            std::fs::write(d.join(f), "A=1").unwrap();
        }
        std::fs::write(d.join("sub/.env"), "A=1").unwrap();
        let found: Vec<_> = env_files_in(&d)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(found, vec![".env", ".env.local"]);
        let _ = std::fs::remove_dir_all(d);
    }
}
