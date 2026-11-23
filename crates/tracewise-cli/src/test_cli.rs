use super::*;

#[test]
fn test_cli_parse() {
    let cli = Cli::parse_from(["tracewise", "status"]);
    assert!(!cli.json);
}

#[test]
fn test_cli_shims_parse() {
    let cli = Cli::parse_from(["tracewise", "shims", "install", "--dir", "/tmp/shims", "--json"]);
    assert!(cli.json);
    match cli.command {
        Some(Commands::Shims { action: ShimsAction::Install { dir } }) => {
            assert_eq!(dir, PathBuf::from("/tmp/shims"));
        }
        _ => panic!("expected Commands::Shims::Install"),
    }

    let cli2 = Cli::parse_from(["tracewise", "shims", "uninstall", "--dir", "/tmp/shims"]);
    assert!(!cli2.json);
    match cli2.command {
        Some(Commands::Shims { action: ShimsAction::Uninstall { dir } }) => {
            assert_eq!(dir, PathBuf::from("/tmp/shims"));
        }
        _ => panic!("expected Commands::Shims::Uninstall"),
    }
}

#[test]
fn test_cli_pending_parse() {
    let cli = Cli::parse_from(["tracewise", "pending", "--json"]);
    assert!(cli.json);
    match cli.command {
        Some(Commands::Pending) => {}
        _ => panic!("expected Commands::Pending"),
    }
}

#[test]
fn test_cli_note_last_parse() {
    let cli = Cli::parse_from(["tracewise", "note", "--last", "document release procedure", "--json"]);
    assert!(cli.json);
    match cli.command {
        Some(Commands::Note { last, change_id, why, .. }) => {
            assert_eq!(last.as_deref(), Some("document release procedure"));
            assert!(change_id.is_none());
            assert!(why.is_none());
        }
        _ => panic!("expected Commands::Note"),
    }
}

#[test]
fn test_cli_run_parse() {
    let cli = Cli::parse_from(["tracewise", "run", "--why", "implement user login flow", "--", "echo", "hello", "world"]);
    match cli.command {
        Some(Commands::Run { why, command }) => {
            assert_eq!(why, "implement user login flow");
            assert_eq!(command, vec!["echo", "hello", "world"]);
        }
        _ => panic!("expected Commands::Run"),
    }
}

#[test]
fn test_cli_proc_chain_parse() {
    let cli = Cli::parse_from(["tracewise", "proc", "chain", "--json"]);
    assert!(cli.json);
    match cli.command {
        Some(Commands::Proc { action: ProcAction::Chain { pid } }) => {
            assert_eq!(pid, None);
        }
        _ => panic!("expected Commands::Proc::Chain"),
    }
}

#[test]
fn test_cli_proc_who_parse() {
    let cli = Cli::parse_from(["tracewise", "proc", "who", "--json"]);
    assert!(cli.json);
    match cli.command {
        Some(Commands::Proc { action: ProcAction::Who { pid } }) => {
            assert_eq!(pid, None);
        }
        _ => panic!("expected Commands::Proc::Who"),
    }
}

#[test]
fn test_cli_agent_parse() {
    let cli = Cli::parse_from(["tracewise", "agent", "install", "claude", "--json"]);
    assert!(cli.json);
    match cli.command {
        Some(Commands::Agent { action: AgentAction::Install { name, scope } }) => {
            assert_eq!(name, "claude");
            assert_eq!(scope, "project");
        }
        _ => panic!("expected Commands::Agent::Install"),
    }
}

#[test]
fn test_cli_hook_from_parse() {
    let cli = Cli::parse_from(["tracewise", "hook", "pre-edit", "--from", "claude", "--record", "/tmp/rec"]);
    match cli.command {
        Some(Commands::Hook { event, from, record, neutral }) => {
            assert_eq!(event, "pre-edit");
            assert_eq!(from.as_deref(), Some("claude"));
            assert_eq!(record, Some(PathBuf::from("/tmp/rec")));
            assert!(!neutral, "the neutral answer is opt-in");
        }
        _ => panic!("expected Commands::Hook"),
    }
    let cli = Cli::parse_from(["tracewise", "hook", "--from", "codex", "--neutral"]);
    assert!(matches!(cli.command, Some(Commands::Hook { neutral: true, .. })));
}

