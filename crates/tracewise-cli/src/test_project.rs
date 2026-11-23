use super::*;
use std::fs;
use tempfile::TempDir;
use tracewise_core::db::Database;
use tracewise_core::project::{load_genesis, new_genesis, project_id, verify_genesis};
use tracewise_git::{is_valid_project_id, read_marker, write_marker, RepoPaths};

/// A v1 repository: `.git/tracewise` with a real database and one object.
fn v1_repo() -> (TempDir, RepoPaths) {
    let dir = TempDir::new().unwrap();
    let legacy = dir.path().join(".git/tracewise");
    fs::create_dir_all(legacy.join("objects/ab")).unwrap();
    drop(Database::open(&legacy.join("tracewise.db")).unwrap());
    fs::write(legacy.join("objects/ab/cdef"), "blob").unwrap();
    let repo = RepoPaths::discover(dir.path()).unwrap();
    (dir, repo)
}

#[test]
fn ensure_project_creates_marker_genesis_and_data_dir() {
    let (home, work) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let info = ensure_project(work.path(), home.path()).unwrap();
    assert_eq!(read_marker(work.path()).as_deref(), Some(info.id.as_str()));
    assert!(is_valid_project_id(&info.id));
    assert_eq!(info.data_dir, home.path().join("projects").join(&info.id));
    let g = load_genesis(&info.data_dir).unwrap();
    assert!(verify_genesis(&g));
    assert_eq!(project_id(&g), info.id);
    assert!(info.recovery_secret.is_some());
}

#[test]
fn ensure_project_twice_returns_the_same_id_and_no_secret() {
    let (home, work) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let first = ensure_project(work.path(), home.path()).unwrap();
    let second = ensure_project(work.path(), home.path()).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.recovery_secret, None);
}

#[test]
fn ensure_project_with_a_marker_but_no_local_genesis_keeps_the_id() {
    let (home, work) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let id = project_id(&new_genesis().unwrap().0);
    write_marker(work.path(), &id).unwrap();
    let info = ensure_project(work.path(), home.path()).unwrap();
    assert_eq!(info.id, id);
    assert!(info.data_dir.is_dir());
    assert_eq!(info.recovery_secret, None);
    assert!(load_genesis(&info.data_dir).is_none());
}

#[test]
fn ensure_project_rejects_a_genesis_that_does_not_match_the_marker() {
    let (home, work) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let info = ensure_project(work.path(), home.path()).unwrap();
    let other = project_id(&new_genesis().unwrap().0);
    write_marker(work.path(), &other).unwrap();
    fs::create_dir_all(home.path().join("projects").join(&other)).unwrap();
    fs::copy(info.data_dir.join("genesis.json"), home.path().join("projects").join(&other).join("genesis.json")).unwrap();
    let err = ensure_project(work.path(), home.path()).unwrap_err().to_string();
    assert!(err.contains(&other) && err.contains(&info.id), "{err}");
}

#[test]
fn migrate_moves_database_and_objects_and_leaves_a_moved_note() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
    assert!(Database::open(&info.data_dir.join("tracewise.db")).is_ok());
    assert_eq!(fs::read_to_string(info.data_dir.join("objects/ab/cdef")).unwrap(), "blob");
    let legacy = repo.git_common_dir.join("tracewise");
    assert!(!legacy.join("tracewise.db").exists());
    assert!(!repo.git_common_dir.join("tracewise.migrating").exists());
    assert_eq!(fs::read_to_string(legacy.join("MOVED")).unwrap().trim(), info.data_dir.display().to_string());
    assert_eq!(repo.data_dir_in(home.path()), info.data_dir);
}

#[test]
fn migrate_twice_is_a_no_op() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    assert!(migrate_v1(&repo, home.path()).unwrap().is_some());
    assert!(migrate_v1(&repo, home.path()).unwrap().is_none());
}

