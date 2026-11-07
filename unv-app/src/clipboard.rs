// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Clearing the system clipboard some seconds after the app copied a secret to it (R15).
//!
//! The timer and the comparison live here, not in the page: a hidden or hung webview still gets its
//! clipboard cleared. The clipboard is cleared only if it still holds what the app put there, so
//! something the person copied in the meantime is left alone. A newer copy replaces the pending
//! one, so the clock always runs from the last copy.
//!
//! The system clipboard is reached with the platform's own tools (`wl-paste`/`wl-copy`, `xclip` or
//! `xsel` on Linux, `pbpaste`/`pbcopy` on macOS, PowerShell on Windows), run with fixed arguments and
//! no shell. If none is present the copy simply stays, which is reported to the caller as
//! [`Outcome::Unavailable`] so the page can try its own clear.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What happened when the timer fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The clipboard held the copied text and is now empty.
    Cleared,
    /// The clipboard holds something else now, or nothing: left as it is.
    Changed,
    /// No tool could read or clear the clipboard here.
    Unavailable,
    /// A newer copy took over the timer.
    Superseded,
}

/// The clipboard, as far as this module needs it.
pub trait Board: Send + Sync {
    /// The current text, or `None` when it cannot be read.
    fn read(&self) -> Option<String>;
    /// Empties the clipboard; `false` when it could not be done.
    fn clear(&self) -> bool;
}

fn same(a: &str, b: &str) -> bool {
    a.trim_end_matches(['\r', '\n']) == b.trim_end_matches(['\r', '\n'])
}

/// Clears `board` only if it still holds `ours`.
pub fn clear_if_unchanged(board: &dyn Board, ours: &str) -> Outcome {
    let Some(now) = board.read() else {
        return Outcome::Unavailable;
    };
    if !same(&now, ours) {
        return Outcome::Changed;
    }
    if board.clear() {
        Outcome::Cleared
    } else {
        Outcome::Unavailable
    }
}

/// The pending clear. One per app; `schedule` replaces whatever was pending.
pub struct Clearer {
    generation: Arc<AtomicU64>,
    board: Arc<dyn Board>,
}

impl Clearer {
    pub fn new(board: Arc<dyn Board>) -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            board,
        }
    }

    /// The real clipboard.
    pub fn system() -> Self {
        Self::new(Arc::new(SystemBoard))
    }

    /// Clears the clipboard after `delay` if it still holds `text` and no newer copy was scheduled.
    /// `done` is told how it went (from the timer's thread). Returns at once.
    pub fn schedule(
        &self,
        text: String,
        delay: Duration,
        done: impl FnOnce(Outcome) + Send + 'static,
    ) {
        let mine = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = Arc::clone(&self.generation);
        let board = Arc::clone(&self.board);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let outcome = if generation.load(Ordering::SeqCst) != mine {
                Outcome::Superseded
            } else {
                clear_if_unchanged(board.as_ref(), &text)
            };
            done(outcome);
        });
    }

    /// Cancels the pending clear (the person turned the setting off).
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// Runs a tool and returns what it printed, or `None` if it is missing, fails or takes over three
/// seconds (a clipboard owner that does not answer must not hold the timer thread).
fn run(program: &str, args: &[&str], capture: bool) -> Option<Vec<u8>> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        // A clearing tool may leave a background owner behind (xclip does) that keeps its copy of a
        // pipe open for ever; only a read needs its output.
        .stdout(if capture { Stdio::piped() } else { Stdio::null() })
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        // No console window flashing up from a GUI app (CREATE_NO_WINDOW).
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut out = Vec::new();
                if let Some(mut pipe) = child.stdout.take() {
                    pipe.read_to_end(&mut out).ok()?;
                }
                return Some(out);
            }
            Ok(None) if started.elapsed() < Duration::from_secs(3) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// The platform's clipboard, through its command-line tools.
pub struct SystemBoard;

impl SystemBoard {
    #[cfg(target_os = "linux")]
    fn tools() -> Vec<(&'static str, Vec<&'static str>)> {
        // (program, read arguments), the Wayland tool first when there is a Wayland session.
        let mut t = Vec::new();
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            t.push(("wl-paste", vec!["--no-newline"]));
        }
        t.push(("xclip", vec!["-selection", "clipboard", "-o"]));
        t.push(("xsel", vec!["--clipboard", "--output"]));
        t
    }
}

impl Board for SystemBoard {
    #[cfg(target_os = "linux")]
    fn read(&self) -> Option<String> {
        Self::tools()
            .iter()
            .find_map(|(prog, read)| run(prog, read, true))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    #[cfg(target_os = "linux")]
    fn clear(&self) -> bool {
        let mut cleared = false;
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            cleared |= run("wl-copy", &["--clear"], false).is_some();
        }
        // The X selection too: a Wayland session still runs X clients, and both may hold the text.
        cleared |= run("xclip", &["-selection", "clipboard", "-i"], false).is_some();
        cleared |= run("xsel", &["--clipboard", "--clear"], false).is_some();
        cleared
    }