#[test]
fn test_cli_shell_begin_end_parse() {
    let cli_begin = Cli::parse_from(["tracewise", "shell", "begin", "--pid", "12345", "--", "cargo", "test"]);
    match cli_begin.command {
        Some(Commands::Shell { action: ShellAction::Begin { pid, command } }) => {
            assert_eq!(pid, 12345);
            assert_eq!(command, vec!["cargo", "test"]);
        }
        _ => panic!("expected ShellAction::Begin"),
    }

    let cli_end = Cli::parse_from(["tracewise", "shell", "end", "--pid", "12345", "--exit", "0"]);
    match cli_end.command {
        Some(Commands::Shell { action: ShellAction::End { pid, exit } }) => {
            assert_eq!(pid, 12345);
            assert_eq!(exit, 0);
        }
        _ => panic!("expected ShellAction::End"),
    }
}

#[test]
fn test_cli_undo_redo_parse() {
    let cli_undo = Cli::parse_from(["tracewise", "undo", "--actor", "codex", "--since", "1h", "--dry-run"]);
    match cli_undo.command {
        Some(Commands::Undo { id, actor, since, dry_run, strict, .. }) => {
            assert_eq!(id, None);
            assert_eq!(actor.as_deref(), Some("codex"));
            assert_eq!(since.as_deref(), Some("1h"));
            assert!(dry_run);
            assert!(!strict);
        }
        _ => panic!("expected Commands::Undo"),
    }

    let cli_undo_id = Cli::parse_from(["tracewise", "undo", "01M5T3BNN6CDA1X5BEX9QGDD4R", "--strict"]);
    match cli_undo_id.command {
        Some(Commands::Undo { id, actor, since, dry_run, strict, .. }) => {
            assert_eq!(id.as_deref(), Some("01M5T3BNN6CDA1X5BEX9QGDD4R"));
            assert_eq!(actor, None);
            assert_eq!(since, None);
            assert!(!dry_run);
            assert!(strict);
        }
        _ => panic!("expected Commands::Undo"),
    }

    let cli_redo = Cli::parse_from(["tracewise", "undo", "--redo", "01M5T3BNN6CDA1X5BEX9QGDD4R", "--dry-run"]);
    match cli_redo.command {
        Some(Commands::Undo { id, dry_run, redo, .. }) => {
            assert_eq!(id.as_deref(), Some("01M5T3BNN6CDA1X5BEX9QGDD4R"));
            assert!(dry_run && redo);
        }
        _ => panic!("expected Commands::Undo"),
    }
}

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["tracewise"];
    full.extend_from_slice(args);
    Cli::try_parse_from(&full).unwrap_or_else(|e| panic!("{args:?} does not parse: {e}"))
}

#[test]
fn the_definition_has_no_duplicate_name_alias_or_letter() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}

#[test]
fn every_command_has_its_short_name() {
    let names = [
        ("i", "init"),
        ("st", "status"),
        ("dr", "doctor"),
        ("l", "log"),
        ("sw", "show"),
                        ("sc", "scan"),
        ("rec", "record"),
        ("b", "blame"),
        ("w", "why"),
        ("d", "diff"),
        ("u", "undo"),
        ("rs", "restore"),
        ("n", "note"),
        ("r", "run"),
        ("p", "pending"),
        ("cfg", "config"),
        ("dm", "daemon"),
    ];
    use clap::CommandFactory;
    let root = Cli::command();
    for (short, long) in names {
        let cmd = root.find_subcommand(long).unwrap_or_else(|| panic!("no command {long}"));
        assert!(cmd.get_all_aliases().any(|a| a == short), "{long} has no alias {short}");
    }
}

#[test]
fn the_short_flags_mean_the_same_as_the_long_ones() {
    let long = parse(&["log", "--limit", "5", "--actor", "claude", "--since", "1h", "--grep", "fix", "--group"]);
    let short = parse(&["l", "-l", "5", "-a", "claude", "-s", "1h", "-g", "fix", "-G"]);
    assert_eq!(format!("{:?}", long.command), format!("{:?}", short.command));

    let long = parse(&["undo", "--actor", "claude", "--intent", "i1", "--session", "s1", "--command", "c1", "--since", "1h", "--dry-run"]);
    let short = parse(&["u", "-a", "claude", "-i", "i1", "-S", "s1", "-c", "c1", "-s", "1h", "-n"]);
    assert_eq!(format!("{:?}", long.command), format!("{:?}", short.command));

    let long = parse(&["ignore", "--prune", "--path", "build/**", "--older-than", "90d", "--dry-run", "--yes", "--backup", "b.db"]);
    let short = parse(&["ignore", "--prune", "-p", "build/**", "-o", "90d", "-n", "-y", "-b", "b.db"]);
    assert_eq!(format!("{:?}", long.command), format!("{:?}", short.command));

    for (long, short) in [
        (&["blame", "a.txt", "--at", "X"][..], &["b", "a.txt", "-t", "X"][..]),
        (&["restore", "a.txt", "--at", "X"], &["rs", "a.txt", "-t", "X"]),
        (&["undo", "--redo", "X", "--dry-run"], &["u", "-r", "X", "-n"]),
        (&["diff", "--since", "1h"], &["d", "-s", "1h"]),
        (
            &["log", "--md", "--actor", "a", "--intent", "i", "--session", "s", "--since", "1h"],
            &["l", "--md", "-a", "a", "-i", "i", "-S", "s", "-s", "1h"],
        ),
        (&["doctor", "--all"], &["dr", "-a"]),
        (&["status", "a.txt", "--limit", "3"], &["st", "a.txt", "-l", "3"]),
    ] {
        assert_eq!(format!("{:?}", parse(long).command), format!("{:?}", parse(short).command), "{long:?} vs {short:?}");
    }
    assert!(parse(&["status", "-j"]).json && parse(&["l", "-j"]).json);
}

