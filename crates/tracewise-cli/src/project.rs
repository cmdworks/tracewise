//! Projects without git: `ensure_project` names a folder (marker + signed genesis), `migrate_v1` moves v1 data out of `.git/tracewise`.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracewise_core::db::Database;
use tracewise_core::project::{load_genesis, new_genesis, project_id, save_genesis, verify_genesis};
use tracewise_git::{read_marker, write_marker, RepoPaths, MARKER_FILE};

const MOVED: &str = "MOVED";
/// Inside a moving history: names the legacy folder it came from, so only that folder's own leftover copy is ever deleted.
const MOVING_FROM: &str = ".moving-from";
/// A lock older than this was left by a process that died mid-move.
const STALE_LOCK: std::time::Duration = std::time::Duration::from_secs(10 * 60);

#[derive(Debug)]
pub struct ProjectInfo {
    pub id: String,
    pub data_dir: PathBuf,
    /// Only when this call created the genesis; it is not stored anywhere.
    pub recovery_secret: Option<String>,
}

/// The project `work_tree` belongs to, created (genesis, marker, data folder) when it has no marker yet. A marker deleted after a
/// migration is written again from the legacy folder's `MOVED` note, so the project is not forked into a new one.
pub fn ensure_project(work_tree: &Path, home: &Path) -> io::Result<ProjectInfo> {
    ensure(work_tree, home, true)
}

fn ensure(work_tree: &Path, home: &Path, recover: bool) -> io::Result<ProjectInfo> {
    let recovered = || {
        let id = tracewise_git::moved_id(&work_tree.join(".git").join("tracewise"))?;
        (!work_tree.join(MARKER_FILE).exists() && home.join("projects").join(&id).is_dir()).then_some(id)
    };
    if let Some(id) = read_marker(work_tree).or_else(|| recover.then(recovered).flatten()) {
        if read_marker(work_tree).is_none() {
            write_marker(work_tree, &id)?;
        }
        let data_dir = home.join("projects").join(&id);
        tracewise_platform::private::create_dir_all(&data_dir)?;
        if let Some(g) = load_genesis(&data_dir) {
            let found = project_id(&g);
            if !verify_genesis(&g) || found != id {
                let msg = format!("the genesis in {} (id {found}) does not belong to the marker's project {id}", data_dir.display());
                return Err(io::Error::new(io::ErrorKind::InvalidData, msg));
            }
        }
        tracewise_core::identity::repo_id(&data_dir, work_tree);
        return Ok(ProjectInfo { id, data_dir, recovery_secret: None });
    }
    if work_tree.join(MARKER_FILE).exists() {
        // never replace a marker we cannot read: it may name a project whose history lives elsewhere
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} has no valid project id", work_tree.join(MARKER_FILE).display()),
        ));
    }
    let (g, secret) = new_genesis()?;
    let id = project_id(&g);
    let data_dir = home.join("projects").join(&id);
    tracewise_platform::private::create_dir_all(&data_dir)?;
    save_genesis(&data_dir, &g)?;
    tracewise_core::identity::repo_id(&data_dir, work_tree);
    write_marker(work_tree, &id)?;
    Ok(ProjectInfo { id, data_dir, recovery_secret: Some(secret) })
}

/// Why `init` must not make `root` a project: the home folder, `/` or the system temp folder (everything in it would be recorded,
/// and every project made under it would belong to it) or a folder holding the Tracewise home (the watcher would record its own writes). `home` empty means there is no home folder to compare.
pub fn init_refusal(root: &Path, home: &Path, tracewise_home: &Path) -> Option<String> {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    // a Tracewise home that does not exist yet cannot be canonicalized: compare as given too
    let holds_home = tracewise_home.starts_with(root) || canon(tracewise_home).starts_with(canon(root));
    let root = canon(root);
    if root.parent().is_none() || (!home.as_os_str().is_empty() && root == canon(home)) || root == canon(&std::env::temp_dir()) {
        return Some(format!(
            "{} is the home folder, the file system root or the temp folder; run `tracewise init` in a project folder",
            root.display()
        ));
    }
    holds_home.then(|| {
        format!("{} holds the Tracewise home ({}); run `tracewise init` in a project folder", root.display(), tracewise_home.display())
    })
}

