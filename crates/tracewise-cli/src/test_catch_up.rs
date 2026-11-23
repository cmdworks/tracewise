use super::*;
use std::path::PathBuf;
use tracewise_core::db::Database;
use tracewise_core::scan::scan_repository;

/// A project with one recorded file, and the paths of the repository.
fn project() -> (tempfile::TempDir, RepoPaths) {
    let temp = tempfile::tempdir().unwrap();
    let work_tree = temp.path().join("repo");
    let git_dir = work_tree.join(".git");
    let data_dir = git_dir.join("tracewise");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(work_tree.join("a.txt"), "one\n").unwrap();
    scan_repository(&work_tree, &data_dir, "main", "1", &ScanOptions::default(), &[PathBuf::from("a.txt")]).unwrap();
    let repo = RepoPaths { work_tree, git_dir: git_dir.clone(), git_common_dir: git_dir };
    (temp, repo)
}

fn edit(repo: &RepoPaths, text: &str) {
    fs::write(repo.work_tree.join("a.txt"), text).unwrap();
}

fn changes(repo: &RepoPaths) -> Vec<tracewise_core::model::Change> {
    Database::open(&repo.data_dir().join("tracewise.db")).unwrap().list_changes(Some("a.txt"), None).unwrap()
}

#[test]
fn a_hand_edit_is_recorded_as_catch_up_and_as_the_person() {
    let (_t, repo) = project();
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Prompt, CaptureMode::Agents, false, false, 10_000), 1);
    let newest = changes(&repo).remove(0);
    assert_eq!(newest.source, "catch-up");
    assert_ne!(newest.actor.kind, "agent", "an edit found at the start of a turn is the person's");
    assert_eq!(newest.confidence, "likely");
}

#[test]
fn full_mode_with_a_running_watcher_does_nothing() {
    let (_t, repo) = project();
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Full, true, false, 10_000), 0);
    assert_eq!(changes(&repo).len(), 1, "only the baseline create");
}

#[test]
fn full_mode_with_a_stopped_watcher_catches_up() {
    let (_t, repo) = project();
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Full, false, false, 10_000), 1);
}

#[test]
fn every_command_looks_again_so_a_fresh_save_is_seen_at_once() {
    let (_t, repo) = project();
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_000), 1);
    edit(&repo, "one\ntwo\nthree\n");
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_010), 1, "10 ms later, still seen");
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_020), 0, "nothing new: nothing recorded");
}

#[test]
fn a_prompt_looks_right_after_a_command() {
    let (_t, repo) = project();
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_000), 0);
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Prompt, CaptureMode::Agents, false, false, 10_100), 1);
}

#[test]
fn a_slow_last_run_skips_the_prompt_but_not_a_command() {
    let (_t, repo) = project();
    fs::write(repo.data_dir().join("catch-up"), "1000 900").unwrap();
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Prompt, CaptureMode::Agents, false, false, 20_000), 0);
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 20_000), 1);
}

#[test]
fn a_project_with_no_history_is_left_alone() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    fs::create_dir_all(&git_dir).unwrap();
    let repo = RepoPaths { work_tree: temp.path().to_path_buf(), git_dir: git_dir.clone(), git_common_dir: git_dir };
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_000), 0);
}

#[test]
fn a_stale_lock_is_taken_over_and_a_fresh_one_is_respected() {
    let (_t, repo) = project();
    edit(&repo, "one\ntwo\n");
    let lock = repo.data_dir().join("catch-up.lock");
    fs::write(&lock, "").unwrap();
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_000), 0, "someone else is scanning");
    let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    fs::OpenOptions::new().write(true).open(&lock).unwrap().set_modified(long_ago).unwrap();
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_000), 1);
    assert!(!lock.exists(), "the lock is gone afterwards");
}

#[test]
fn an_agent_reading_the_history_records_nothing_but_its_prompt_hook_does() {
    let (_t, repo) = project();
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, true, 10_000), 0, "an agent's `log` is not a catch-up");
    assert_eq!(changes(&repo).len(), 1);
    assert_eq!(catch_up_with(&repo, Trigger::Prompt, CaptureMode::Agents, false, true, 10_000), 1);
    let newest = changes(&repo).remove(0);
    assert_eq!((newest.actor.kind.as_str(), newest.confidence.as_str()), ("human", "likely"), "{newest:?}");
}

#[test]
fn a_command_run_by_a_person_names_the_person_too() {
    let (_t, repo) = project();
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_000), 1);
    let newest = changes(&repo).remove(0);
    assert_eq!((newest.actor.kind.as_str(), newest.confidence.as_str()), ("human", "likely"), "never the environment's agent, never exact");
}

#[test]
fn a_slow_prompt_catch_up_looks_again_after_a_minute() {
    let (_t, repo) = project();
    fs::write(repo.data_dir().join("catch-up"), "1000 900").unwrap();
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Prompt, CaptureMode::Agents, false, false, 30_000), 0, "30 s after a slow run: skipped");
    assert!(prompt_is_throttled(&repo.data_dir()));
    assert_eq!(catch_up_with(&repo, Trigger::Prompt, CaptureMode::Agents, false, false, 62_000), 1, "a minute later: looks anyway");
}

#[test]
fn a_stamp_from_the_future_does_not_stop_anything() {
    let (_t, repo) = project();
    fs::write(repo.data_dir().join("catch-up"), "9999999999999 10").unwrap();
    edit(&repo, "one\ntwo\n");
    assert_eq!(catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_000), 1);
}

#[test]
fn two_catch_ups_at_once_record_one_edit_once() {
    let (_t, repo) = project();
    edit(&repo, "one\ntwo\n");
    let repo = std::sync::Arc::new(repo);
    let total: usize = (0..4)
        .map(|_| {
            let repo = repo.clone();
            std::thread::spawn(move || catch_up_with(&repo, Trigger::Command, CaptureMode::Agents, false, false, 10_000))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .sum();
    assert_eq!(total, 1);
    assert_eq!(changes(&repo).len(), 2, "the baseline create and one edit");
}
