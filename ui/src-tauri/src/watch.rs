//! Tells the app and the menu bar when a history changed, instead of asking again and again.
//!
//! The operating system reports a write to a history file (`tracewise.db`, its journal, the watcher's `daemon.pid`); a short wait
//! gathers a burst of writes into one signal, which becomes the `history-changed` event for the window and a wake-up for the menu.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;

/// A burst of writes (one change is several) is one signal if the writes are this close together.
const QUIET: Duration = Duration::from_millis(250);

/// The files whose change means "read the history again".
pub fn is_history_file(path: &Path) -> bool {
    matches!(path.file_name().and_then(|n| n.to_str()), Some("tracewise.db" | "tracewise.db-wal" | "daemon.pid" | "repos" | "config.toml"))
}

/// Which of `events` (paths that changed) are worth a signal.
pub fn any_history_file(paths: &[PathBuf]) -> bool {
    paths.iter().any(|p| is_history_file(p))
}

/// Watches folders and sends one `()` per burst of writes to a history file.
pub struct Watcher {
    inner: Mutex<Option<notify::RecommendedWatcher>>,
    watched: Mutex<HashSet<PathBuf>>,
    tx: Sender<Vec<PathBuf>>,
}

impl Watcher {
    /// A watcher and the receiving end of its signals: each message is the burst's first batch of changed paths.
    pub fn new() -> (Self, Receiver<()>) {
        let (tx, raw) = channel::<Vec<PathBuf>>();
        let (signal, out) = channel::<()>();
        // the quiet wait: after one write, wait until none came for a moment, then send one signal
        std::thread::spawn(move || {
            while let Ok(first) = raw.recv() {
                let mut relevant = any_history_file(&first);
                loop {
                    match raw.recv_timeout(QUIET) {
                        Ok(more) => relevant |= any_history_file(&more),
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                if relevant && signal.send(()).is_err() {
                    return;
                }
            }
        });
        let sender = tx.clone();
        let inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                let _ = sender.send(event.paths);
            }
        })
        .ok();
        (Self { inner: Mutex::new(inner), watched: Mutex::new(HashSet::new()), tx }, out)
    }

    /// Watches these folders too (each once). A folder that does not exist is left for later.
    pub fn ensure(&self, dirs: &[PathBuf]) {
        use notify::{RecursiveMode, Watcher as _};
        let (Ok(mut inner), Ok(mut watched)) = (self.inner.lock(), self.watched.lock()) else { return };
        let Some(w) = inner.as_mut() else { return };
        for dir in dirs {
            if !watched.contains(dir) && dir.is_dir() && w.watch(dir, RecursiveMode::NonRecursive).is_ok() {
                watched.insert(dir.clone());
            }
        }
    }

    /// A signal made by hand (after the menu itself changed something).
    pub fn poke(&self) {
        let _ = self.tx.send(vec![PathBuf::from("tracewise.db")]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_history_files_count() {
        assert!(is_history_file(Path::new("/h/projects/x/tracewise.db")));
        assert!(is_history_file(Path::new("/h/projects/x/tracewise.db-wal")));
        assert!(is_history_file(Path::new("/h/daemon.pid")));
        assert!(!is_history_file(Path::new("/h/daemon.log")));
        assert!(!is_history_file(Path::new("/h/projects/x/objects/ab/cdef")));
        assert!(any_history_file(&[PathBuf::from("/h/a.log"), PathBuf::from("/h/projects/x/tracewise.db")]));
        assert!(!any_history_file(&[]));
    }

    #[test]
    fn a_burst_of_writes_is_one_signal_and_other_files_are_none() {
        let dir = std::env::temp_dir().join(format!("tracewise-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (watcher, signals) = Watcher::new();
        watcher.ensure(&[dir.clone()]);
        // give the system a moment to start reporting
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(dir.join("noise.log"), "x").unwrap();
        assert!(signals.recv_timeout(Duration::from_millis(900)).is_err(), "a log file is not a history");
        for i in 0..5 {
            std::fs::write(dir.join("tracewise.db"), format!("{i}")).unwrap();
        }
        assert!(signals.recv_timeout(Duration::from_secs(3)).is_ok(), "a write to the history is reported");
        assert!(signals.recv_timeout(Duration::from_millis(900)).is_err(), "five writes close together are one signal");
        watcher.poke();
        assert!(signals.recv_timeout(Duration::from_secs(2)).is_ok(), "a poke is reported too");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
