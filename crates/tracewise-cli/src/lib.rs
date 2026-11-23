use clap::CommandFactory;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use tracewise_core::blob::BlobStore;
use tracewise_core::db::Database;
use tracewise_core::diff::format_unified_diff;
use tracewise_core::model::{validate_why, Actor, Change, CommandRecord, Intent, Window};
use tracewise_core::predict::predict_written_files;
use tracewise_core::{record_baseline, scan_repository, scan_specific_paths, scan_specific_paths_with_db, ScanOptions};
use tracewise_git::{get_git_info, walk_baseline_files, RepoPaths};
use util::*;

mod bulk;
mod capture_report;
mod capture_preset;
mod catch_up;
mod cli;
mod cmd_adapters;
mod cmd_agents;
mod cmd_capture;
mod cmd_config;
mod cmd_git_hooks;
mod cmd_history;
mod cmd_reasons;
mod cmd_repos;
mod cmd_share;
mod cmd_shims;
mod cmd_strict;
mod editors;
mod fast_shell;
mod hook;
mod lsp;
pub mod project;
mod self_heal;
mod setup;
mod shim_detect;
mod shim_install;
mod ui;
mod unhooked;
mod util;
mod wrapper;

pub use cli::*;
use fast_shell::{fast_shell_begin, fast_shell_end, try_fast_shell};

