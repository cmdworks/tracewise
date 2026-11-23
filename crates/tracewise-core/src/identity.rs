//! Who a Tracewise data folder belongs to: an id and the work-tree path it was made for.
//!
//! Copying a repository with its `.git` folder copies the Tracewise history too, and two copies would then record
//! into separate histories under the same id and push to the same device branch. The stamp tells a **copy**
//! (the original is still there with the same id) from a **move** (the old path is gone).
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyState {
    /// This is the folder the data was made for.
    Original,
    /// The folder was moved (the old path no longer holds this id); the stamp has been updated.
    Moved,
    /// A copy: `original` still exists with the same id.
    Copy { original: PathBuf },
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn read(data_dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(data_dir.join(name)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// The repository id, created (with the path stamp) on first use.
pub fn repo_id(data_dir: &Path, main_work_tree: &Path) -> String {
    if let Some(id) = read(data_dir, "id") {
        if read(data_dir, "home").is_none() {
            let _ = tracewise_platform::private::write(data_dir.join("home"), canonical(main_work_tree).to_string_lossy().as_bytes());
        }
        return id;
    }
    let id = ulid::Ulid::new().to_string();
    let _ = tracewise_platform::private::create_dir_all(data_dir);
    let _ = tracewise_platform::private::write(data_dir.join("id"), &id);
    let _ = tracewise_platform::private::write(data_dir.join("home"), canonical(main_work_tree).to_string_lossy().as_bytes());
    id
}

/// Compares the stamp with where this data folder is now. `main_work_tree` is the repository's own work tree
/// (not a linked worktree: those share the data folder and are not copies). `id_at(folder)` is the id a checkout at
/// `folder` has, `None` when it has none.
pub fn check_copy(data_dir: &Path, main_work_tree: &Path, id_at: &dyn Fn(&Path) -> Option<String>) -> CopyState {
    let id = repo_id(data_dir, main_work_tree);
    let here = canonical(main_work_tree);
    let Some(home) = read(data_dir, "home").map(PathBuf::from) else {
        return CopyState::Original;
    };
    if home == here {
        return CopyState::Original;
    }
    if id_at(&home).as_deref() == Some(id.as_str()) {
        CopyState::Copy { original: home }
    } else {
        let _ = tracewise_platform::private::write(data_dir.join("home"), here.to_string_lossy().as_bytes());
        CopyState::Moved
    }
}

/// The work tree stamped into this data folder (`<data_dir>/home`): how a data folder outside the work tree finds it.
pub fn work_tree_of(data_dir: &Path) -> Option<PathBuf> {
    read(data_dir, "home").map(PathBuf::from)
}

/// The device name this repository pushes under: its own override (written by the CLI's `project::adopt_copy`) or the machine's.
pub fn device_for(data_dir: &Path) -> String {
    read(data_dir, "device").unwrap_or_else(crate::get_device_id)
}

#[cfg(test)]
#[path = "test_identity.rs"]
mod tests;
