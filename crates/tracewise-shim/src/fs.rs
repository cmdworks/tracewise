use std::path::PathBuf;

use crate::{operands, WriteTargets};

/// `Paths(files)`, or `when_empty` when there are none.
fn paths_or(files: Vec<PathBuf>, when_empty: WriteTargets) -> WriteTargets {
    if files.is_empty() {
        when_empty
    } else {
        WriteTargets::Paths(files)
    }
}

/// The file operands of a tool whose options may take a value: `value_flags` (`-s`, `--size`) take the next argument, a letter
/// glued to its value (`-s10`) and any other option are skipped, and everything after `--` is a file.
fn files_skipping_option_values(args: &[String], value_flags: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            files.extend(args[i + 1..].iter().map(PathBuf::from));
            break;
        }
        if value_flags.contains(&a.as_str()) {
            i += 2;
        } else if a.starts_with('-') {
            // covers `-s10` too: any option that is not a bare value flag takes no following argument
            i += 1;
        } else {
            files.push(PathBuf::from(a));
            i += 1;
        }
    }
    files
}

/// The operands as sources and the last one as the destination; None with fewer than two.
fn sources_and_dest(args: &[String]) -> Option<(Vec<PathBuf>, PathBuf)> {
    let non_flags: Vec<&String> = operands(args).into_iter().collect();
    let (dest, sources) = non_flags.split_last().filter(|(_, s)| !s.is_empty())?;
    Some((sources.iter().map(|s| PathBuf::from(*s)).collect(), PathBuf::from(*dest)))
}

pub fn detect_tee_targets(args: &[String]) -> WriteTargets {
    paths_or(operands(args).into_iter().map(PathBuf::from).collect(), WriteTargets::None)
}

/// Where `sources` end up: inside `dest` when it is a folder, else `dest` itself.
fn into_dir(sources: &[PathBuf], dest: PathBuf) -> Vec<PathBuf> {
    if dest.is_dir() {
        sources.iter().filter_map(|s| s.file_name().map(|f| dest.join(f))).collect()
    } else {
        vec![dest]
    }
}

pub fn detect_mv_targets(args: &[String]) -> WriteTargets {
    let Some((sources, dest)) = sources_and_dest(args) else { return WriteTargets::AnyInCwd };
    let mut targets = into_dir(&sources, dest);
    targets.extend(sources);
    WriteTargets::Paths(targets)
}

pub fn detect_cp_targets(args: &[String]) -> WriteTargets {
    let Some((sources, dest)) = sources_and_dest(args) else { return WriteTargets::AnyInCwd };
    WriteTargets::Paths(into_dir(&sources, dest))
}

pub fn detect_ln_targets(args: &[String]) -> WriteTargets {
    if let Some((sources, dest)) = sources_and_dest(args) {
        return WriteTargets::Paths(into_dir(&sources, dest));
    }
    // one operand: the link is made in the current folder under the target's name
    match operands(args).first().and_then(|a| PathBuf::from(*a).file_name().map(PathBuf::from)) {
        Some(name) => WriteTargets::Paths(vec![name]),
        None => WriteTargets::AnyInCwd,
    }
}

pub fn detect_rm_targets(args: &[String]) -> WriteTargets {
    paths_or(operands(args).into_iter().map(PathBuf::from).collect(), WriteTargets::AnyInCwd)
}

pub fn detect_truncate_targets(args: &[String]) -> WriteTargets {
    paths_or(files_skipping_option_values(args, &["-s", "-r", "--size", "--reference"]), WriteTargets::AnyInCwd)
}

pub fn detect_chflags_targets(args: &[String]) -> WriteTargets {
    // First operand is the flag spec, the rest are files.
    paths_or(operands(args).into_iter().skip(1).map(PathBuf::from).collect(), WriteTargets::None)
}

pub fn detect_xattr_targets(args: &[String]) -> WriteTargets {
    let flags: String = args.iter().filter(|a| a.starts_with('-') && !a.starts_with("--")).flat_map(|a| a[1..].chars()).collect();
    let skip = if flags.contains('w') {
        2
    } else if flags.contains('d') {
        1
    } else if flags.contains('c') {
        0
    } else {
        return WriteTargets::None;
    };
    paths_or(operands(args).into_iter().skip(skip).map(PathBuf::from).collect(), WriteTargets::None)
}

pub fn detect_touch_targets(args: &[String]) -> WriteTargets {
    paths_or(files_skipping_option_values(args, &["-d", "-t", "-r", "--date", "--reference"]), WriteTargets::AnyInCwd)
}

/// `cp` and `mv`: the source operands and the destination (the last operand). None for any other tool or fewer
/// than two operands.
pub fn copy_operands(tool: &str, args: &[String]) -> Option<(Vec<String>, String)> {
    if tool != "cp" && tool != "mv" {
        return None;
    }
    let ops = operands(args);
    if ops.len() < 2 {
        return None;
    }
    let (dest, sources) = ops.split_last()?;
    Some((sources.iter().map(|s| s.to_string()).collect(), dest.to_string()))
}

pub fn detect_fs_targets(tool: &str, args: &[String]) -> Option<WriteTargets> {
    match tool {
        "tee" => Some(detect_tee_targets(args)),
        "mv" => Some(detect_mv_targets(args)),
        "cp" => Some(detect_cp_targets(args)),
        "ln" => Some(detect_ln_targets(args)),
        "rm" => Some(detect_rm_targets(args)),
        "truncate" => Some(detect_truncate_targets(args)),
        "chflags" => Some(detect_chflags_targets(args)),
        "xattr" => Some(detect_xattr_targets(args)),
        "touch" => Some(detect_touch_targets(args)),
        _ => None,
    }
}