fn handle_early_commands(cli: &Cli) -> bool {
    if let Some(Commands::Hook { ref event, ref from, ref record, neutral }) = cli.command {
        hook::handle_hook(event, from.as_deref(), record.as_deref(), neutral);
        return true;
    }
    if let Some(Commands::Lsp) = cli.command {
        std::process::exit(lsp::run_lsp());
    }
    if let Some(Commands::Capture { ref action }) = cli.command {
        let env = setup::Env::current();
        let (presets, warnings) = capture_preset::load(&env.root.join("presets.toml"));
        let code = match action {
            None | Some(CaptureAction::Status) => {
                capture_preset::print_status(&env, &presets, &warnings, cli.json);
                0
            }
            Some(CaptureAction::List) => {
                capture_preset::print_list(&env, &presets, &warnings, cli.json);
                0
            }
            Some(CaptureAction::Daemon { action }) => {
                if !matches!(
                    action,
                    DaemonAction::Run | DaemonAction::Start | DaemonAction::Stop | DaemonAction::Status | DaemonAction::Install { .. } | DaemonAction::Uninstall
                ) {
                    eprintln!("tracewise: not a watcher command a person runs");
                    std::process::exit(2);
                }
                let here = std::env::current_dir().ok().and_then(|d| RepoPaths::discover(&d));
                cmd_capture::daemon_global(cli.json, action.clone(), here.as_ref());
                0
            }
            Some(CaptureAction::Sensor { action }) => {
                cmd_strict::run(cli.json, Commands::Strict { action: action.clone() });
                0
            }
            Some(CaptureAction::Repos) => {
                cmd_repos::run(cli.json);
                0
            }
            Some(CaptureAction::Use { preset, dry_run, yes }) => {
                capture_preset::run_use(&env, &presets, preset, *dry_run, *yes, cli.json, setup::live())
            }
        };
        std::process::exit(code);
    }
    if let Some(Commands::Editors { apply, ref key, ref dir, settings, ref when, after, every, hints, min_chars, min_lines, ref ignore, write, open, .. }) = cli.command {
        if settings {
            std::process::exit(editors::run_settings(editors::Settings { when: when.clone(), after, every, hints, min_chars, min_lines, ignore: ignore.clone() }, write, open, dir.clone()));
        }
        std::process::exit(editors::run_zed(apply, key, dir.clone()));
    }
    if let Some(Commands::Completions { ref shell }) = cli.command {
        use clap_complete::Shell;
        let shell = match shell.as_str() {
            "bash" => Shell::Bash,
            "zsh" => Shell::Zsh,
            "fish" => Shell::Fish,
            "elvish" => Shell::Elvish,
            _ => Shell::PowerShell,
        };
        clap_complete::generate(shell, &mut cli::command(), "tracewise", &mut std::io::stdout());
        return true;
    }
    if let Some(Commands::Companion) = cli.command {
        let info = serde_json::json!({
            "api": hook::neutral::API,
            "name": "tracewise",
            "version": env!("CARGO_PKG_VERSION"),
            "features": ["neutral-hook", "locks", "lock-list-json", "notices", "json-contract", "record"],
        });
        if cli.json {
            println!("{info}");
        } else {
            println!("tracewise {} (companion api {})", env!("CARGO_PKG_VERSION"), hook::neutral::API);
        }
        return true;
    }

    match cli.command {
        Some(Commands::Setup { ref login, no_start, no_shell_rc, strict, ref events, ref profile, ref capture, ref preset }) => {
            let login = match login.as_str() {
                "yes" => setup::Login::Yes,
                "no" => setup::Login::No,
                _ => setup::Login::Ask,
            };
            setup::run_setup(cli.json, login, no_start, no_shell_rc, strict, events, profile, capture, preset.as_deref());
            return true;
        }
        Some(Commands::Uninstall { purge, purge_histories }) => {
            setup::run_uninstall(cli.json, purge || purge_histories, purge_histories);
            return true;
        }
        Some(Commands::Doctor { all: true, .. }) => {
            setup::run_doctor_all(cli.json);
            return true;
        }
        _ => {}
    }

    if let Some(Commands::Daemon { ref action }) = cli.command {
        if matches!(
            action,
            DaemonAction::Run
                | DaemonAction::Start
                | DaemonAction::Stop
                | DaemonAction::Status
                | DaemonAction::Install { .. }
                | DaemonAction::Uninstall
        ) {
            let here = std::env::current_dir().ok().and_then(|d| RepoPaths::discover(&d));
            cmd_capture::daemon_global(cli.json, action.clone(), here.as_ref());
            return true;
        }
    }

    if let Some(Commands::Shell { action: ShellAction::Init { ref shell, non_interactive } }) = cli.command {
        handle_shell_init(shell, non_interactive);
        return true;
    }

    if let Some(Commands::GitWrapper { action: GitWrapperAction::Install { ref dir } }) = cli.command {
        install_git_wrapper(cli.json, dir);
        return true;
    }

    if let Some(Commands::Strict { .. }) = cli.command {
        if let Some(ref command) = cli.command {
            cmd_strict::run(cli.json, command.clone());
        }
        return true;
    }

    if let Some(Commands::Shims { .. }) = cli.command {
        if let Some(ref command) = cli.command {
            cmd_shims::run(cli.json, command.clone());
        }
        return true;
    }

    if let Some(Commands::Config { ref action }) = cli.command {
        let here = std::env::current_dir().ok().and_then(|d| RepoPaths::discover(&d));
        let project = here.as_ref().map(|r| r.work_tree.as_path());
        match action {
            ConfigAction::Get { key } => cmd_config::get(cli.json, key.clone(), project),
            ConfigAction::Set { key, value, global } => cmd_config::set(cli.json, key, value, *global, project),
        }
        return true;
    }

    if let Some(Commands::Init) = cli.command {
        init_anywhere(cli.json, cli.directory.as_deref());
        return true;
    }

    if let Some(Commands::Proc { ref action }) = cli.command {
        handle_proc_action(cli.json, action);
        return true;
    }

    false
}

