use std::path::Path;
#[cfg(test)]
use std::{fs, path::PathBuf};

mod read_only;
use read_only::is_read_only_git;

pub use tracewise_platform::locate::{find_real_git, find_real_git_inner, find_tracewise_bin, find_tracewise_bin_inner, is_tracewise_bin_name};

fn exec_or_run(target: &Path, args: &[String], arg0: Option<&str>, remove_depth: bool) -> ! {
    let env_remove = if remove_depth { Some("TRACEWISE_GIT_DEPTH") } else { None };
    tracewise_platform::process::fast_exec(target, args, arg0, None, env_remove);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let git_args = if args.len() > 1 { &args[1..] } else { &[] };

    // 1. Loop guard: if TRACEWISE_GIT_DEPTH is set, exec /usr/bin/git directly
    if std::env::var_os("TRACEWISE_GIT_DEPTH").is_some() {
        let fallback = Path::new("/usr/bin/git");
        let target = if fallback.is_file() { fallback } else { Path::new("git") };
        exec_or_run(target, git_args, None, false);
    }

    // 2. If read-only, exec real git and do nothing else
    if is_read_only_git(git_args) {
        let real_git = find_real_git(None);
        exec_or_run(&real_git, git_args, None, false);
    }

    // 3. Otherwise exec tracewise in wrapper mode with TRACEWISE_GIT_DEPTH unset. No `tracewise` found: fail open, run git unrecorded.
    match find_tracewise_bin() {
        Some(tracewise_bin) => exec_or_run(&tracewise_bin, git_args, Some("git"), true),
        None => exec_or_run(&find_real_git(None), git_args, None, false),
    }
}

#[cfg(test)]
#[path = "test_main.rs"]
mod tests;
