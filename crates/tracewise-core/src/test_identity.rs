use super::*;

fn repo(dir: &Path) -> PathBuf {
    let wt = dir.to_path_buf();
    fs::create_dir_all(wt.join(".git").join("tracewise")).unwrap();
    wt
}

/// A folder's id as the CLI answers it: the marker id (a file here stands in for the marker).
fn id_at(folder: &Path) -> Option<String> {
    fs::read_to_string(folder.join("marker")).ok().map(|s| s.trim().to_string())
}

#[test]
fn second_checkout_on_the_same_machine_is_a_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("home/projects/p");
    let (first, second) = (repo(&tmp.path().join("first")), repo(&tmp.path().join("second")));
    let id = repo_id(&data, &first);
    for f in [&first, &second] {
        fs::write(f.join("marker"), &id).unwrap();
    }
    assert_eq!(check_copy(&data, &first, &id_at), CopyState::Original);
    assert_eq!(check_copy(&data, &second, &id_at), CopyState::Copy { original: first.canonicalize().unwrap() });
}

#[test]
fn moved_folder_is_not_a_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("home/projects/p");
    let old = repo(&tmp.path().join("old"));
    let id = repo_id(&data, &old);
    fs::write(old.join("marker"), &id).unwrap();
    let new = tmp.path().join("new");
    fs::rename(&old, &new).unwrap();
    assert_eq!(check_copy(&data, &new, &id_at), CopyState::Moved);
    assert_eq!(check_copy(&data, &new, &id_at), CopyState::Original);
}

#[test]
fn work_tree_of_reads_the_stamp() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("home/projects/p");
    assert_eq!(work_tree_of(&data), None);
    let wt = repo(&tmp.path().join("wt"));
    repo_id(&data, &wt);
    assert_eq!(work_tree_of(&data), Some(wt.canonicalize().unwrap()));
}