/// `init` needs no existing project: the root is the nearest `.git` or marker above (a linked worktree's main checkout), else the
/// current folder.
fn init_anywhere(json: bool, directory: Option<&Path>) {
    if let Some(dir) = directory {
        if std::env::set_current_dir(dir).is_err() {
            eprintln!("tracewise: cannot enter {}", dir.display());
            process::exit(2);
        }
    }
    let cwd = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("tracewise: cannot read the current folder: {e}");
        process::exit(2);
    });
    let here = RepoPaths::discover(&cwd);
    // a linked worktree shares the main checkout's project, like `migrate_v1` does
    let root = match &here {
        Some(r) if tracewise_git::read_marker(&r.work_tree).is_none() => main_work_tree(r),
        Some(r) => r.work_tree.clone(),
        None => cwd.clone(),
    };
    let home = tracewise_platform::paths::tracewise_home_dir();
    if let Some(why) = project::init_refusal(&root, &cmd_agents::home_dir(), &home) {
        eprintln!("tracewise: {why}");
        process::exit(1);
    }
    if let Some(original) = here.as_ref().and_then(util::copy_of) {
        eprintln!(
            "tracewise: this folder is a copy of {} and shares its project; run `tracewise doctor --adopt-copy` to give it a project of its own",
            original.display()
        );
        process::exit(1);
    }
    let info = project::ensure_project(&root, &home).unwrap_or_else(|e| {
        eprintln!("tracewise: could not set up the project: {e}");
        process::exit(1);
    });
    let Some(repo_paths) = RepoPaths::discover(&cwd) else {
        eprintln!("tracewise: could not find the project at {}", root.display());
        process::exit(1);
    };
    cmd_history::repo::run_init(json, &repo_paths, &info);
}

fn handle_shell_init(shell: &str, non_interactive: bool) {
    let tracewise_bin = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "tracewise".to_string());

    if non_interactive {
        match shell {
            "zsh" => print!("{}", tracewise_shell::zsh_non_interactive(&tracewise_bin)),
            "bash" => print!("{}", tracewise_shell::bash_non_interactive(&tracewise_bin)),
            _ => {
                eprintln!("tracewise: unsupported shell '{}'", shell);
                process::exit(1);
            }
        }
        return;
    }

    match shell {
        "zsh" => print!("{}", tracewise_shell::zsh_interactive()),
        "fish" => print!("{}", tracewise_shell::fish_interactive()),
        "pwsh" | "powershell" => print!("{}", tracewise_platform::shell::pwsh_interactive()),
        "nu" | "nushell" => print!("{}", tracewise_platform::shell::nu_interactive()),
        _ => {
            eprintln!("tracewise: unsupported shell '{}'", shell);
            process::exit(1);
        }
    }
}

fn install_git_wrapper(json: bool, dir: &Path) {
    let _ = fs::create_dir_all(dir);
    let symlink_path = dir.join("git");
    let _ = fs::remove_file(&symlink_path);

    let self_exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("tracewise: failed to get current executable: {e}");
            process::exit(1);
        }
    };

    let target_bin = self_exe.parent().map(|p| p.join("tracewise-git")).filter(|p| p.is_file()).unwrap_or_else(|| self_exe.clone());

    if let Err(e) = tracewise_platform::shims::create_shim_link(&target_bin, &symlink_path) {
        eprintln!("tracewise: failed to create symlink: {e}");
        process::exit(1);
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "installed": true,
                "path": symlink_path.to_string_lossy(),
            })
        );
    } else {
        println!("Installed git wrapper to {}", symlink_path.display());
    }
}

fn handle_proc_action(json: bool, action: &ProcAction) {
    match action {
        ProcAction::Chain { pid } => {
            let target_pid = pid.unwrap_or_else(tracewise_proc::parent_pid);
            let chain = tracewise_proc::process_chain(target_pid);
            if json {
                println!("{}", serde_json::to_string(&chain).unwrap_or_else(|_| "[]".to_string()));
            } else {
                for p in &chain {
                    println!("{:>7} {:>7} {:<16} {}", p.pid, p.ppid, p.name, p.path);
                }
            }
        }
        ProcAction::Who { pid } => {
            let res = tracewise_proc::who(*pid, None);
            if json {
                println!("{}", serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string()));
            } else {
                let match_info = match &res.matched {
                    Some(m) => format!(" (process: {} {})", m.pid, m.name),
                    None => String::new(),
                };
                println!("{}:{} ({}){}", res.kind, res.name, res.source, match_info);
            }
        }
    }
}

