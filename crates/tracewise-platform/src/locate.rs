//! Finding the real `git` (not the Tracewise one first on `PATH`) and the full `tracewise` program: used by the `git` wrapper, the
//! command shims and the CLI, which would otherwise each carry a copy.
use std::fs;
use std::path::{Path, PathBuf};

pub fn is_tracewise_bin_name(name: &str) -> bool {
    name == "tracewise" || name == "tracewise.exe" || name == "tracewise-git" || name == "tracewise-git.exe"
}

pub fn find_real_git_inner(path_var: Option<&str>, wrapper_dir: Option<&Path>) -> PathBuf {
    if let Ok(p) = std::env::var("TRACEWISE_REAL_GIT") {
        let pb = PathBuf::from(&p);
        if pb.is_file() {
            return pb;
        }
    }
    let usr_git = Path::new("/usr/bin/git");
    if usr_git.is_file() {
        let is_wrapper = if let Some(w) = wrapper_dir { usr_git.parent() == Some(w) } else { false };
        if !is_wrapper {
            return PathBuf::from("/usr/bin/git");
        }
    }
    if let Some(path_str) = path_var {
        let self_exe = std::env::current_exe().ok().and_then(|p| p.canonicalize().ok());
        for dir in std::env::split_paths(path_str) {
            if let Some(w) = wrapper_dir {
                if dir == w {
                    continue;
                }
            }
            let candidate = dir.join("git");
            if candidate.is_file() {
                if let Ok(canon) = candidate.canonicalize() {
                    if let Some(ref self_p) = self_exe {
                        if &canon == self_p {
                            continue;
                        }
                    }
                    if let Some(name) = canon.file_name().and_then(|n| n.to_str()) {
                        if is_tracewise_bin_name(name) {
                            continue;
                        }
                    }
                }
                if let Ok(target) = fs::read_link(&candidate) {
                    if let Some(name) = target.file_name().and_then(|n| n.to_str()) {
                        if is_tracewise_bin_name(name) {
                            continue;
                        }
                    }
                }
                return candidate;
            }
        }
    }
    PathBuf::from("/usr/bin/git")
}

pub fn find_real_git(wrapper_dir: Option<&Path>) -> PathBuf {
    let path_val = std::env::var("PATH").ok();
    find_real_git_inner(path_val.as_deref(), wrapper_dir)
}

/// The full `tracewise` binary, or None. Never searches PATH: a `tracewise` found there can be a stale install or
/// another wrapper (that is how an old binary once looped through this one).
pub fn find_tracewise_bin_inner(self_exe: Option<&Path>, tracewise_bin_env: Option<&str>, home_env: Option<&str>) -> Option<PathBuf> {
    if let Some(exe) = self_exe {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("tracewise");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let Some(p) = tracewise_bin_env {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    if let Some(h) = home_env {
        let candidate = PathBuf::from(h).join(".cargo/bin/tracewise");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn find_tracewise_bin() -> Option<PathBuf> {
    // `current_exe` can be the symlink we were started through (a `git` in some bin folder), so resolve it:
    // the real file sits next to `tracewise`.
    let self_exe = std::env::current_exe().ok().map(|p| p.canonicalize().unwrap_or(p));
    let tracewise_bin = std::env::var("TRACEWISE_BIN").ok();
    let home = std::env::var("HOME").ok();
    find_tracewise_bin_inner(self_exe.as_deref(), tracewise_bin.as_deref(), home.as_deref())
}
