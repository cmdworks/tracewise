use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// How `--help` groups the commands a person types, with one short line for each. A visible command that is in no group is listed
/// under "More" with the first line of its own description (a test fails when that happens, so the groups stay complete).
const GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Get started",
        &[
            ("init", "Start recording this folder"),
            ("setup", "Install Tracewise for your user (no sudo)"),
            ("uninstall", "Remove what setup installed (project histories stay)"),
            ("status", "This project at a glance; with a path, who holds a file and who changed it"),
            ("doctor", "Check this project, or with --all the whole installation"),
        ],
    ),
    (
        "Read history",
        &[
            ("log", "What changed, newest first"),
            ("show", "One change with its diff"),
            ("blame", "Who last changed each line, and why"),
            ("why", "The full story of one line"),
            ("diff", "What changed since a time, as a diff"),
            ("scan", "Look for changes now"),
        ],
    ),
    (
        "Say why",
        &[("note", "Say why, after the fact or before editing"), ("run", "Run a command under a reason"), ("pending", "Changes that have no reason yet")],
    ),
    ("Go back", &[("undo", "Take back a change, a task or an actor's work (--redo puts it back)"), ("restore", "Put a whole file back as it was")]),
    (
        "Choose what records",
        &[
            ("capture", "What records live: terminal, watcher, and presets of them"),
            ("agent", "Connect coding agents (hooks and skills)"),
            ("editors", "Set up an editor (Zed)"),
            ("ignore", "Stop recording paths; --prune removes their history"),
        ],
    ),
    (
        "Team and coordination",
        &[("share", "Share history between machines"), ("lock", "See and release file locks"), ("jobs", "Background work that agents started")],
    ),
    ("Settings", &[("config", "Read or change settings"), ("completions", "Tab completion for your shell")]),
];

/// The colours of `--help`: clap leaves them out when the output is not a terminal or `NO_COLOR` is set.
fn styles() -> clap::builder::Styles {
    use clap::builder::styling::{Ansi256Color, AnsiColor, Color, Style};
    let brand = Style::new().bold().fg_color(Some(Color::Ansi256(Ansi256Color(166))));
    clap::builder::Styles::styled()
        .header(brand)
        .usage(brand)
        .literal(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Cyan))))
        .placeholder(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green))))
        .valid(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green))))
        .invalid(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow))))
        .error(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Red))))
}

/// The commands, grouped by what they are for, one line each (name, short name, what it does), then the "Options" heading.
fn grouped_listing(root: &clap::Command) -> String {
    use crate::ui::{paint_with, ACCENT, CYAN, DIM};
    let visible: Vec<&clap::Command> = root.get_subcommands().filter(|c| !c.is_hide_set() && c.get_name() != "help").collect();
    // the description starts after "  name        alias ": it wraps under itself instead of under the names
    let indent = 2 + 10 + 1 + 4 + 1;
    let text_width = crate::ui::terminal_width().saturating_sub(indent + 1).max(20);
    let line = |name: &str, about: &str| {
        let alias = visible.iter().find(|c| c.get_name() == name).and_then(|c| c.get_visible_aliases().next()).unwrap_or("");
        let wrapped = crate::ui::wrap(about, text_width).join(&format!("\n{}", " ".repeat(indent)));
        format!("  {} {} {wrapped}\n", paint_with(true, CYAN, &format!("{name:<10}")), paint_with(true, DIM, &format!("{alias:<4}")))
    };
    let mut out = String::new();
    let mut listed: Vec<&str> = Vec::new();
    for (title, entries) in GROUPS {
        let members: Vec<&(&str, &str)> = entries.iter().filter(|(n, _)| visible.iter().any(|c| c.get_name() == *n)).collect();
        if members.is_empty() {
            continue;
        }
        out.push_str(&format!("{}\n", paint_with(true, ACCENT, title)));
        for (name, about) in members {
            out.push_str(&line(name, about));
            listed.push(name);
        }
        out.push('\n');
    }
    let more: Vec<&&clap::Command> = visible.iter().filter(|c| !listed.contains(&c.get_name())).collect();
    if !more.is_empty() {
        out.push_str(&format!("{}\n", paint_with(true, ACCENT, "More")));
        for c in more {
            let about = c.get_about().map(|a| a.to_string()).unwrap_or_default();
            out.push_str(&line(c.get_name(), about.lines().next().unwrap_or("")));
        }
        out.push('\n');
    }
    out.push_str(&format!("{}\n", paint_with(true, ACCENT, "Options")));
    for (flag, about) in [
        ("-C <DIR>", "Run as if started in another folder"),
        ("-j, --json", "Answer in JSON, for scripts and tools"),
        ("-h, --help", "Show help (also `<command> --help`)"),
        ("-V, --version", "Show the version"),
    ] {
        out.push_str(&format!("  {} {about}\n", paint_with(true, CYAN, &format!("{flag:<14}"))));
    }
    out
}