#[test]
fn migrate_keeps_the_old_data_when_the_copy_fails() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let id = project_id(&new_genesis().unwrap().0);
    write_marker(&repo.work_tree, &id).unwrap();
    fs::create_dir_all(home.path().join("projects")).unwrap();
    fs::write(home.path().join("projects").join(&id), "in the way").unwrap();
    assert!(migrate_v1(&repo, home.path()).is_err());
    let legacy = repo.git_common_dir.join("tracewise");
    assert!(Database::open(&legacy.join("tracewise.db")).is_ok());
    assert!(legacy.join("objects/ab/cdef").exists());
    assert!(!legacy.join("MOVED").exists());
    assert!(!repo.git_common_dir.join("tracewise.migrating").exists());
}

#[test]
fn migrate_updates_the_repos_registry() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
    let repos = tracewise_core::repos::list_in(home.path());
    assert!(repos.iter().any(|r| r.work_tree == repo.work_tree && r.data_dir == info.data_dir));
}

#[test]
fn migrate_from_a_linked_worktree_puts_the_marker_in_the_main_checkout() {
    let home = TempDir::new().unwrap();
    let (dir, main) = v1_repo();
    let wt = TempDir::new().unwrap();
    let repo = RepoPaths {
        work_tree: wt.path().to_path_buf(),
        git_dir: main.git_dir.join("worktrees/x"),
        git_common_dir: main.git_common_dir.clone(),
    };
    let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
    assert_eq!(read_marker(dir.path()), Some(info.id.clone()));
    assert_eq!(main.data_dir_in(home.path()), info.data_dir);
    assert_eq!(repo.data_dir_in(home.path()), info.data_dir);
}

#[test]
fn ensure_project_stamps_the_work_tree_into_the_data_dir() {
    let (home, work) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let info = ensure_project(work.path(), home.path()).unwrap();
    assert_eq!(tracewise_core::identity::work_tree_of(&info.data_dir), Some(work.path().canonicalize().unwrap()));
}

#[test]
fn migrate_keeps_the_legacy_id_and_stamp() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let legacy = repo.git_common_dir.join("tracewise");
    let id = tracewise_core::identity::repo_id(&legacy, &repo.work_tree);
    let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
    assert_eq!(fs::read_to_string(info.data_dir.join("id")).unwrap().trim(), id);
    assert_eq!(tracewise_core::identity::work_tree_of(&info.data_dir), Some(repo.work_tree.clone()));
}

fn back_date(path: &std::path::Path, secs: u64) {
    let when = std::time::SystemTime::now() - std::time::Duration::from_secs(secs);
    fs::File::options().write(true).open(path).unwrap().set_modified(when).unwrap();
}

#[test]
fn a_young_migrating_lock_is_respected() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let lock = repo.git_common_dir.join("tracewise.migrating");
    fs::write(&lock, "").unwrap();
    back_date(&lock, 60);
    assert!(migrate_v1(&repo, home.path()).unwrap().is_none());
    assert!(repo.git_common_dir.join("tracewise/tracewise.db").exists());
}

#[test]
fn a_stale_migrating_lock_is_taken_over() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let lock = repo.git_common_dir.join("tracewise.migrating");
    fs::write(&lock, "").unwrap();
    back_date(&lock, 11 * 60);
    assert!(migrate_v1(&repo, home.path()).unwrap().is_some());
    assert!(!lock.exists());
}

#[test]
fn a_copy_across_file_systems_is_refused_only_while_the_daemon_runs() {
    let cross = io::Error::from(io::ErrorKind::CrossesDevices);
    let msg = refuse_copy(&cross, true).unwrap().to_string();
    assert!(msg.contains("tracewise capture daemon stop"), "{msg}");
    assert!(refuse_copy(&cross, false).is_none());
    assert!(refuse_copy(&io::Error::from(io::ErrorKind::DirectoryNotEmpty), true).is_none());
}

#[cfg(unix)]
#[test]
fn copy_all_copies_files_and_folders_and_refuses_symlinks() {
    let t = TempDir::new().unwrap();
    let from = t.path().join("from");
    fs::create_dir_all(from.join("a/b")).unwrap();
    fs::write(from.join("top"), "1").unwrap();
    fs::write(from.join("a/b/deep"), "2").unwrap();
    copy_all(&from, &t.path().join("to"), None).unwrap();
    assert_eq!(fs::read_to_string(t.path().join("to/top")).unwrap(), "1");
    assert_eq!(fs::read_to_string(t.path().join("to/a/b/deep")).unwrap(), "2");

    std::os::unix::fs::symlink(from.join("top"), from.join("a/link")).unwrap();
    assert!(copy_all(&from, &t.path().join("to2"), None).is_err());
}

