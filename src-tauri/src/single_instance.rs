// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! One desktop copy per vault (R38 row 8), with no dependency beyond the standard library.
//!
//! The first copy takes an exclusive lock on `instance.lock` in the data directory and keeps it
//! until it exits; the operating system drops the lock when the process dies, so a crash never
//! leaves a stale one. A later copy finds the lock taken, writes `instance.wake`, and exits. The
//! first copy watches that file and raises its window when its contents change. If the lock cannot
//! be tried at all (a read-only directory, an odd file system) the app starts normally rather than
//! refusing to start.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The held lock. Dropping it (or the process ending) lets the next copy become the first.
pub struct Guard {
    _lock: File,
}

pub enum Claim {
    /// This copy is the first and holds the lock.
    First(Guard),
    /// Another copy is running; it has been asked to raise its window.
    Second,
    /// The lock could not be tried; carry on as the only copy.
    Unavailable,
}

const LOCK: &str = "instance.lock";
const WAKE: &str = "instance.wake";

pub fn claim(dir: &Path) -> Claim {
    if fs::create_dir_all(dir).is_err() {
        return Claim::Unavailable;
    }
    let Ok(file) = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK))
    else {
        return Claim::Unavailable;
    };
    match file.try_lock() {
        Ok(()) => Claim::First(Guard { _lock: file }),
        Err(TryLockError::WouldBlock) => {
            ring(dir);
            Claim::Second
        }
        Err(TryLockError::Error(_)) => Claim::Unavailable,
    }
}

/// Asks the running copy to raise its window: the file's contents change on every call.
pub fn ring(dir: &Path) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let _ = fs::write(dir.join(WAKE), format!("{nanos} {}", std::process::id()));
}

/// Calls `on_ring` whenever another copy rings. Stops when dropped.
pub struct Watcher {
    stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn watch(dir: PathBuf, every: Duration, on_ring: impl Fn() + Send + 'static) -> Watcher {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let read = move || fs::read_to_string(dir.join(WAKE)).unwrap_or_default();
    // What is already there (an old ring from before this copy started) is not a ring.
    let mut seen = read();
    std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            std::thread::sleep(every);
            let now = read();
            // `fs::write` truncates before it writes, so a read can land on an empty file: that is
            // a ring half written, not a second ring.
            if !now.is_empty() && now != seen {
                seen = now;
                on_ring();
            }
        }
    });
    Watcher { stop }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("unv-single-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn the_first_copy_holds_the_lock_and_a_second_one_is_told_to_stand_down() {
        let dir = scratch("claim");
        let Claim::First(guard) = claim(&dir) else {
            panic!("an empty directory has no other copy")
        };
        assert!(matches!(claim(&dir), Claim::Second));
        assert!(dir.join(WAKE).exists(), "the second copy rings the first");
        // Closing the first copy frees the lock for the next one.
        drop(guard);
        assert!(matches!(claim(&dir), Claim::First(_)));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_directory_that_cannot_be_made_does_not_stop_the_app_starting() {
        let dir = scratch("blocked");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a-file");
        fs::write(&file, b"x").unwrap();
        // A directory below a regular file cannot exist.
        assert!(matches!(claim(&file.join("below")), Claim::Unavailable));
        // A lock file that cannot be opened (the name is taken by a directory) is the same.
        fs::create_dir_all(dir.join(LOCK)).unwrap();
        assert!(matches!(claim(&dir), Claim::Unavailable));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_ring_wakes_the_watcher_and_an_old_ring_does_not() {
        let dir = scratch("watch");
        fs::create_dir_all(&dir).unwrap();
        ring(&dir); // before the watcher exists
        let hits = Arc::new(AtomicUsize::new(0));
        let h = Arc::clone(&hits);
        let w = watch(dir.clone(), Duration::from_millis(20), move || {
            h.fetch_add(1, Ordering::Relaxed);
        });
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(hits.load(Ordering::Relaxed), 0, "an old ring is not news");
        ring(&dir);
        for _ in 0..100 {
            if hits.load(Ordering::Relaxed) > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(hits.load(Ordering::Relaxed), 1);
        // Dropping the watcher stops it: a later ring is not heard.
        drop(w);
        std::thread::sleep(Duration::from_millis(80));
        ring(&dir);
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(hits.load(Ordering::Relaxed), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
