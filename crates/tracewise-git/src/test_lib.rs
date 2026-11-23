use super::*;
use std::fs;

#[test]
fn test_discover_standard_repo() {
    let temp = tempfile::tempdir().unwrap();
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(repo_dir.join(".git")).unwrap();
    fs::create_dir_all(repo_dir.join("src")).unwrap();

    let discovered = RepoPaths::discover(&repo_dir.join("src")).unwrap();
    assert_eq!(discovered.work_tree, repo_dir.canonicalize().unwrap());
    assert_eq!(discovered.git_dir, repo_dir.join(".git").canonicalize().unwrap());
    assert_eq!(discovered.git_common_dir, repo_dir.join(".git").canonicalize().unwrap());
    assert_eq!(discovered.data_dir(), repo_dir.join(".git").canonicalize().unwrap().join("tracewise"));
}

#[test]
fn test_discover_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let main_repo = temp.path().join("main");
    let main_git = main_repo.join(".git");
    let wt_git_dir = main_git.join("worktrees").join("wt1");
    fs::create_dir_all(&wt_git_dir).unwrap();
    fs::write(wt_git_dir.join("commondir"), "../..\n").unwrap();

    let wt_repo = temp.path().join("wt1");
    fs::create_dir_all(&wt_repo).unwrap();
    fs::write(wt_repo.join(".git"), format!("gitdir: {}\n", wt_git_dir.display())).unwrap();

    let discovered = RepoPaths::discover(&wt_repo).unwrap();
    assert_eq!(discovered.work_tree, wt_repo.canonicalize().unwrap());
    assert_eq!(discovered.git_dir, wt_git_dir.canonicalize().unwrap());
    assert_eq!(discovered.git_common_dir, main_git.canonicalize().unwrap());
    assert_eq!(discovered.data_dir(), main_git.canonicalize().unwrap().join("tracewise"));
}

#[test]
fn test_discover_none() {
    let temp = tempfile::tempdir().unwrap();
    assert!(RepoPaths::discover(temp.path()).is_none());
}

#[test]
fn test_baseline_files_counting() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::write(repo.join(".git/config"), "test").unwrap();
    fs::write(repo.join("README.md"), "# Hello").unwrap();
    fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(repo.join("ignored.txt"), "secret").unwrap();
    fs::write(repo.join(".tracewiseignore"), "skip.txt\n").unwrap();
    fs::write(repo.join("skip.txt"), "skipped").unwrap();

    let files = walk_baseline_files(repo);
    assert_eq!(files, vec![PathBuf::from(".gitignore"), PathBuf::from(".tracewiseignore"), PathBuf::from("README.md")]);
    assert_eq!(count_baseline_files(repo), 3);
}

#[test]
fn test_get_git_info() {
    let temp = tempfile::tempdir().unwrap();
    let repo_dir = temp.path().join("repo");
    let git_dir = repo_dir.join(".git");
    fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/feature\n").unwrap();
    fs::write(git_dir.join("refs/heads/feature"), "abcdef1234567890\n").unwrap();

    let repo = RepoPaths { work_tree: repo_dir.clone(), git_dir: git_dir.clone(), git_common_dir: git_dir };

    let (branch, head) = get_git_info(&repo);
    assert_eq!(branch, "feature");
    assert_eq!(head, "abcdef1234567890");
}

fn marker_id() -> String {
    format!("tw1:{}", "ab".repeat(13))
}

#[test]
fn discover_finds_a_marker_only_folder_from_a_subfolder() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    fs::create_dir_all(root.join("a/b")).unwrap();
    write_marker(&root, &marker_id()).unwrap();
    let d = RepoPaths::discover(&root.join("a/b")).unwrap();
    assert_eq!(d.work_tree, root.canonicalize().unwrap());
    assert!(!d.has_git());
    assert_eq!(d.git_dir, d.work_tree.join(".git"));
}

#[test]
fn discover_prefers_the_nearest_of_git_and_marker() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("sub/x")).unwrap();
    write_marker(&root.join("sub"), &marker_id()).unwrap();
    let d = RepoPaths::discover(&root.join("sub/x")).unwrap();
    assert_eq!(d.work_tree, root.join("sub").canonicalize().unwrap());
}

#[test]
fn data_dir_is_home_projects_id_when_marker_exists() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    fs::create_dir_all(&root).unwrap();
    write_marker(&root, &marker_id()).unwrap();
    let d = RepoPaths::discover(&root).unwrap();
    let home = t.path().join("home");
    assert_eq!(d.data_dir_in(&home), home.join("projects").join(marker_id()));
}

#[test]
fn data_dir_keeps_legacy_folder_while_its_db_exists() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    fs::create_dir_all(root.join(".git/tracewise")).unwrap();
    fs::write(root.join(".git/tracewise/tracewise.db"), "").unwrap();
    write_marker(&root, &marker_id()).unwrap();
    let d = RepoPaths::discover(&root).unwrap();
    assert_eq!(d.data_dir_in(&t.path().join("home")), d.git_common_dir.join("tracewise"));
}

#[test]
fn data_dir_defaults_to_legacy_without_marker() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    fs::create_dir_all(root.join(".git")).unwrap();
    let d = RepoPaths::discover(&root).unwrap();
    assert_eq!(d.data_dir_in(&t.path().join("home")), d.git_common_dir.join("tracewise"));
}