#[cfg(unix)]
#[test]
fn migrate_refuses_a_legacy_folder_with_a_symlink() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let legacy = repo.git_common_dir.join("tracewise");
    std::os::unix::fs::symlink("/nowhere", legacy.join("objects/ab/link")).unwrap();
    assert!(migrate_v1(&repo, home.path()).is_err());
    assert!(Database::open(&legacy.join("tracewise.db")).is_ok());
    assert!(
        legacy.join("objects/ab/cdef").exists()
            && !legacy.join("MOVED").exists()
            && !repo.git_common_dir.join("tracewise.migrating").exists()
    );
}

const KEPT: &[u8] = b"kept content\n";

/// A v1 repository with one record in its database and one blob.
fn v1_repo_with_history() -> (TempDir, RepoPaths, String) {
    let (dir, repo) = v1_repo();
    let legacy = repo.git_common_dir.join("tracewise");
    let db = Database::open(&legacy.join("tracewise.db")).unwrap();
    let intent = tracewise_core::model::Intent {
        id: ulid::Ulid::new().to_string(),
        kind: "intent".to_string(),
        actor: tracewise_core::model::Actor { kind: "agent".to_string(), name: Some("codex".to_string()), session: None },
        paths: vec!["a.txt".to_string()],
        why: "kept across the move".to_string(),
        task: None,
        created: "2026-10-23T10:00:00.000Z".to_string(),
        expires: "2099-01-01T00:00:00.000Z".to_string(),
        ended: 0,
    };
    db.insert_intent(&intent).unwrap();
    drop(db);
    let hash = tracewise_core::blob::BlobStore::new(&legacy).store(KEPT).unwrap();
    (dir, repo, hash)
}

fn assert_reachable(repo: &RepoPaths, home: &std::path::Path, hash: &str, when: &str) {
    let data = repo.data_dir_in(home);
    let db = Database::open(&data.join("tracewise.db")).unwrap();
    assert_eq!(db.list_intents(None).unwrap().len(), 1, "{when}: the record is in {}", data.display());
    assert_eq!(tracewise_core::blob::BlobStore::new(&data).read(hash).unwrap(), KEPT, "{when}: the blob is in {}", data.display());
}

#[test]
fn a_migration_that_fails_part_way_keeps_every_record_and_blob_reachable() {
    let cases = FAIL_POINTS.iter().map(|p| (false, *p)).chain(FAIL_POINTS.iter().chain(COPY_FAIL_POINTS).map(|p| (true, *p)));
    for (copy, point) in cases {
        let home = TempDir::new().unwrap();
        let (_dir, repo, hash) = v1_repo_with_history();
        FORCE_COPY.with(|f| f.set(copy));
        FAIL_AT.with(|f| f.set(point));
        let failed = migrate_v1(&repo, home.path());
        FAIL_AT.with(|f| f.set(""));
        let when = format!("{point} (copy: {copy})");
        assert!(failed.is_err(), "{when}: {failed:?}");
        assert_reachable(&repo, home.path(), &hash, &format!("after a failure at {when}"));
        let retry = migrate_v1(&repo, home.path()).unwrap_or_else(|e| panic!("{when}: the retry failed: {e}")).expect("the retry finishes");
        FORCE_COPY.with(|f| f.set(false));
        assert_eq!(repo.data_dir_in(home.path()), retry.data_dir, "{when}");
        assert_reachable(&repo, home.path(), &hash, &format!("after the retry from {when}"));
        assert!(load_genesis(&retry.data_dir).is_some_and(|g| verify_genesis(&g)), "{when}: the genesis survives");
        let legacy = repo.git_common_dir.join("tracewise");
        assert!(legacy.join("MOVED").exists() && !repo.git_common_dir.join("tracewise.migrating").exists(), "{when}");
    }
}

