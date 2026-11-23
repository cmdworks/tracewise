//! Commands: Init, Status, Log, Show, Scan, Blame, Why, Diff, Undo, Restore, Gc, Verify.
use super::*;

pub(crate) mod ignore;
pub(crate) mod inspect;
pub(crate) mod prune;
pub(crate) mod redo;
pub(crate) mod repo;
pub(crate) mod report;
pub(crate) mod revert;
#[cfg(test)]
mod tests;

pub(crate) use inspect::*;
pub(crate) use redo::*;
pub(crate) use repo::*;
pub(crate) use revert::*;

/// A path as the commands want it, relative to the project: an editor hands over an absolute one (in either spelling of the folder).
fn rel(repo_paths: &RepoPaths, path: &str) -> String {
    util::normalize_path(path, &repo_paths.work_tree)
}

/// `<path>:<line>` with the path made relative.
fn rel_target(repo_paths: &RepoPaths, target: &str) -> String {
    match target.rsplit_once(':') {
        Some((path, line)) if line.chars().all(|c| c.is_ascii_digit()) && !line.is_empty() => format!("{}:{line}", rel(repo_paths, path)),
        _ => target.to_string(),
    }
}

pub(crate) fn run(json: bool, repo_paths: &RepoPaths, command: Commands) {
    match command {
        Commands::Status { path: None, .. } => run_status(json, repo_paths),
        Commands::Status { path: Some(path), limit } => run_file(json, repo_paths, rel(repo_paths, &path), limit),
        Commands::Scan => run_scan(json, repo_paths),
        Commands::Record { source, paths } => run_record(json, repo_paths, source, paths),
        Commands::Ignore { patterns, prune, paths, older_than, dry_run, yes, backup } => {
            if patterns.is_empty() && !prune {
                eprintln!("tracewise: ignore needs patterns to add, or --prune");
                process::exit(2);
            }
            if !patterns.is_empty() {
                ignore::run_ignore(json, repo_paths, patterns);
            }
            if prune {
                prune::run_prune(json, repo_paths, prune::PruneArgs { paths, older_than, dry_run, yes, backup })
            }
        }
        Commands::Log { path, limit, actor, since, grep, after, group, md, intent, session, unfold, fold_after } => {
            util::require_valid_since(since.as_deref());
            if md {
                return report::run_report(json, repo_paths, revert::Target::new(actor, intent, session), since);
            }
            util::require_valid_since(Some(&fold_after).filter(|a| a.as_str() != "0").map(String::as_str));
            run_log(json, repo_paths, path.map(|p| rel(repo_paths, &p)), limit, inspect::LogFilter { actor, since, grep, after, group, unfold, fold_after })
        }
        Commands::Show { id } => run_show(json, repo_paths, id),
        Commands::Blame { path, at } => run_blame(json, repo_paths, rel(repo_paths, &path), at),
        Commands::Why { target } => run_why(json, repo_paths, rel_target(repo_paths, &target)),
        Commands::Diff { since, path } => {
            util::require_valid_since(since.as_deref());
            run_diff(json, repo_paths, since, path.map(|p| rel(repo_paths, &p)))
        }
        Commands::Undo { id, actor, intent, session, command, since, dry_run, strict, redo } => {
            if redo {
                let Some(id) = id else {
                    eprintln!("tracewise: undo --redo needs the id of the undo to redo");
                    process::exit(2);
                };
                return run_redo(json, repo_paths, id, dry_run);
            }
            util::require_valid_since(since.as_deref());
            run_undo(json, repo_paths, id, revert::Target::new(actor, intent, session).with_command(command), since, dry_run, strict)
        }
        Commands::Restore { path, at } => run_restore(json, repo_paths, rel(repo_paths, &path), at),
        _ => unreachable!("dispatched to the wrong group"),
    }
}
