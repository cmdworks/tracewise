//! A v1 history (`.git/tracewise`) moves to the Tracewise home on the first command a person runs, never from a hook.
mod common;

/// A git repository with a v1 history and nothing in the Tracewise home.
fn v1() -> common::Repo {
    let repo = common::Repo::uninitialised("");
    let legacy = repo.dir().join(".git/tracewise");
    std::fs::create_dir_all(&legacy).unwrap();
    drop(tracewise_core::db::Database::open(&legacy.join("tracewise.db")).unwrap());
    repo
}

#[test]
fn the_move_prints_its_recovery_secret_once() {
    let repo = v1();
    let first = repo.me(&["status"]);
    assert!(first.ok(), "{}", first.err);
    assert_eq!(first.err.matches("twrecover1:").count(), 1, "{}", first.err);
    assert!(!first.out.contains("twrecover1:"), "only on stderr: {}", first.out);
    assert!(repo.dir().join(".git/tracewise/MOVED").exists());
    let second = repo.me(&["status"]);
    assert!(!second.err.contains("twrecover1:") && !second.out.contains("twrecover1:"), "{}", second.err);
}

#[test]
fn git_hooks_and_the_daemon_never_move_the_history() {
    let repo = v1();
    for args in [&["git-hook", "pre-commit"][..], &["daemon", "scan-paths"], &["daemon", "sweep"], &["daemon", "active"]] {
        let run = repo.with("human:me", args, Some(""), &[]);
        assert!(!run.err.contains("twrecover1:"), "{args:?}: {}", run.err);
        assert!(repo.dir().join(".git/tracewise/tracewise.db").exists(), "{args:?}");
        assert!(!repo.dir().join(".git/tracewise/MOVED").exists(), "{args:?}");
        assert!(!repo.home().join("projects").exists(), "{args:?}");
    }
}

#[test]
fn a_command_the_daemon_starts_never_moves_the_history() {
    let repo = v1();
    let run = repo.with("human:me", &["scan"], None, &[("TRACEWISE_NO_MIGRATE", "1")]);
    assert!(!run.err.contains("twrecover1:"), "{}", run.err);
    assert!(repo.dir().join(".git/tracewise/tracewise.db").exists() && !repo.dir().join(".git/tracewise/MOVED").exists());
    // the same command run by a person moves it
    let run = repo.me(&["scan"]);
    assert!(run.err.contains("twrecover1:"), "{}", run.err);
    assert!(repo.dir().join(".git/tracewise/MOVED").exists());
}
