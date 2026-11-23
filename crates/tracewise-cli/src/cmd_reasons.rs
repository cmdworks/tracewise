//! Commands: Intent, Note, Lock, Run, Pending, AdoptCopy, Ps, Background, Doctor.
use super::*;

pub(crate) mod cmd_run;
pub(crate) mod intent;
pub(crate) mod lock;
pub(crate) mod misc;
pub(crate) mod note;

pub(crate) use cmd_run::*;
pub(crate) use intent::*;
pub(crate) use lock::*;
pub(crate) use misc::*;
pub(crate) use note::*;

// Uses tracewise_core::attribute_window(...) via cmd_run::run_cmd_run

pub(crate) fn run(json: bool, repo_paths: &RepoPaths, command: Commands) {
    match command {
        Commands::Note { list: true, session, .. } => run_intent(json, repo_paths, IntentAction::List { session }),
        Commands::Note { reason: Some(why), before, task, agent, session, .. } => {
            if before.is_empty() {
                eprintln!("tracewise: note --why needs the files it is about: --before <files>");
                process::exit(2);
            }
            run_intent(json, repo_paths, IntentAction::Add { why, task, agent, session, paths: before })
        }
        Commands::Note { last, change_id, why, label, auto, .. } => run_note(json, repo_paths, last, change_id, why, label || auto, auto),
        Commands::Lock { action } => run_lock(json, repo_paths, action),
        Commands::Run { why, command } => run_cmd_run(repo_paths, why, command),
        Commands::Pending => run_pending(json, repo_paths),
        Commands::Jobs { stop: None } => run_ps(json, repo_paths),
        Commands::Jobs { stop: Some(id) } => run_background(json, repo_paths, id),
        Commands::Doctor { verify, gc, rebuild, adopt_copy, .. } => {
            if !(verify || gc || rebuild || adopt_copy) {
                return run_doctor(json, repo_paths);
            }
            // each action asked for, in a fixed order
            if adopt_copy {
                run_adopt_copy(json, repo_paths);
            }
            if rebuild {
                crate::cmd_share::rebuild(json, repo_paths);
            }
            if gc {
                crate::cmd_history::run_gc(json, repo_paths);
            }
            if verify {
                crate::cmd_history::run_verify(json, repo_paths);
            }
        }
        _ => unreachable!("dispatched to the wrong group"),
    }
}