/// Gives a copied checkout a project of its own: new genesis, id and data folder. History made before the copy stays with the original.
pub fn adopt_copy(work_tree: &Path, home: &Path) -> io::Result<ProjectInfo> {
    match fs::remove_file(work_tree.join(MARKER_FILE)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    // a copy carries the original's MOVED note: never take the original's id back from it
    let info = ensure(work_tree, home, false)?;
    // a device name of its own, so this copy and the original never push to the same ref
    let device = format!("{}-copy-{}", tracewise_core::get_device_id(), info.id[info.id.len() - 6..].to_lowercase());
    tracewise_platform::private::write(info.data_dir.join("device"), &device)?;
    Ok(info)
}

/// Moves `<git common dir>/tracewise` into `<home>/projects/<id>` as one folder; `None` when there is nothing to move or another
/// process is moving it. On one file system the switch is a single rename; across file systems a verified copy is renamed into
/// place first and the legacy folder leaves after it, so an interrupted move leaves the whole history in one of the two folders.
pub fn migrate_v1(repo: &RepoPaths, home: &Path) -> io::Result<Option<ProjectInfo>> {
    let legacy = repo.git_common_dir.join("tracewise");
    // beside the legacy folder, not in it: that folder is renamed away
    let lock = repo.git_common_dir.join("tracewise.migrating");
    if !legacy.join("tracewise.db").exists() {
        return finish_interrupted(repo, home, &legacy, &lock);
    }
    if legacy.join(MOVED).exists() {
        return Ok(None);
    }
    if fs::metadata(&lock).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|age| age > STALE_LOCK)) {
        let _ = fs::remove_file(&lock);
    }
    match fs::OpenOptions::new().write(true).create_new(true).open(&lock) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
        r => drop(r?),
    }
    let moved = move_legacy(repo, &legacy, home);
    // after the switch the lock stays on failure: it tells the next run to finish the job
    if moved.is_ok() || legacy.join("tracewise.db").exists() {
        let _ = fs::remove_file(&lock);
    }
    let info = moved?;
    tracewise_core::repos::repoint_in(home, &legacy, &info.data_dir);
    tracewise_core::repos::register_in(home, &repo.work_tree, &info.data_dir);
    Ok(Some(info))
}

/// A move that stopped after the switch (the database is in the new folder) but before the note and the registry were written.
fn finish_interrupted(repo: &RepoPaths, home: &Path, legacy: &Path, lock: &Path) -> io::Result<Option<ProjectInfo>> {
    let data_dir = repo.data_dir_in(home);
    // only the lock says a move started here: a legacy folder without a database may just be another checkout's leftover
    if !lock.exists() || data_dir == legacy || !data_dir.join("tracewise.db").is_file() {
        return Ok(None);
    }
    fs::create_dir_all(legacy)?;
    fs::write(legacy.join(MOVED), format!("{}\n", data_dir.display()))?;
    let _ = fs::remove_file(data_dir.join(MOVING_FROM));
    let _ = fs::remove_dir_all(repo.git_common_dir.join("tracewise.old"));
    tracewise_core::repos::repoint_in(home, legacy, &data_dir);
    tracewise_core::repos::register_in(home, &repo.work_tree, &data_dir);
    let _ = fs::remove_file(lock);
    let id = data_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Some(ProjectInfo { id, data_dir, recovery_secret: None }))
}

fn move_legacy(repo: &RepoPaths, legacy: &Path, home: &Path) -> io::Result<ProjectInfo> {
    // checked before anything moves, so a refusal leaves the legacy folder as it was
    no_symlinks(legacy)?;
    // a linked worktree shares the main checkout's data, so the marker goes where the main checkout finds it too
    let main = (repo.git_common_dir.file_name().is_some_and(|n| n == ".git")).then(|| repo.git_common_dir.parent()).flatten();
    let root = match main {
        Some(main) if read_marker(&repo.work_tree).is_none() => main,
        _ => &repo.work_tree,
    };
    let info = ensure_project(root, home)?;
    let target = &info.data_dir;
    // the WAL's commits go into the database file, so the folder alone is the whole history
    Database::open(&legacy.join("tracewise.db")).and_then(|db| db.checkpoint()).map_err(io::Error::other)?;
    if target.join("tracewise.db").exists() {
        // only this folder's own leftover copy (a cross-device move that stopped before the legacy folder left) may go
        let ours = fs::read_to_string(target.join(MOVING_FROM)).is_ok_and(|t| Path::new(t.trim()) == legacy);
        if !ours {
            let msg = format!(
                "another checkout's history is already in {}; run `tracewise doctor --adopt-copy` to give this checkout a project of its own",
                target.display()
            );
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, msg));
        }
        fs::remove_dir_all(target)?;
    } else {
        // the new folder's own files (genesis, stamp) join the legacy folder so the project moves as one; legacy id and stamp win
        for e in fs::read_dir(target)? {
            let name = e?.file_name();
            if legacy.join(&name).exists() {
                fs::remove_file(target.join(&name))?;
            } else {
                move_entry(&target.join(&name), &legacy.join(&name))?;
            }
        }
        fs::remove_dir(target)?;
    }
    // travels with the history, so a retry can tell its own leftover copy from another checkout's history
    fs::write(legacy.join(MOVING_FROM), format!("{}\n", legacy.display()))?;
    step("before-switch")?;
    if let Err(e) = switch_rename(legacy, target) {
        if e.kind() != io::ErrorKind::CrossesDevices {
            return Err(e);
        }
        if let Some(refused) = refuse_copy(&e, daemon_running()) {
            return Err(refused);
        }
        copy_into_place(repo, legacy, target)?;
    }
    step("after-switch")?;
    fs::create_dir_all(legacy)?;
    fs::write(legacy.join(MOVED), format!("{}\n", target.display()))?;
    let _ = fs::remove_file(target.join(MOVING_FROM));
    Ok(info)
}