/// Commands that only show the history: they first catch up on what changed that nothing recorded live.
fn reads_history(command: &Commands) -> bool {
    matches!(
        command,
        Commands::Status { .. }
            | Commands::Log { .. }
            | Commands::Show { .. }
            | Commands::Blame { .. }
            | Commands::Why { .. }
            | Commands::Diff { .. }
            | Commands::Pending
    )
}

/// Commands that read or change the history: without `tracewise init` they have nothing to work on.
fn needs_history(command: &Commands) -> bool {
    matches!(
        command,
        Commands::Log { .. }
            | Commands::Record { .. }
            | Commands::Show { .. }
            | Commands::Blame { .. }
            | Commands::Why { .. }
            | Commands::Diff { .. }
            | Commands::Undo { .. }
            | Commands::Restore { .. }
            | Commands::Doctor { verify: true, .. }
            | Commands::Doctor { gc: true, .. }
            | Commands::Ignore { prune: true, .. }
            | Commands::Pending
            | Commands::Note { .. }
            | Commands::Lock { .. }
    )
}

fn dispatch_repo_command(json: bool, repo_paths: &RepoPaths, command: Commands) {
    if needs_history(&command) && !repo_paths.data_dir().join("tracewise.db").is_file() {
        eprintln!("tracewise: this repository has no Tracewise history yet; run `tracewise init` first");
        process::exit(1);
    }
    if reads_history(&command) {
        catch_up::catch_up(repo_paths, catch_up::Trigger::Command);
    }
    match command {
        Commands::Status { .. }
        | Commands::Log { .. }
        | Commands::Show { .. }
        | Commands::Scan
        | Commands::Record { .. }
        | Commands::Blame { .. }
        | Commands::Why { .. }
        | Commands::Diff { .. }
        | Commands::Undo { .. }
        | Commands::Restore { .. }
        | Commands::Ignore { .. }
        => cmd_history::run(json, repo_paths, command),
        Commands::Daemon { .. } | Commands::Shell { .. } | Commands::CommandHistory | Commands::GitWrapper { .. } => {
            cmd_capture::run(json, repo_paths, command)
        }
        Commands::Doctor { .. }
        | Commands::Note { .. }
        | Commands::Lock { .. }
        | Commands::Jobs { .. }
        | Commands::Run { .. }
        | Commands::Pending => cmd_reasons::run(json, repo_paths, command),
        Commands::Share { .. } => cmd_share::run(json, repo_paths, command),
        Commands::Setup { .. }
        | Commands::Uninstall { .. }
        | Commands::Shims { .. }
        | Commands::Config { .. }
        | Commands::Init => {
            unreachable!("handled before repository lookup")
        }
        Commands::GitHooks { action } => cmd_git_hooks::run(json, repo_paths, &action),
        Commands::GitHook { name, args } => cmd_git_hooks::run_hook(repo_paths, &name, &args),
        Commands::Agent { action } => match action {
            AgentAction::Install { name, scope } => {
                if let Err(e) = cmd_agents::handle_agent_install(repo_paths, &name, &scope, &cmd_agents::home_dir(), json) {
                    eprintln!("tracewise: {e}");
                    process::exit(1);
                }
            }
            AgentAction::Uninstall { name, scope } => {
                let _ = cmd_agents::handle_agent_uninstall(repo_paths, &name, &scope, &cmd_agents::home_dir(), json);
            }
            AgentAction::Status => {
                let _ = cmd_agents::handle_agent_status(repo_paths, json);
            }
            AgentAction::List => {
                let _ = cmd_adapters::handle_agent_list(repo_paths, json);
            }
            AgentAction::Check { file, payload } => {
                let _ = cmd_adapters::handle_agent_check(&file, &payload, json);
            }
        },
        Commands::Hook { .. } | Commands::Lsp | Commands::Completions { .. } | Commands::Capture { .. } | Commands::Editors { .. } | Commands::Companion | Commands::Proc { .. } | Commands::Strict { .. } => {
            unreachable!()
        }
    }
}

