//! The catch-up scan: records what changed on disk that nothing recorded live (a hand edit, a script, another program). The
//! watcher does that continuously; without one (`capture.mode = agents`, or a watcher that is not running) this does it at the
//! moments that matter: before a command reads the history, and when an agent turn starts. Unchanged files are skipped by size
//! and modification time, so it costs one stat per file.
//!
//! What it finds is recorded as the person's (`likely`, `source = catch-up`): nothing else saw the write, and the agents that
//! have hooks were recorded live. A write by something no hook saw (an agent without hooks, a dev server an agent left running)
//! is therefore recorded as the person's too; only the watcher, with its windows, can tell better.
use crate::util;
use std::fs;
use std::path::Path;
use std::time::Instant;
use tracewise_core::capture::{capture_mode, CaptureMode};
use tracewise_core::db::Database;
use tracewise_core::model::Actor;
use tracewise_core::scan::{scan_repository_with_db, ScanOptions};
use tracewise_git::{get_git_info, walk_baseline_files, RepoPaths};

/// Why a catch-up is asked for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Trigger {
    /// A command is about to read the history.
    Command,
    /// A person sent a prompt to an agent: whatever changed since the last turn is theirs.
    Prompt,
}

/// After a run this slow a prompt does not wait for another one (a command still does).
const SLOW_MS: u128 = 500;
/// A prompt that skipped because the last scan was slow looks again after this long, so one slow run cannot switch it off for good.
const RECHECK_MS: u128 = 60_000;
/// A lock file older than this belongs to a process that died.
const STALE_LOCK_MS: u128 = 10_000;

fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// Records what changed since the last look, when this project has no live watcher. Returns how many changes it recorded;
/// 0 when it was skipped, and on any error (it never makes the command or the hook it rides on fail).
pub(crate) fn catch_up(repo: &RepoPaths, trigger: Trigger) -> usize {
    let by_agent = Actor::from_env().is_some_and(|a| a.kind == "agent");
    catch_up_with(repo, trigger, capture_mode(&repo.work_tree), util::is_daemon_running(), by_agent, now_ms())
}

/// `by_agent`: the process was started by an agent. An agent reading the history does not record on its own: what it would
/// find, the next prompt finds, and says whose it is.
pub(crate) fn catch_up_with(
    repo: &RepoPaths,
    trigger: Trigger,
    mode: CaptureMode,
    watcher_running: bool,
    by_agent: bool,
    now: u128,
) -> usize {
    if (mode == CaptureMode::Full && watcher_running) || (trigger == Trigger::Command && by_agent) {
        return 0;
    }
    let data_dir = repo.data_dir();
    let db_path = data_dir.join("tracewise.db");
    // a copy of the project that was not adopted shares the original's history: its files are not the original's changes
    if !db_path.is_file() || util::in_unadopted_copy(repo) {
        return 0;
    }
    let stamp = data_dir.join("catch-up");
    if let Some((at, took)) = read_stamp(&stamp) {
        // a stamp from the future (the clock went back) counts for nothing
        let age = (at <= now).then(|| now - at);
        // a command always looks (a person who saves a file and runs `tracewise log` expects to see it); a prompt looks too, unless
        // the last look was slow, and then at most once a minute
        if trigger == Trigger::Prompt && took > SLOW_MS && age.is_some_and(|a| a < RECHECK_MS) {
            return 0;
        }
    }
    // Not needed for correctness (the database's write lock serialises two scans), but it keeps a second terminal from walking the tree.
    let Some(_lock) = Lock::take(&data_dir.join("catch-up.lock")) else { return 0 };

    let started = Instant::now();
    // a prompt must not wait on someone else's write lock: the hot open gives up after a quarter of a second
    let opened = if trigger == Trigger::Prompt { Database::open_hot(&db_path) } else { Database::open(&db_path) };
    let recorded = match opened {
        Ok(mut db) => {
            let files = walk_baseline_files(&repo.work_tree);
            let (branch, head) = get_git_info(repo);
            scan_repository_with_db(&repo.work_tree, &data_dir, &branch, &head, &options(), &files, &mut db).map(|c| c.len()).unwrap_or(0)
        }
        Err(_) => 0,
    };
    // written before the lock goes, so whoever takes the lock next sees this run
    let _ = tracewise_platform::private::write(&stamp, format!("{now} {}", started.elapsed().as_millis()));
    recorded
}

/// How long the last catch-up took, for `doctor`.
pub(crate) fn last_run_ms(data_dir: &Path) -> Option<u128> {
    read_stamp(&data_dir.join("catch-up")).map(|(_, took)| took)
}

/// Whether a prompt would skip its catch-up right now because the last one was slow.
pub(crate) fn prompt_is_throttled(data_dir: &Path) -> bool {
    last_run_ms(data_dir).is_some_and(|took| took > SLOW_MS)
}

/// Both triggers name the person themselves: the environment of this process may belong to an agent, and the changes found are not its.
fn options() -> ScanOptions {
    ScanOptions {
        source: "catch-up".to_string(),
        actor: Some(util::human_actor_from_system()),
        confidence: Some("likely".to_string()),
        ..ScanOptions::default()
    }
}

/// `(when the last run started, how long it took)`, both in milliseconds.
fn read_stamp(path: &Path) -> Option<(u128, u128)> {
    let text = fs::read_to_string(path).ok()?;
    let (at, took) = text.split_once(' ')?;
    Some((at.trim().parse().ok()?, took.trim().parse().ok()?))
}

/// A lock file made with `create_new`, removed when dropped.
struct Lock(std::path::PathBuf);

impl Lock {
    fn take(path: &Path) -> Option<Self> {
        let make = || fs::OpenOptions::new().write(true).create_new(true).open(path).ok().map(|_| Self(path.to_path_buf()));
        if let Some(lock) = make() {
            return Some(lock);
        }
        let age_ms = fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|d| d.as_millis());
        if age_ms.is_some_and(|a| a > STALE_LOCK_MS) {
            let _ = fs::remove_file(path);
            return make();
        }
        None
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
#[path = "test_catch_up.rs"]
mod tests;