/// The one rename that moves the whole project (a test can make it act as if the target were on another file system).
fn switch_rename(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(test)]
    if FORCE_COPY.with(|f| f.get()) {
        return Err(io::Error::from(io::ErrorKind::CrossesDevices));
    }
    fs::rename(from, to)
}

fn daemon_running() -> bool {
    // tests never look at a real daemon; `refuse_copy` is tested on its own
    !cfg!(test) && crate::util::is_daemon_running()
}

/// Across file systems: a verified copy beside the target is renamed into place, then the legacy folder leaves by one rename on its
/// own file system. Until that rename the legacy folder is the history, so a leftover copy is simply discarded.
fn copy_into_place(repo: &RepoPaths, legacy: &Path, target: &Path) -> io::Result<()> {
    let mut staging = target.as_os_str().to_owned();
    staging.push(".staging");
    let staging = PathBuf::from(staging);
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    copy_all(legacy, &staging, Some(&repo.git_common_dir.join("tracewise.migrating")))?;
    step("after-copy")?;
    if tree(legacy)? != tree(&staging)? {
        let _ = fs::remove_dir_all(&staging);
        return Err(io::Error::other("the history changed while it was copied; run any tracewise command again"));
    }
    fs::rename(&staging, target)?;
    step("before-legacy-leaves")?;
    let old = repo.git_common_dir.join("tracewise.old");
    if old.exists() {
        fs::remove_dir_all(&old)?;
    }
    fs::rename(legacy, &old)?;
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

/// Every file under `root` with its size, to check a copy.
fn tree(root: &Path) -> io::Result<Vec<(PathBuf, u64)>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, u64)>) -> io::Result<()> {
        for e in fs::read_dir(dir)? {
            let e = e?;
            let meta = fs::symlink_metadata(e.path())?;
            if meta.is_dir() {
                walk(root, &e.path(), out)?;
            } else {
                out.push((e.path().strip_prefix(root).unwrap_or(&e.path()).to_path_buf(), meta.len()));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

/// `rename`, or across file systems a full copy and only then the removal of the original.
fn move_entry(from: &Path, to: &Path) -> io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_all(from, to, None)?;
    if from.is_dir() {
        fs::remove_dir_all(from)
    } else {
        fs::remove_file(from)
    }
}

/// A copy to another file system would miss what a running daemon writes meanwhile; a rename on one file system would not.
fn refuse_copy(rename_error: &io::Error, daemon_running: bool) -> Option<io::Error> {
    (rename_error.kind() == io::ErrorKind::CrossesDevices && daemon_running).then(|| {
        io::Error::other("the Tracewise home is on another file system; run `tracewise capture daemon stop` first, then any tracewise command")
    })
}

fn symlink_error(p: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{} is a symbolic link; move it out of the folder and run again", p.display()))
}

fn no_symlinks(p: &Path) -> io::Result<()> {
    let kind = fs::symlink_metadata(p)?.file_type();
    if kind.is_symlink() {
        return Err(symlink_error(p));
    }
    if kind.is_dir() {
        for e in fs::read_dir(p)? {
            no_symlinks(&e?.path())?;
        }
    }
    Ok(())
}

/// Copies a tree without following symbolic links (a link is an error) and syncs every file before the caller deletes the original.
/// `lock` is touched before each entry, so a long copy is never mistaken for a stale move.
fn copy_all(from: &Path, to: &Path, lock: Option<&Path>) -> io::Result<()> {
    if let Some(lock) = lock {
        let _ = fs::File::options().write(true).open(lock).and_then(|f| f.set_modified(std::time::SystemTime::now()));
    }
    let kind = fs::symlink_metadata(from)?.file_type();
    if kind.is_symlink() {
        return Err(symlink_error(from));
    }
    if kind.is_dir() {
        tracewise_platform::private::create_dir_all(to)?;
        for e in fs::read_dir(from)? {
            let e = e?;
            copy_all(&e.path(), &to.join(e.file_name()), lock)?;
        }
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::File::open(to)?.sync_all()
}

#[cfg(test)]
thread_local! {
    static FAIL_AT: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
    static FORCE_COPY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// The failure points of a move on one file system, then those only a copy across file systems reaches.
#[cfg(test)]
const FAIL_POINTS: &[&str] = &["before-switch", "after-switch"];
#[cfg(test)]
const COPY_FAIL_POINTS: &[&str] = &["after-copy", "before-legacy-leaves"];

/// A named point of the move; a test can make it fail to check what an interrupted move leaves behind.
fn step(name: &'static str) -> io::Result<()> {
    #[cfg(test)]
    if FAIL_AT.with(|f| f.get()) == name {
        return Err(io::Error::other(format!("test failure at {name}")));
    }
    let _ = name;
    Ok(())
}

#[cfg(test)]
#[path = "test_project.rs"]
mod tests;