#[test]
fn a_move_across_file_systems_copies_verifies_and_switches() {
    let home = TempDir::new().unwrap();
    let (_dir, repo, hash) = v1_repo_with_history();
    FORCE_COPY.with(|f| f.set(true));
    let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
    FORCE_COPY.with(|f| f.set(false));
    assert_eq!(repo.data_dir_in(home.path()), info.data_dir);
    assert_reachable(&repo, home.path(), &hash, "after a copy");
    let legacy = repo.git_common_dir.join("tracewise");
    assert_eq!(fs::read_dir(&legacy).unwrap().count(), 1, "only the MOVED note is left");
    assert!(!repo.git_common_dir.join("tracewise.old").exists());
    assert!(!home.path().join("projects").join(format!("{}.staging", info.id)).exists());
}

#[test]
fn a_move_that_stopped_before_the_registry_was_written_is_finished() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
    // what a crash right after the switch leaves: the lock, no MOVED note, the registry still on the legacy folder
    let legacy = repo.git_common_dir.join("tracewise");
    fs::remove_file(legacy.join("MOVED")).unwrap();
    fs::write(repo.git_common_dir.join("tracewise.migrating"), "").unwrap();
    fs::write(home.path().join("repos"), format!("{}\t{}\n", repo.work_tree.display(), legacy.display())).unwrap();

    let finished = migrate_v1(&repo, home.path()).unwrap().expect("the job is finished");
    assert_eq!(finished.data_dir, info.data_dir);
    assert!(legacy.join("MOVED").exists());
    assert!(!repo.git_common_dir.join("tracewise.migrating").exists());
    let registered = fs::read_to_string(home.path().join("repos")).unwrap();
    assert!(registered.contains(&info.data_dir.display().to_string()), "{registered}");
    assert!(migrate_v1(&repo, home.path()).unwrap().is_none(), "nothing left to do");
}

#[test]
fn adopt_copy_starts_an_empty_history() {
    let (home, work) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let first = ensure_project(work.path(), home.path()).unwrap();
    drop(Database::open(&first.data_dir.join("tracewise.db")).unwrap());
    let second = adopt_copy(work.path(), home.path()).unwrap();
    assert_ne!(second.id, first.id);
    assert_ne!(second.data_dir, first.data_dir);
    assert_eq!(read_marker(work.path()).as_deref(), Some(second.id.as_str()));
    assert!(second.recovery_secret.is_some());
    let device = tracewise_core::identity::device_for(&second.data_dir);
    assert_ne!(device, tracewise_core::identity::device_for(&first.data_dir));
    assert!(tracewise_core::share::is_safe_id(&device), "{device}");
    // the history made before the copy stays with the original
    assert!(first.data_dir.join("tracewise.db").exists());
    assert!(!second.data_dir.join("tracewise.db").exists());
}

#[test]
fn a_second_checkout_never_deletes_the_history_another_checkout_already_moved() {
    let home = TempDir::new().unwrap();
    let (_a_dir, a, hash) = v1_repo_with_history();
    let moved = migrate_v1(&a, home.path()).unwrap().unwrap();
    // the marker reaches checkout B, which still has its own v1 history
    let (_b_dir, b) = v1_repo();
    write_marker(&b.work_tree, &moved.id).unwrap();
    let err = migrate_v1(&b, home.path()).unwrap_err().to_string();
    assert!(err.contains("adopt-copy") && err.contains(&moved.id), "{err}");
    assert_reachable(&a, home.path(), &hash, "A after B's attempt");
    assert!(b.git_common_dir.join("tracewise/tracewise.db").exists(), "B keeps its own history");
    assert!(!b.git_common_dir.join("tracewise/MOVED").exists());
}

#[test]
fn a_finished_move_leaves_no_moving_note() {
    let home = TempDir::new().unwrap();
    for copy in [false, true] {
        let (_dir, repo) = v1_repo();
        FORCE_COPY.with(|f| f.set(copy));
        let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
        FORCE_COPY.with(|f| f.set(false));
        assert!(!info.data_dir.join(MOVING_FROM).exists(), "copy: {copy}");
    }
}

