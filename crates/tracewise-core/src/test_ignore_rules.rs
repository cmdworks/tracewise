use super::*;
use std::fs;

#[test]
fn gitignore_rules_apply_to_single_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git/info")).unwrap();
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join(".gitignore"), "/target\n*.log\n!keep.log\nnode_modules\n").unwrap();
    fs::write(root.join("sub/.gitignore"), "*.tmp\n").unwrap();
    fs::write(root.join(".git/info/exclude"), "secret.txt\n").unwrap();
    fs::write(root.join(".tracewiseignore"), "generated/\n").unwrap();
    let mut r = IgnoreRules::load(root);
    for ignored in [
        "target/debug/tracewise",
        "a.log",
        "deep/b.log",
        "web/node_modules/x/index.js",
        "sub/x.tmp",
        "secret.txt",
        "generated/out.rs",
        ".DS_Store",
    ] {
        assert!(r.is_ignored(Path::new(ignored)), "{ignored} should be ignored");
    }
    for kept in ["src/main.rs", "keep.log", "x.tmp", "targets/a.txt", "sub/y.txt", ".gitignore"] {
        assert!(!r.is_ignored(Path::new(kept)), "{kept} should be recorded");
    }
}

#[test]
fn is_ignored_without_git_for_builtin_folders() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = IgnoreRules::load(dir.path());
    assert!(r.is_ignored(Path::new("web/node_modules/x.js")));
    assert!(r.is_ignored(Path::new("build/out.txt")));
    assert!(!r.is_ignored(Path::new("src/main.rs")));
}

#[test]
fn is_not_ignored_with_git() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    let mut r = IgnoreRules::load(dir.path());
    assert!(!r.is_ignored(Path::new("build/out.txt")));
}

#[test]
fn the_project_marker_is_ignored_only_at_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = IgnoreRules::load(dir.path());
    assert!(r.is_ignored(Path::new(".tracewiseproject")));
    assert!(!r.is_ignored(Path::new("sub/.tracewiseproject")));
}

#[test]
fn the_tracewise_home_inside_the_work_tree_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = IgnoreRules::load_in(dir.path(), &dir.path().join(".tracewise"));
    assert!(r.is_ignored(Path::new(".tracewise/device.key")));
    assert!(r.is_ignored(Path::new(".tracewise/projects/x/tracewise.db")));
    assert!(!r.is_ignored(Path::new(".tracewiseignore")));
    assert!(!r.is_ignored(Path::new("src/a.rs")));
}

#[test]
fn with_dot_git_as_a_file_the_builtin_ignores_do_not_apply() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".git"), "gitdir: /elsewhere\n").unwrap();
    let mut r = IgnoreRules::load(dir.path());
    assert!(!r.is_ignored(Path::new("build/out.txt")));
    assert!(!r.is_ignored(Path::new("node_modules/x.js")));
}