/// `width` as the wrapping width of `cmd` and every command below it (clap sets it on one command only).
fn with_width(cmd: clap::Command, width: usize) -> clap::Command {
    // clap wraps the options but leaves the description at the top as written, so that is wrapped here
    let wrapped = |text: &clap::builder::StyledStr| crate::ui::wrap(&text.to_string().replace('\n', " "), width).join("\n");
    let about = cmd.get_about().map(wrapped);
    let long_about = cmd.get_long_about().map(wrapped);
    let mut cmd = cmd.term_width(width);
    if let Some(a) = about {
        cmd = cmd.about(a);
    }
    if let Some(a) = long_about {
        cmd = cmd.long_about(a);
    }
    cmd.mut_subcommands(|sub| with_width(sub, width))
}

/// The command line definition with the grouped, coloured `--help`.
pub fn command() -> clap::Command {
    use clap::CommandFactory;
    let base = Cli::command();
    let listing = grouped_listing(&base);
    let width = crate::ui::terminal_width();
    with_width(base, width)
        .styles(styles())
        .help_template("{about}\n\n{usage-heading} {usage}\n\n{before-help}{after-help}")
        .before_help(listing)
        .after_help(
            "Run `tracewise <command> --help` for a command's options and examples.\n\
             Also answers to `tw`, `trace` and `tracwise`. Documentation: https://cmdworks.github.io/tracewise/",
        )
}