#[test]
fn a_deleted_marker_is_restored_from_the_moved_note() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
    fs::remove_file(repo.work_tree.join(tracewise_git::MARKER_FILE)).unwrap();
    assert_eq!(repo.data_dir_in(home.path()), info.data_dir);
    let again = ensure_project(&repo.work_tree, home.path()).unwrap();
    assert_eq!(again.id, info.id);
    assert_eq!(again.recovery_secret, None);
    assert_eq!(read_marker(&repo.work_tree), Some(info.id));
}

#[test]
fn adopt_copy_of_a_migrated_copy_gets_a_new_id_despite_the_moved_note() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
    let adopted = adopt_copy(&repo.work_tree, home.path()).unwrap();
    assert_ne!(adopted.id, info.id);
}

#[test]
fn a_legacy_folder_without_the_lock_is_not_taken_for_a_started_move() {
    let home = TempDir::new().unwrap();
    // checkout A's moved history, and a checkout B whose `.git/tracewise` has no database and no MOVED note
    let (_a_dir, a) = v1_repo();
    let moved = migrate_v1(&a, home.path()).unwrap().unwrap();
    let b_dir = TempDir::new().unwrap();
    fs::create_dir_all(b_dir.path().join(".git/tracewise")).unwrap();
    let b = RepoPaths::discover(b_dir.path()).unwrap();
    write_marker(&b.work_tree, &moved.id).unwrap();
    assert!(migrate_v1(&b, home.path()).unwrap().is_none());
    assert!(!b.git_common_dir.join("tracewise/MOVED").exists());
    assert!(!tracewise_core::repos::list_in(home.path()).iter().any(|r| r.work_tree == b.work_tree));
}

#[test]
fn migrate_repoints_every_checkout_that_used_the_legacy_folder() {
    let home = TempDir::new().unwrap();
    let (_dir, repo) = v1_repo();
    let linked = TempDir::new().unwrap();
    let legacy = repo.git_common_dir.join("tracewise");
    tracewise_core::repos::register_in(home.path(), linked.path(), &legacy);
    let info = migrate_v1(&repo, home.path()).unwrap().unwrap();
    let repos = tracewise_core::repos::list_in(home.path());
    assert!(repos.iter().any(|r| r.work_tree == linked.path() && r.data_dir == info.data_dir), "the linked worktree follows");
}

#[test]
fn a_long_copy_keeps_the_migrating_lock_fresh() {
    let t = TempDir::new().unwrap();
    let from = t.path().join("from");
    fs::create_dir_all(from.join("a")).unwrap();
    fs::write(from.join("a/x"), "1").unwrap();
    let lock = t.path().join("lock");
    fs::write(&lock, "").unwrap();
    back_date(&lock, 11 * 60);
    copy_all(&from, &t.path().join("to"), Some(&lock)).unwrap();
    let age = fs::metadata(&lock).unwrap().modified().unwrap().elapsed().unwrap();
    assert!(age < std::time::Duration::from_secs(60), "{age:?}");
}

#[cfg(unix)]
#[test]
fn copied_folders_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let t = TempDir::new().unwrap();
    let from = t.path().join("from");
    fs::create_dir_all(from.join("a")).unwrap();
    copy_all(&from, &t.path().join("to"), None).unwrap();
    for d in ["to", "to/a"] {
        assert_eq!(fs::metadata(t.path().join(d)).unwrap().permissions().mode() & 0o077, 0, "{d}");
    }
}

#[test]
fn init_refuses_the_home_folder_the_root_and_a_folder_holding_the_tracewise_home() {
    let t = TempDir::new().unwrap();
    let (home, tw) = (t.path().join("me"), t.path().join("me/.tracewise"));
    fs::create_dir_all(home.join("proj")).unwrap();
    assert!(init_refusal(&home, &home, &tw).is_some());
    assert!(init_refusal(Path::new("/"), &home, &tw).is_some());
    assert!(init_refusal(&std::env::temp_dir(), &home, &tw).is_some(), "everything under the temp folder would join that project");
    assert!(init_refusal(t.path(), Path::new(""), &tw).is_some(), "it holds the Tracewise home");
    assert!(init_refusal(&home.join("proj"), &home, &tw).is_none());
    assert!(init_refusal(&home.join("proj"), Path::new(""), &t.path().join("elsewhere")).is_none());
}