#[test]
fn a_reason_is_given_as_note_why_or_m() {
    let reasons = |args: &[&str]| match parse(args).command {
        Some(Commands::Run { why, command }) => (why, command),
        Some(Commands::Note { reason: Some(why), before, .. }) => (why, before),
        other => panic!("{other:?}"),
    };
    for flag in [&["--note", "fix it"][..], &["--why", "fix it"], &["-m", "fix it"]] {
        let mut run = vec!["run"];
        run.extend_from_slice(flag);
        run.extend(["--", "cargo", "test"]);
        assert_eq!(reasons(&run), ("fix it".to_string(), vec!["cargo".to_string(), "test".to_string()]), "{flag:?}");
        let mut intent = vec!["n"];
        intent.extend_from_slice(flag);
        intent.extend(["--before", "a.txt"]);
        assert_eq!(reasons(&intent), ("fix it".to_string(), vec!["a.txt".to_string()]), "{flag:?}");
    }
}

#[test]
fn setup_capture_flag_values() {
    let capture = |args: &[&str]| match parse(args).command {
        Some(Commands::Setup { capture, .. }) => capture,
        other => panic!("{other:?}"),
    };
    assert_eq!(capture(&["setup"]), "ask");
    assert_eq!(capture(&["setup", "--capture", "agents"]), "agents");
    assert_eq!(capture(&["setup", "--capture", "full"]), "full");
    assert!(Cli::try_parse_from(["tracewise", "setup", "--capture", "sometimes"]).is_err());
}

#[test]
fn the_command_list_stays_short_and_the_merged_commands_are_gone() {
    use clap::CommandFactory;
    let root = Cli::command();
    let visible: Vec<&str> = root.get_subcommands().filter(|c| !c.is_hide_set()).map(|c| c.get_name()).collect();
    assert!(visible.len() <= 25, "{} commands in --help: {visible:?}", visible.len());
    // the jobs of these commands moved into another command (see docs/commands.md); they are not kept under the old name
    for gone in ["redo", "ps", "background", "prune", "file", "report", "intent", "verify", "gc", "rebuild", "adopt-copy", "push", "fetch", "repos"] {
        assert!(root.find_subcommand(gone).is_none(), "`{gone}` was merged into another command");
    }
    // what programs call stays, but out of --help
    for internal in ["hook", "git-hook", "git-wrapper", "shell", "proc", "companion", "lsp", "commands", "record", "daemon", "strict", "shims", "git-hooks"] {
        let cmd = root.find_subcommand(internal).unwrap_or_else(|| panic!("`{internal}` is called by installed programs"));
        assert!(cmd.is_hide_set(), "`{internal}` is an internal entry point, not a command to list");
    }
}

#[test]
fn the_help_lists_every_visible_command_in_a_group() {
    let root = command();
    let help = root.clone().render_help().to_string();
    assert!(!help.contains("\nMore\n"), "a visible command is in no group (add it to GROUPS in cli.rs):\n{help}");
    for c in root.get_subcommands().filter(|c| !c.is_hide_set() && c.get_name() != "help") {
        assert!(help.contains(c.get_name()), "`{}` is missing from --help", c.get_name());
    }
    for (title, _) in GROUPS {
        assert!(help.contains(title), "{title}");
    }
}

#[test]
fn every_help_wraps_to_the_terminal_width() {
    let width = crate::ui::terminal_width();
    let root = command();
    for sub in root.get_subcommands().filter(|c| !c.is_hide_set()) {
        let help = sub.clone().render_long_help().to_string();
        let too_wide: Vec<&str> = help.lines().filter(|l| l.chars().count() > width).collect();
        assert!(too_wide.is_empty(), "`{}` help is wider than {width}: {too_wide:?}", sub.get_name());
    }
}