#[derive(Parser, Debug, Clone)]
#[command(name = "tracewise", version, about = "Tracewise: a flight recorder for code changes by people and coding agents")]
pub struct Cli {
    #[arg(short = 'C', help = "Run as if tracewise was started in <dir> instead of the current working directory")]
    pub directory: Option<PathBuf>,
    #[arg(short = 'j', long, global = true, help = "Output in JSON format")]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start recording the project here: the nearest folder above with .git or .tracewiseproject, else this folder
    #[command(visible_alias = "i")]
    Init,
    /// Show the status of the project, or with a path who holds that file and who changed it lately
    #[command(after_help = "Examples:\n  tracewise status\n  tracewise status src/main.rs\n  tracewise status src/main.rs --limit 3 --json")]
    #[command(visible_alias = "st")]
    Status {
        #[arg(help = "A file, relative to the repository: show its lock and its latest changes")]
        path: Option<String>,
        #[arg(short, long, default_value_t = 10, requires = "path", help = "With a path: how many changes to show")]
        limit: usize,
    },
    /// Check repository health and report stuck immutable files; `--all` checks the whole installation
    #[command(after_help = "Examples:\n  tracewise doctor\n  tracewise doctor --all")]
    #[command(visible_alias = "dr")]
    Doctor {
        #[arg(short = 'a', long, help = "Check the whole Tracewise installation, not only this repository")]
        all: bool,
        #[arg(long, help = "Check that no record was altered or removed (the hash chain)")]
        verify: bool,
        #[arg(long, help = "Pack loose stored versions into delta packs and clean up")]
        gc: bool,
        #[arg(long, help = "Rebuild the database from the local batches")]
        rebuild: bool,
        #[arg(long, help = "Give a copied repository folder its own id and device name")]
        adopt_copy: bool,
    },
    /// Install Tracewise for this user: programs, shims, git wrapper, one block in ~/.zshrc (no sudo)
    #[command(after_help = "Examples:\n  tracewise setup\n  tracewise setup --login yes --profile core")]
    Setup {
        #[arg(long, value_parser = ["yes", "no", "ask"], default_value = "ask", help = "Start the daemon at login")]
        login: String,
        #[arg(long = "no-start", help = "Do not start the daemon now")]
        no_start: bool,
        #[arg(long = "no-shell-rc", help = "Do not touch ~/.zshrc")]
        no_shell_rc: bool,
        #[arg(long, help = "Also install the strict sensor (asks for sudo)")]
        strict: bool,
        #[arg(long, default_value = "light", help = "With --strict: light or full")]
        events: String,
        #[arg(long, default_value = "core", help = "Default profile (core or full)")]
        profile: String,
        #[arg(
            long,
            value_parser = ["agents", "full", "ask"],
            default_value = "ask",
            help = "full: a background watcher records every change; agents: nothing runs in the background (ask asks once, full without a terminal)"
        )]
        capture: String,
        #[arg(long, help = "A capture preset (agents, balanced, full, or one of yours; see `tracewise capture list`): chosen from a menu when left out in a terminal")]
        preset: Option<String>,
    },
    /// Remove what `tracewise setup` installed and the agents' user-scope hooks (project histories stay)
    Uninstall {
        #[arg(long, help = "Also delete the ~/.tracewise folder, except the project histories in ~/.tracewise/projects")]
        purge: bool,
        #[arg(long = "purge-histories", help = "With --purge: also delete every project's history (~/.tracewise/projects)")]
        purge_histories: bool,
    },
    /// Show audit log
    #[command(after_help = "Examples:\n  tracewise log\n  tracewise log src/main.rs\n  tracewise log --json")]
    #[command(visible_alias = "l")]
    Log {
        #[arg(help = "Filter changes by path")]
        path: Option<String>,
        #[arg(short = 'l', long, help = "Maximum number of records to return")]
        limit: Option<usize>,
        #[arg(short = 'a', long, help = "Filter by actor name")]
        actor: Option<String>,
        #[arg(short = 's', long, allow_hyphen_values = true, help = "Filter since duration or time")]
        since: Option<String>,
        #[arg(short = 'g', long, help = "Only changes whose path or reason contains this text (case-insensitive)")]
        grep: Option<String>,
        #[arg(long, help = "Only changes newer than this change id (an app asks for what is new since the last row it has)")]
        after: Option<String>,
        #[arg(short = 'G', long, help = "Cluster the changes by task (intent, else session)")]
        group: bool,
        #[arg(long, help = "One task as Markdown, for a review or a hand-off (use with --intent or --session, and --actor/--since)")]
        md: bool,
        #[arg(short = 'i', long, requires = "md", help = "With --md: the changes under this intent id")]
        intent: Option<String>,
        #[arg(short = 'S', long, requires = "md", help = "With --md: the changes of this session id")]
        session: Option<String>,
        #[arg(short = 'u', long, help = "Show every change, not a run of old minor edits as one line")]
        unfold: bool,
        #[arg(long, allow_hyphen_values = true, default_value = "1d", help = "Show a run of minor edits (automatic reasons like `whitespace`) older than this as one line; 0 folds all")]
        fold_after: String,
    },
    /// Show details and diff of a change
    #[command(visible_alias = "sw")]
    Show {
        #[arg(help = "The change ID (ULID)")]
        id: String,
    },
    /// Scan repository for changes
    #[command(visible_alias = "sc")]
    Scan,
    /// Record the named files now: an editor says they were saved (source `editor` unless told otherwise)
    #[command(
        visible_alias = "rec",
        after_help = "Examples:\n  tracewise record src/main.rs\n  tracewise rec --source zed a.rs b.rs --json"
    )]
    #[command(hide = true)]
    Record {
        #[arg(
            long,
            default_value = "editor",
            value_parser = ["editor", "lsp", "vscode", "zed", "jetbrains"],
            help = "Who is asking: recorded as the change's source (a name the sensor, the watcher or the hooks use is refused)"
        )]
        source: String,
        #[arg(required = true, help = "Files to look at, relative to the project or absolute inside it")]
        paths: Vec<String>,
    },
    /// Show line history blame for a file
    #[command(after_help = "Examples:\n  tracewise blame src/main.rs")]
    #[command(visible_alias = "b")]
    Blame {
        #[arg(help = "Path to the file to blame")]
        path: String,
        #[arg(short = 't', long, help = "Blame the file as it was right after this change id")]
        at: Option<String>,
    },
    /// Show why a specific line was changed and its history
    #[command(after_help = "Examples:\n  tracewise why src/main.rs:42")]
    #[command(visible_alias = "w")]
    Why {
        #[arg(help = "<path>:<line> to inspect")]
        target: String,
    },
    /// Show unified diff from an earlier state to now
    #[command(visible_alias = "d")]
    Diff {
        #[arg(short = 's', long, allow_hyphen_values = true, help = "Time or duration (e.g. 1h, 30s)")]
        since: Option<String>,
        #[arg(help = "Optional file path")]
        path: Option<String>,
    },
    /// Undo a specific change or all changes by an actor in a time window
    #[command(
        after_help = "Examples:\n  tracewise undo <change id>\n  tracewise undo --actor claude --since 1h --dry-run\n  tracewise undo --intent <intent id> --dry-run"
    )]
    #[command(visible_alias = "u")]
    Undo {
        #[arg(help = "The change ID to undo")]
        id: Option<String>,
        #[arg(short = 'a', long, help = "Revert changes made by this actor")]
        actor: Option<String>,
        #[arg(short = 'i', long, help = "Revert every change recorded under this intent id (a whole task)")]
        intent: Option<String>,
        #[arg(short = 'S', long, help = "Revert every change of this session id")]
        session: Option<String>,
        #[arg(short = 'c', long, help = "Revert every change of this command id (a bulk change is one command)")]
        command: Option<String>,
        #[arg(short = 's', long, allow_hyphen_values = true, help = "Time window (e.g. 1h, 30m)")]
        since: Option<String>,
        #[arg(short = 'n', long, help = "Print what would be done without modifying files or database")]
        dry_run: bool,
        #[arg(long, help = "Strict mode: refuse if file was edited since")]
        strict: bool,
        #[arg(short = 'r', long, help = "Redo: <id> is an undo's change id; put its changes back")]
        redo: bool,
    },
    /// Restore a file to its state at a change ID or time
    #[command(after_help = "Examples:\n  tracewise restore a.txt --at <change id>")]
    #[command(visible_alias = "rs")]
    Restore {
        #[arg(help = "The file path to restore")]
        path: String,
        #[arg(short = 't', long, help = "Change ID or timestamp")]
        at: String,
    },
    /// Add patterns to .tracewiseignore so Tracewise stops recording them; with --prune also remove the history of ignored (or chosen) paths
    #[command(
        after_help = "Examples:\n  tracewise ignore 'dist/' '*.log'\n  tracewise ignore --prune --dry-run\n  tracewise ignore --prune --yes\n  tracewise ignore --prune --path 'build/**' --older-than 90d --yes"
    )]
    Ignore {
        #[arg(help = "Ignore patterns (same syntax as .gitignore)")]
        patterns: Vec<String>,
        #[arg(long, help = "Remove the history of ignored (or chosen) paths and keep the chain verifiable")]
        prune: bool,
        #[arg(short = 'p', long = "path", requires = "prune", help = "With --prune: prune this path glob instead of the ignored paths (repeatable)")]
        paths: Vec<String>,
        #[arg(short = 'o', long, requires = "prune", help = "With --prune: only records older than this (e.g. 90d, 12w)")]
        older_than: Option<String>,
        #[arg(short = 'n', long, requires = "prune", help = "With --prune: print what would be removed without changing anything")]
        dry_run: bool,
        #[arg(short = 'y', long, requires = "prune", help = "With --prune: do not ask for confirmation (required without a terminal)")]
        yes: bool,
        #[arg(short = 'b', long, requires = "prune", help = "With --prune: write a copy of the database to this file first")]
        backup: Option<String>,
    },
    /// Watcher daemon management
    #[command(visible_alias = "dm")]
    #[command(hide = true)]
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Hook called by agent harness
    #[command(hide = true)]
    Hook {
        /// Hook event name
        #[arg(default_value = "")]
        event: String,
        #[arg(long, help = "Agent adapter name")]
        from: Option<String>,
        #[arg(long, help = "Record raw hook payload JSON into <dir>")]
        record: Option<PathBuf>,
        #[arg(
            long,
            help = "Answer in a neutral JSON shape (api, event, decision, reason, context) instead of the agent's schema, for a program that nests this hook"
        )]
        neutral: bool,
    },
    /// Language server for editors without a Tracewise extension: hover says why a line changed, a save is recorded (talks LSP on stdin and stdout)
    #[command(hide = true)]
    Lsp,
    /// Which layers record live (terminal, background watcher) and named presets of them: `capture`, `capture list`, `capture use <name>`
    #[command(visible_alias = "cap")]
    Capture {
        #[command(subcommand)]
        action: Option<CaptureAction>,
    },
    /// Set up an editor: `editors zed` prints (or with --apply writes) the Zed task and key that ask "why?" in Zed's own terminal; `editors zed --settings` asks what you want and prints, writes or opens the settings
    Editors {
        #[arg(value_parser = ["zed"], help = "The editor")]
        editor: String,
        #[arg(long, help = "Write the files instead of printing them (a backup of each existing file is kept)")]
        apply: bool,
        #[arg(long, default_value = "cmd-alt-w", help = "The key that runs the task")]
        key: String,
        #[arg(long, help = "The editor's config folder (default ~/.config/zed)")]
        dir: Option<PathBuf>,
        #[arg(long, help = "Print the settings.json block for the options below, to paste into Zed's settings (nothing is written)")]
        settings: bool,
        #[arg(long, help = "With --settings: add the block to Zed's settings.json (a backup is kept; comments stay)")]
        write: bool,
        #[arg(long, help = "With --settings: open Zed's settings.json in Zed")]
        open: bool,
        #[arg(long, value_delimiter = ',', help = "When the popup shows: pause (default), every-save, interval, leave, manual (a list: whichever comes first)")]
        when: Vec<String>,
        #[arg(long, help = "Seconds without a save before the popup (pause)")]
        after: Option<f64>,
        #[arg(long, help = "At most one popup per file in this many seconds (interval)")]
        every: Option<f64>,
        #[arg(long, help = "Also show who changed each run of lines at its end (inlay hints)")]
        hints: bool,
        #[arg(long, help = "Do not ask about a save that changed fewer characters than this (it gets an automatic reason)")]
        min_chars: Option<u64>,
        #[arg(long, help = "Do not ask about a save that changed fewer lines than this")]
        min_lines: Option<u64>,
        #[arg(long, value_delimiter = ',', help = "File patterns never asked about, e.g. '*.md,dist/**'")]
        ignore: Vec<String>,
    },
    /// What a program that nests Tracewise needs to know: its API version and features
    #[command(hide = true)]
    Companion,
    /// Agent adapters and configuration
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Terminal shell integration
    #[command(hide = true)]
    Shell {
        #[command(subcommand)]
        action: ShellAction,
    },
    /// List recorded commands
    #[command(name = "commands")]
    #[command(hide = true)]
    CommandHistory,
    /// Git wrapper management
    #[command(name = "git-wrapper")]
    #[command(hide = true)]
    GitWrapper {
        #[command(subcommand)]
        action: GitWrapperAction,
    },
    /// Git hooks for git apps that do not run the Tracewise git wrapper
    #[command(name = "git-hooks")]
    #[command(hide = true)]
    GitHooks {
        #[command(subcommand)]
        action: GitHooksAction,
    },
    /// Called by the hooks that `git-hooks install` writes
    #[command(name = "git-hook")]
    #[command(hide = true)]
    GitHook {
        /// Git hook name (pre-commit, post-commit, post-checkout, post-merge, post-rewrite)
        name: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Say why: after the fact (`note <id> "why"`, `note --last "why"`) or before editing (`note --why "why" --before <files>`)
    #[command(
        after_help = "Examples:\n  tracewise note --last \"why these changes were made\"\n  tracewise note <change id> \"why\"\n  tracewise note --why \"fix the title\" --before a.txt b.txt\n  tracewise note --list"
    )]
    #[command(visible_alias = "n")]
    Note {
        #[arg(long, help = "Add note to every pending change of the current actor")]
        last: Option<String>,
        #[arg(help = "The change ID (ULID)")]
        change_id: Option<String>,
        #[arg(help = "The reason text")]
        why: Option<String>,
        #[arg(short = 'm', long = "note", visible_alias = "why", conflicts_with_all = ["last", "change_id", "why", "list"], help = "Before editing: why these files are going to change (the reason); give the files with --before")]
        reason: Option<String>,
        #[arg(short = 'b', long, num_args = 1.., value_name = "PATH", requires = "reason", help = "Before editing: the files or globs the reason covers")]
        before: Vec<String>,
        #[arg(long, conflicts_with_all = ["last", "change_id", "why", "reason"], help = "List the reasons declared before editing that are still active")]
        list: bool,
        #[arg(long, hide = true, help = "Task ID if any")]
        task: Option<String>,
        #[arg(long, hide = true, help = "Agent name")]
        agent: Option<String>,
        #[arg(long, hide = true, help = "Session ID")]
        session: Option<String>,
        #[arg(long, hide = true, help = "A short label chosen from a list (an editor's buttons): skips the length check made for typed reasons")]
        label: bool,
        #[arg(long, hide = true, help = "An automatic reason (an editor skipping the question): never replaces a reason a person gave")]
        auto: bool,
    },
    /// The opt-in sensor that names the process behind every write
    #[command(hide = true)]
    Strict {
        #[command(subcommand)]
        action: StrictAction,
    },
    /// Work that agents started in the background: list it, or `--stop <id>` to stop attributing later writes to a job
    Jobs {
        #[arg(long, help = "Stop attributing later writes to this background job (an id from the list)")]
        stop: Option<String>,
    },
    /// Manage locks
    #[command(after_help = "Examples:\n  tracewise lock list\n  tracewise lock release a.txt")]
    Lock {
        #[command(subcommand)]
        action: LockAction,
    },
    /// Manage sharing configuration
    Share {
        #[command(subcommand)]
        action: ShareAction,
    },
    /// Print the completions for a shell, so the commands and options complete when you press Tab
    #[command(after_help = "Examples:\n  tracewise completions zsh > ~/.zfunc/_tracewise\n  tracewise completions bash > /usr/local/etc/bash_completion.d/tracewise\n  tracewise completions fish > ~/.config/fish/completions/tracewise.fish")]
    Completions {
        #[arg(value_parser = ["bash", "zsh", "fish", "elvish", "powershell"], help = "The shell")]
        shell: String,
    },
    /// Read or change Tracewise's settings (retention, profile, reasons mode, storage, durability)
    #[command(
        after_help = "Examples:\n  tracewise config get\n  tracewise config get reasons.mode\n  tracewise config set reasons.mode enforce\n  tracewise config set --global retention.commands 14d"
    )]
    #[command(visible_alias = "cfg")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Inspect process hierarchy and identity
    #[command(hide = true)]
    Proc {
        #[command(subcommand)]
        action: ProcAction,
    },
    /// Run a command under a declared reason intent
    #[command(
        after_help = "Examples:\n  tracewise run -m \"run the tests\" -- cargo test\n  tracewise run --note \"run the tests\" -- cargo test"
    )]
    #[command(visible_alias = "r")]
    Run {
        #[arg(short = 'm', long = "note", visible_alias = "why", help = "Why this command is being run (the reason)")]
        why: String,
        #[arg(last = true, required = true, help = "Command and arguments to execute")]
        command: Vec<String>,
    },
    /// List changes without a reason
    #[command(visible_alias = "p")]
    Pending,
    /// Manage command shims for external tools
    #[command(hide = true)]
    Shims {
        #[command(subcommand)]
        action: ShimsAction,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum GitHooksAction {
    /// Write the five hooks into this repository (an existing hook is kept and still runs)
    Install,
    /// Remove them and put back the hooks that were kept
    Uninstall,
    /// Show how each hook stands
    Status,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ShimsAction {
    /// Install command shims into a directory
    Install {
        #[arg(long, help = "Directory to install shims into")]
        dir: PathBuf,
    },
    /// Uninstall command shims from a directory
    Uninstall {
        #[arg(long, help = "Directory to uninstall shims from")]
        dir: PathBuf,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProcAction {
    /// Show process chain up to pid 1
    Chain {
        #[arg(long, help = "Process ID (default: parent PID)")]
        pid: Option<u32>,
    },
    /// Determine who is acting from the process hierarchy
    Who {
        #[arg(long, help = "Process ID (default: parent PID)")]
        pid: Option<u32>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigAction {
    /// Show one setting, or all of them, with where each value comes from
    Get { key: Option<String> },
    /// Change a setting in .tracewiseconfig (or the global file with --global)
    Set {
        key: String,
        value: String,
        #[arg(long, help = "Write the global file ($TRACEWISE_HOME/config.toml) instead of this repository's .tracewiseconfig")]
        global: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum KeyAction {
    /// Print the key as one string to hand to a teammate or keep as a backup (it is the key itself: a secret)
    Export {
        #[arg(long, help = "Key id to export (default: the current key)")]
        id: Option<String>,
    },
    /// Add an exported key to this clone
    Import {
        key: String,
        #[arg(long, help = "Use it for new batches too (after a rotation); otherwise only to read")]
        current: bool,
    },
    /// List key ids (never the keys) and which is current
    List,
    /// Make a new key for new batches; old keys stay so old batches can still be read
    Rotate,
    /// Rotate, then push every record of this device again under the new key (for a lost or leaked key; this device's data must be intact)
    Rekey {
        #[arg(long, help = "Also delete this device's old ref on the remote first (the old batches become unreachable there)")]
        prune_old: bool,
        #[arg(long, help = "Confirm --prune-old")]
        yes: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ShareAction {
    /// Push unpushed records to the companion remote
    Push,
    /// Fetch records from the companion remote
    Fetch,
    /// Initialize sharing with companion remote
    Init {
        #[arg(long, help = "URL of the companion git repository (default: this project's own origin, with hidden refs)")]
        remote: Option<String>,
        #[arg(long, help = "Sharing level (full, summary, off); default summary")]
        level: Option<String>,
        #[arg(
            long,
            help = "Keep the batches under refs/tracewise/device/ (no branches on the remote); the default when --remote is left out"
        )]
        hidden: bool,
        #[arg(long, help = "Encrypt every batch with a project key kept on this machine only (made now if there is none)")]
        encrypt: bool,
    },
    /// Approve the remote named in .tracewiseconfig so `tracewise share push` may upload to it
    Trust,
    /// Show how sharing is set up here: remote, level, refs, encryption, key, warnings
    Status,
    /// The project's encryption key: export, import, list, rotate, rekey
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// List the signing keys pinned for each device; `--forget DEVICE` drops one after a deliberate key change
    Keys {
        #[arg(long, help = "Device id whose pinned key to forget")]
        forget: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum LockAction {
    /// List active locks
    List,
    /// Release a lock on a path or all locks
    Release {
        #[arg(help = "Path to release")]
        path: Option<String>,
        #[arg(long, help = "Release every lock")]
        all: bool,
    },
    /// Wait for a lock on a path
    Wait {
        #[arg(help = "Path to wait for")]
        path: String,
        #[arg(long, help = "Agent name")]
        agent: Option<String>,
        #[arg(long, help = "Session ID")]
        session: Option<String>,
        #[arg(long, default_value = "30", help = "Timeout in seconds")]
        timeout: u64,
    },
}

#[derive(Debug, Clone)]
pub enum IntentAction {
    Add {
        why: String,
        task: Option<String>,
        agent: Option<String>,
        session: Option<String>,
        paths: Vec<String>,
    },
    List {
        session: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ShellAction {
    /// Initialize shell integration
    Init {
        shell: String,
        #[arg(long = "non-interactive", help = "Print non-interactive shell hook block")]
        non_interactive: bool,
    },
    /// Pre-execution hook for shell command
    Preexec {
        #[arg(long)]
        id: String,
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Pre-command hook after shell command finishes
    Precmd {
        #[arg(long)]
        id: String,
        #[arg(long)]
        exit: i32,
    },
    /// Begin a command window for shell command
    Begin {
        #[arg(long)]
        pid: u32,
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// End a command window for shell command
    End {
        #[arg(long)]
        pid: u32,
        #[arg(long)]
        exit: i32,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum GitWrapperAction {
    /// Install git wrapper
    Install {
        #[arg(long)]
        dir: PathBuf,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum StrictAction {
    /// Is the sensor installed, connected and running, and if not, why
    Status,
    /// Install the sensor (needs sudo; run it yourself)
    Install {
        /// Print what would be run instead of running it
        #[arg(long)]
        dry_run: bool,
        /// Event preset: light (default) or full (adds write events, about four times the cost)
        #[arg(long, default_value = "light")]
        events: String,
    },
    /// Remove the sensor
    Uninstall {
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum DaemonAction {
    /// Run daemon in foreground
    Run,
    /// Record the changed paths named on stdin (one JSON line each); started by tracewise-watchd
    #[command(hide = true)]
    ScanPaths,
    /// Print 1 when an agent is working or a background job is alive (the strict sensor runs only then)
    #[command(hide = true)]
    Active,
    /// Release expired locks and repair lock flags; started by tracewise-watchd
    #[command(hide = true)]
    Sweep,
    Start,
    Stop,
    Status,
    /// Install daemon LaunchAgent plist
    Install {
        #[arg(long = "no-load", help = "Do not load into service manager")]
        no_load: bool,
    },
    /// Uninstall daemon LaunchAgent plist
    Uninstall,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CaptureAction {
    /// The current preset and each layer (the default)
    Status,
    /// The presets: the built-in ones and the ones in ~/.tracewise/presets.toml
    List,
    /// The background watcher: start, stop, status, install at login, uninstall
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// The optional sensor that names the exact process behind every write: status, install, uninstall
    Sensor {
        #[command(subcommand)]
        action: StrictAction,
    },
    /// The repositories the watcher follows, with change counts and sizes
    Repos,
    /// Apply a preset through the same code as `tracewise setup`
    Use {
        #[arg(help = "The preset name (see `tracewise capture list`)")]
        preset: String,
        #[arg(short = 'n', long, help = "Only say what would change")]
        dry_run: bool,
        #[arg(short = 'y', long, help = "Do not ask first")]
        yes: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum AgentAction {
    /// Install agent hooks and configuration
    Install {
        /// Agent name (claude, codex, gemini, antigravity, cursor)
        name: String,
        #[arg(long, default_value = "project", help = "Configuration scope (project, user)")]
        scope: String,
    },
    /// Uninstall agent hooks and configuration
    Uninstall {
        /// Agent name (claude, codex, gemini, antigravity, cursor)
        name: String,
        #[arg(long, default_value = "project", help = "Configuration scope (project, user)")]
        scope: String,
    },
    /// Status of agent adapters
    Status,
    /// List the built-in agents and the adapter files (user and project)
    List,
    /// Show what a payload would become with an adapter file, without running the agent
    Check {
        /// The adapter file (`<name>.toml`)
        file: std::path::PathBuf,
        /// A hook payload (JSON file)
        payload: std::path::PathBuf,
    },
}

#[cfg(test)]
#[path = "test_cli.rs"]
mod tests;