/// The whole command line; `tracewise` and its old name `tracewise` both run this.
pub fn main() {
    tracewise_platform::signal::reset_sigpipe();
    if cmd_shims::try_fast_read_only_shim() {
        return;
    }

    let os_args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let shim_tool = os_args.first().and_then(|a| Path::new(a).file_name()?.to_str()).filter(|n| cmd_shims::SHIM_TOOLS.contains(n));
    if let Some(tool) = shim_tool {
        tracewise_shim::exec_if_non_utf8(tool, &os_args[1..], Path::new(&os_args[0]).parent());
    }
    let args: Vec<String> = std::env::args().collect();
    let bin_name = args.first().and_then(|a| Path::new(a).file_name().and_then(|n| n.to_str())).unwrap_or("");
    if bin_name == "git" {
        wrapper::run_git_wrapper(&args[1..]);
        return;
    }

    if cmd_shims::SHIM_TOOLS.contains(&bin_name) {
        cmd_shims::run_shim(bin_name, &args[1..]);
    }

    if try_fast_shell(&args) {
        return;
    }

    let cli = parse_cli();

    if handle_early_commands(&cli) {
        return;
    }

    if let Some(ref dir) = cli.directory {
        if std::env::set_current_dir(dir).is_err() {
            eprintln!("tracewise: not a Tracewise project (no .git or .tracewiseproject here or above); run `tracewise init`");
            process::exit(2);
        }
    }

    let current_dir = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => {
            eprintln!("tracewise: not a Tracewise project (no .git or .tracewiseproject here or above); run `tracewise init`");
            process::exit(2);
        }
    };

    let repo_paths = match RepoPaths::discover(&current_dir) {
        Some(paths) => paths,
        None => {
            eprintln!("tracewise: not a Tracewise project (no .git or .tracewiseproject here or above); run `tracewise init`");
            process::exit(2);
        }
    };

    // agent hooks and the git wrapper returned earlier; git hooks, the daemon's helpers and every command the daemon starts
    // (TRACEWISE_NO_MIGRATE, its stderr goes to daemon.log) run inside other programs: none of them move data, so they stay fast
    // and never print the recovery secret
    let by_a_person = !matches!(cli.command, Some(Commands::Daemon { .. } | Commands::GitHook { .. }))
        && std::env::var_os("TRACEWISE_NO_MIGRATE").is_none();
    match by_a_person.then(|| project::migrate_v1(&repo_paths, &tracewise_platform::paths::tracewise_home_dir())) {
        Some(Ok(Some(info))) => {
            tracewise_core::repos::wake_daemon();
            if let Some(secret) = info.recovery_secret {
                eprintln!("tracewise: this project's history moved to {}", info.data_dir.display());
                eprintln!("Recovery secret: {secret}");
                eprintln!("Keep it in a password manager; it is shown only this once.");
            }
        }
        Some(Err(e)) => eprintln!("tracewise: could not move this project's history out of .git: {e}"),
        _ => {}
    }

    let db_path = repo_paths.data_dir().join("tracewise.db");
    if db_path.exists() {
        if let Ok(db) = Database::open(&db_path) {
            let _ = db.import_spool(&repo_paths.data_dir());
            let _ = db.clean_expired_locks(&repo_paths.work_tree);
        }
    }

    if !matches!(cli.command, Some(Commands::Doctor { adopt_copy: true, .. })) {
        if let Some(original) = util::copy_of(&repo_paths) {
            eprintln!(
                "tracewise: this folder is a copy of {} (same tracewise id); its history is separate from now on. \
                 Run `tracewise doctor --adopt-copy` to give it its own id before pushing.",
                original.display()
            );
        }
    }

    let json = cli.json;
    match cli.command {
        Some(command) => dispatch_repo_command(json, &repo_paths, command),
        None => {
            let _ = Cli::command().print_help();
            println!();
        }
    }
}

#[cfg(test)]
pub static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
#[path = "test_main.rs"]
mod tests;

fn parse_cli() -> Cli {
    use clap::FromArgMatches;
    Cli::from_arg_matches(&cli::command().get_matches()).unwrap_or_else(|e| e.exit())
}