    #[cfg(target_os = "macos")]
    fn read(&self) -> Option<String> {
        run("pbpaste", &[], true).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    #[cfg(target_os = "macos")]
    fn clear(&self) -> bool {
        run("pbcopy", &[], false).is_some()
    }

    #[cfg(windows)]
    fn read(&self) -> Option<String> {
        run(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-Clipboard -Raw",
            ],
            true,
        )
        .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    #[cfg(windows)]
    fn clear(&self) -> bool {
        run(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Set-Clipboard -Value $null",
            ],
            false,
        )
        .is_some()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    fn read(&self) -> Option<String> {
        None
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    fn clear(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::sync::Mutex;

    struct Fake {
        text: Mutex<Option<String>>,
        readable: bool,
        clearable: bool,
        clears: AtomicU64,
    }

    impl Fake {
        fn holding(t: &str) -> Arc<Self> {
            Arc::new(Self {
                text: Mutex::new(Some(t.to_string())),
                readable: true,
                clearable: true,
                clears: AtomicU64::new(0),
            })
        }
    }

    impl Board for Fake {
        fn read(&self) -> Option<String> {
            self.readable
                .then(|| self.text.lock().unwrap().clone().unwrap_or_default())
        }
        fn clear(&self) -> bool {
            self.clears.fetch_add(1, Ordering::SeqCst);
            if self.clearable {
                *self.text.lock().unwrap() = Some(String::new());
            }
            self.clearable
        }
    }

    #[test]
    fn it_clears_only_what_the_app_put_there() {
        let b = Fake::holding("hunter2");
        // A tool that adds a trailing newline still matches.
        *b.text.lock().unwrap() = Some("hunter2\n".into());
        assert_eq!(clear_if_unchanged(b.as_ref(), "hunter2"), Outcome::Cleared);
        assert_eq!(b.text.lock().unwrap().as_deref(), Some(""));
        // Something the person copied since is theirs.
        let other = Fake::holding("grocery list");
        assert_eq!(
            clear_if_unchanged(other.as_ref(), "hunter2"),
            Outcome::Changed
        );
        assert_eq!(other.clears.load(Ordering::SeqCst), 0);
        // No tool, or a tool that cannot clear, is reported rather than claimed.
        let blind = Arc::new(Fake {
            text: Mutex::new(None),
            readable: false,
            clearable: true,
            clears: AtomicU64::new(0),
        });
        assert_eq!(
            clear_if_unchanged(blind.as_ref(), "x"),
            Outcome::Unavailable
        );
        let stuck = Arc::new(Fake {
            text: Mutex::new(Some("x".into())),
            readable: true,
            clearable: false,
            clears: AtomicU64::new(0),
        });
        assert_eq!(
            clear_if_unchanged(stuck.as_ref(), "x"),
            Outcome::Unavailable
        );
    }

    #[test]
    fn a_newer_copy_takes_over_the_timer_and_cancel_stops_it() {
        // The bug this pins: with one timer per copy, the first copy's timer cleared the clipboard
        // while the person was still using the second copy.
        let b = Fake::holding("second");
        let c = Clearer::new(b.clone());
        let (tx, rx) = mpsc::channel();
        let tx1 = tx.clone();
        c.schedule("first".into(), Duration::from_millis(60), move |o| {
            tx1.send(("first", o)).unwrap();
        });
        c.schedule("second".into(), Duration::from_millis(60), move |o| {
            tx.send(("second", o)).unwrap();
        });
        let mut got: Vec<_> = (0..2)
            .map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect();
        got.sort_by_key(|(n, _)| *n);
        assert_eq!(
            got,
            vec![("first", Outcome::Superseded), ("second", Outcome::Cleared)]
        );

        let b2 = Fake::holding("kept");
        let c2 = Clearer::new(b2.clone());
        let (tx, rx) = mpsc::channel();
        c2.schedule("kept".into(), Duration::from_millis(60), move |o| {
            tx.send(o).unwrap();
        });
        c2.cancel();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Outcome::Superseded
        );
        assert_eq!(b2.clears.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_missing_tool_is_none_not_a_panic() {
        assert!(run("unv-no-such-clipboard-tool", &[], true).is_none());
        // A tool that fails is none as well.
        #[cfg(unix)]
        assert!(run("false", &[], true).is_none());
        #[cfg(unix)]
        assert_eq!(run("true", &[], true), Some(Vec::new()));
    }
}