#[test]
fn data_dir_is_stable_when_git_appears() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    fs::create_dir_all(&root).unwrap();
    write_marker(&root, &marker_id()).unwrap();
    let home = t.path().join("home");
    let before = RepoPaths::discover(&root).unwrap().data_dir_in(&home);
    fs::create_dir_all(root.join(".git")).unwrap();
    assert_eq!(RepoPaths::discover(&root).unwrap().data_dir_in(&home), before);
}

#[test]
fn linked_worktree_finds_the_marker_of_the_main_work_tree() {
    let t = tempfile::tempdir().unwrap();
    let main = t.path().join("main");
    let wt_git = main.join(".git/worktrees/wt1");
    fs::create_dir_all(&wt_git).unwrap();
    fs::write(wt_git.join("commondir"), "../..\n").unwrap();
    write_marker(&main, &marker_id()).unwrap();
    let wt = t.path().join("wt1");
    fs::create_dir_all(&wt).unwrap();
    fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
    let home = t.path().join("home");
    assert_eq!(RepoPaths::discover(&wt).unwrap().data_dir_in(&home), home.join("projects").join(marker_id()));
}

fn put(root: &Path, rel: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, "x").unwrap();
}

#[test]
fn walk_without_git_skips_builtin_folders() {
    let t = tempfile::tempdir().unwrap();
    for f in ["node_modules/x.js", "target/a", "src/main.rs"] {
        put(t.path(), f);
    }
    assert_eq!(walk_baseline_files(t.path()), vec![PathBuf::from("src/main.rs")]);
}

#[test]
fn walk_without_git_honours_gitignore_and_tracewiseignore() {
    let t = tempfile::tempdir().unwrap();
    for f in ["a.log", "b.tmp", "keep.txt"] {
        put(t.path(), f);
    }
    fs::write(t.path().join(".gitignore"), "*.log\n").unwrap();
    fs::write(t.path().join(".tracewiseignore"), "*.tmp\n").unwrap();
    let files = walk_baseline_files(t.path());
    assert!(files.contains(&PathBuf::from("keep.txt")));
    assert!(!files.contains(&PathBuf::from("a.log")) && !files.contains(&PathBuf::from("b.tmp")));
}

#[test]
fn builtin_ignores_do_not_apply_inside_a_git_repo() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir_all(t.path().join(".git")).unwrap();
    put(t.path(), "build/out.txt");
    assert_eq!(walk_baseline_files(t.path()), vec![PathBuf::from("build/out.txt")]);
}

#[test]
fn walk_skips_the_project_marker_only_at_the_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join(MARKER_FILE), "project = x\n").unwrap();
    fs::write(root.join("sub").join(MARKER_FILE), "project = x\n").unwrap();
    fs::write(root.join("a.txt"), "a").unwrap();
    assert_eq!(walk_baseline_files(root), [PathBuf::from("a.txt"), PathBuf::from("sub").join(MARKER_FILE)]);
}

#[test]
fn a_legacy_database_made_after_the_move_does_not_switch_the_project_back() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    fs::create_dir_all(root.join(".git/tracewise")).unwrap();
    fs::write(root.join(".git/tracewise/tracewise.db"), "").unwrap();
    let home = t.path().join("home");
    fs::write(root.join(".git/tracewise/MOVED"), format!("{}\n", home.join("projects").join(marker_id()).display())).unwrap();
    write_marker(&root, &marker_id()).unwrap();
    let d = RepoPaths::discover(&root).unwrap();
    assert_eq!(d.data_dir_in(&home), home.join("projects").join(marker_id()));
}

#[test]
fn a_deleted_marker_is_found_again_through_the_moved_note() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    fs::create_dir_all(root.join(".git/tracewise")).unwrap();
    let home = t.path().join("home");
    let target = home.join("projects").join(marker_id());
    fs::write(root.join(".git/tracewise/MOVED"), format!("{}\n", target.display())).unwrap();
    let d = RepoPaths::discover(&root).unwrap();
    assert_eq!(d.data_dir_in(&home), target);
    assert_eq!(moved_id(&d.git_common_dir.join("tracewise")), Some(marker_id()));
    // a note naming anything but a project folder is not followed
    fs::write(root.join(".git/tracewise/MOVED"), "/somewhere/else/tw1:zz\n").unwrap();
    assert_eq!(d.data_dir_in(&home), d.git_common_dir.join("tracewise"));
}

#[test]
fn walk_skips_the_tracewise_home_inside_the_root() {
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "a.txt");
    put(t.path(), ".tracewise/device.key");
    put(t.path(), ".tracewise/projects/x/tracewise.db");
    assert_eq!(walk_baseline_files_in(t.path(), &t.path().join(".tracewise")), vec![PathBuf::from("a.txt")]);
}

#[test]
fn with_dot_git_as_a_file_the_builtin_ignores_do_not_apply() {
    let t = tempfile::tempdir().unwrap();
    fs::write(t.path().join(".git"), "gitdir: /elsewhere\n").unwrap();
    put(t.path(), "build/out.txt");
    assert_eq!(walk_baseline_files(t.path()), vec![PathBuf::from("build/out.txt")]);
}
