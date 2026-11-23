//! `tracewise setup`, `tracewise uninstall`, `tracewise doctor --all`: one root (`TRACEWISE_HOME`), one manifest (`install.json`),
//! one marked block in `~/.zshrc`. Nothing here needs a repository, and nothing needs sudo (the strict sensor has
//! its own `tracewise capture sensor install`).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

/// Written next to the programs by `scripts/package.sh`; absent for a build from source.
pub(crate) const CHECKSUM_FILE: &str = "SHA256SUMS";
pub(crate) const PROGRAMS: &[&str] = &["tracewise", "tracewise-git", "tracewise-shim", "tracewise-watchd", "tracewise-sensor"];
pub(crate) const BLOCK_START: &str = "# >>> tracewise >>>";
pub(crate) const BLOCK_END: &str = "# <<< tracewise <<<";

pub(crate) mod doctor;
pub(crate) mod install;
pub(crate) mod login;
#[cfg(test)]
pub(crate) mod tests;

pub use doctor::*;
pub use install::*;
pub(crate) use login::*;

/// Where things are. Tests point all three at temporary folders.
pub struct Env {
    pub home: PathBuf,
    pub root: PathBuf,
    /// The folder holding the programs to install (next to the running `tracewise`).
    pub src_dir: PathBuf,
    /// The `PATH` to look at when deciding whether a short program name is already taken by another tool.
    pub path: Option<std::ffi::OsString>,
}

impl Env {
    pub fn current() -> Env {
        let exe = std::env::current_exe().ok().and_then(|p| p.canonicalize().ok()).unwrap_or_default();
        Env {
            home: std::env::var("HOME").map(PathBuf::from).unwrap_or_default(),
            root: tracewise_core::get_tracewise_home(),
            src_dir: exe.parent().map(Path::to_path_buf).unwrap_or_default(),
            path: std::env::var_os("PATH"),
        }
    }
    pub(crate) fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }
    pub(crate) fn shims(&self) -> PathBuf {
        self.root.join("shims")
    }
    pub(crate) fn manifest(&self) -> PathBuf {
        self.root.join("install.json")
    }
    pub(crate) fn zshrc(&self) -> PathBuf {
        self.home.join(".zshrc")
    }
    /// Read by every zsh, including the shells an agent starts: the PATH lives here so hooks find `tracewise`.
    pub(crate) fn zshenv(&self) -> PathBuf {
        self.home.join(".zshenv")
    }
    pub(crate) fn agent_plist(&self) -> PathBuf {
        self.home.join("Library/LaunchAgents").join(format!("{AGENT_LABEL}.plist"))
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Manifest {
    pub version: String,
    pub installed_at: String,
    #[serde(default)]
    pub profile: String,
    pub programs: Vec<String>,
    pub shims: usize,
    pub zshrc_block: bool,
    pub zshenv_block: bool,
    pub zshrc_backup: Option<String>,
    pub launch_agent: Option<String>,
    /// Old copies in ~/.cargo/bin that now link to the installed programs (so they never go stale).
    pub cargo_links: Vec<String>,
}

pub(crate) fn read_manifest(env: &Env) -> Option<Manifest> {
    serde_json::from_str(&fs::read_to_string(env.manifest()).ok()?).ok()
}

pub(crate) fn write_manifest(env: &Env, m: &Manifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(m).map_err(|e| e.to_string())?;
    tracewise_platform::private::write(env.manifest(), text + "\n").map_err(|e| format!("cannot write install.json: {e}"))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Login {
    Ask,
    Yes,
    No,
}

pub struct SetupOptions {
    pub login: Login,
    pub start: bool,
    pub shell_rc: bool,
    /// The terminal layer: the shims on `PATH` and the terminal hook. False leaves only the programs on `PATH` (agent hooks need them).
    pub terminal: bool,
    /// Load the login item and start the daemon (off in tests).
    pub live: bool,
    pub profile: String,
    /// `Some(Agents)`: no watcher and no login item. The mode is written to the global config; `None` keeps what is there.
    pub capture: Option<tracewise_core::capture::CaptureMode>,
}

/// The mode for `--capture <choice>`: `agents` and `full` as given; `ask` asks one question when `interactive`, and when it cannot
/// ask it changes nothing (`None`): a script that runs `setup` again must not undo what the person chose.
pub(crate) fn resolve_capture(
    choice: &str,
    interactive: bool,
    ask: impl FnOnce() -> String,
) -> Option<tracewise_core::capture::CaptureMode> {
    use tracewise_core::capture::CaptureMode;
    match choice {
        "agents" => Some(CaptureMode::Agents),
        "full" => Some(CaptureMode::Full),
        "ask" if interactive => match ask().trim().to_lowercase().as_str() {
            "agents" | "a" => Some(CaptureMode::Agents),
            _ => Some(CaptureMode::Full),
        },
        _ => None,
    }
}

/// Tests set `TRACEWISE_SETUP_NO_LAUNCHCTL=1`: they must not load, unload or kill anything on the machine they run on.
pub(crate) fn live() -> bool {
    std::env::var("TRACEWISE_SETUP_NO_LAUNCHCTL").as_deref() != Ok("1")
}

/// What the capture presets mean for `setup`'s options: the terminal layer, the watcher (and so `capture.mode`), and the login item.
fn apply_preset(opts: &mut SetupOptions, preset: &crate::capture_preset::Preset, interactive: bool, login_flag: Login) {
    use tracewise_core::capture::CaptureMode;
    if let Some(terminal) = preset.terminal {
        opts.terminal = terminal;
    }
    match preset.daemon {
        Some(true) => {
            opts.capture = Some(CaptureMode::Full);
            // starting at login is a separate question (or flag), asked only when there is someone to ask
            opts.login = if login_flag == Login::Ask && !interactive { Login::No } else { login_flag };
        }
        Some(false) => {
            opts.capture = Some(CaptureMode::Agents);
            opts.login = Login::No;
        }
        None => {}
    }
}

/// The banner and the question: which preset, with the one that matches what is installed (else `balanced`) as the default.
fn ask_preset(env: &Env, presets: &[crate::capture_preset::Preset]) -> usize {
    use crate::ui::{paint, ACCENT, DIM};
    println!("\n  {} {}", paint(ACCENT, "◆ Tracewise setup"), paint(DIM, "· about a minute, never needs sudo"));
    crate::ui::print_wrapped(2, "A flight recorder for your code: who changed every line, when, and why.", DIM);
    // only an installation that exists has a current preset; a fresh machine is offered the recommended one
    let current = read_manifest(env).and_then(|_| crate::capture_preset::current(presets, &crate::capture_preset::probe(env)).map(|p| p.name.clone()));
    let default = presets
        .iter()
        .position(|p| Some(&p.name) == current.as_ref())
        .or_else(|| presets.iter().position(|p| p.name == "balanced"))
        .unwrap_or(0);
    let options: Vec<(String, String)> = presets
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let tag = if i == default { if current.is_some() { "  (now)" } else { "  (recommended)" } } else { "" };
            // the first sentence is enough for a menu
            let short = p.description.split_once(". ").map_or(p.description.as_str(), |(a, _)| a).trim_end_matches('.');
            (p.name.clone(), format!("{short}{tag}"))
        })
        .collect();
    crate::ui::choose_here("How should Tracewise watch your work?", &options, default)
}

/// A path with the home folder written as `~`.
fn tilde(env: &Env, path: &Path) -> String {
    match path.strip_prefix(&env.home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// The result of a setup, as a checklist and what to do next.
fn print_setup_result(env: &Env, m: &Manifest, opts: &SetupOptions) {
    use crate::ui::{ok_line, paint, warn_line, ACCENT, BOLD, DIM};
    let row = |label: &str, text: String| ok_line(&format!("{:<9} {text}", paint(BOLD, label)));
    println!("\n  {} {}", paint(ACCENT, "◆"), paint(BOLD, &format!("Tracewise {} is set up", m.version)));
    println!("  {}\n", paint(DIM, &tilde(env, &env.root)));
    println!("{}", row("Programs", format!("tracewise (also tw) and {} helper programs in {}", m.programs.len().saturating_sub(1), tilde(env, &env.bin()))));
    if opts.terminal && opts.shell_rc {
        println!("{}", row("Terminal", format!("{} shims, and the terminal hook in ~/.zshrc", m.shims)));
    } else {
        println!("{}", row("Terminal", "off: only the programs are on your PATH (agent hooks need them)".to_string()));
    }
    if opts.shell_rc {
        println!("{}", row("Shell", "one Tracewise block in ~/.zshenv (PATH)".to_string()));
        if install::fish_installed(env) && install::fish_file(env).is_file() {
            println!("{}", row("Fish", format!("one Tracewise file, {}", install::fish_file(env).display())));
        }
        if let Some(shell) = install::unsupported_shell() {
            println!("{}", warn_line(&format!("{shell} is not supported yet: add {}/bin to PATH by hand", env.root.display())));
        }
    } else {
        println!("{}", warn_line("shell files were left alone: add the Tracewise programs to your PATH by hand"));
    }
    match (m.launch_agent.is_some(), opts.capture) {
        (true, _) => println!("{}", row("Watcher", "records changes by any program; starts at login".to_string())),
        (false, Some(tracewise_core::capture::CaptureMode::Full)) => {
            println!("{}", row("Watcher", "records changes by any program; start it with `tracewise capture daemon start`".to_string()))
        }
        _ => println!("{}", row("Watcher", "off: your own edits are found by a quick scan on your next command".to_string())),
    }
    if let Some(b) = &m.zshrc_backup {
        println!("{}", row("Backup", tilde(env, Path::new(b))));
    }
    println!("\n  {}", paint(BOLD, "Next"));
    println!("    {}  Open a new terminal", paint(DIM, "1"));
    println!("    {}  {}", paint(DIM, "2"), "cd your-project && tracewise init");
    println!("    {}  tracewise agent install claude   {}", paint(DIM, "3"), paint(DIM, "(optional: exact credit for an agent)"));
    println!();
    crate::ui::print_wrapped(2, "Check anything with `tracewise doctor --all`; change how much is watched with `tracewise capture use <preset>`.", DIM);
    println!();
}

#[allow(clippy::too_many_arguments)]
pub fn run_setup(json: bool, login: Login, no_start: bool, no_shell_rc: bool, strict: bool, events: &str, profile: &str, capture: &str, preset: Option<&str>) {
    use std::io::IsTerminal;
    let env = Env::current();
    let interactive = std::io::stdin().is_terminal() && !json;
    let (presets, _warnings) = crate::capture_preset::load(&env.root.join("presets.toml"));
    let chosen: Option<&crate::capture_preset::Preset> = match preset {
        Some(name) => match presets.iter().find(|p| p.name == name) {
            Some(p) => Some(p),
            None => {
                eprintln!("tracewise: no preset `{name}`; use one of: {}", presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "));
                process::exit(2);
            }
        },
        // no preset and no `--capture` given: in a terminal, ask once
        None if capture == "ask" && interactive => Some(&presets[ask_preset(&env, &presets)]),
        None => None,
    };
    let mut opts = SetupOptions {
        login,
        start: !no_start,
        shell_rc: !no_shell_rc,
        terminal: true,
        live: live(),
        profile: profile.to_string(),
        capture: if chosen.is_some() { None } else { resolve_capture(capture, false, String::new) },
    };
    if let Some(p) = chosen {
        apply_preset(&mut opts, p, interactive, login);
    }
    match setup_in(&env, &opts) {
        Ok(m) => {
            if json {
                println!("{}", serde_json::to_string(&m).unwrap_or_default());
            } else {
                print_setup_result(&env, &m, &opts);
            }
        }
        Err(e) => {
            eprintln!("tracewise: setup failed: {e}");
            process::exit(1);
        }
    }
    if strict {
        let status = Command::new(env.bin().join("tracewise")).args(["strict", "install", "--events", events]).status();
        if !status.map(|s| s.success()).unwrap_or(false) {
            process::exit(1);
        }
    }
}

pub fn run_uninstall(json: bool, purge: bool, purge_histories: bool) {
    let env = Env::current();
    let removed = uninstall_in(&env, purge, purge_histories, live());
    if json {
        println!("{}", serde_json::json!({ "removed": removed }));
    } else {
        if removed.is_empty() {
            println!("nothing to remove: Tracewise is not set up here");
        }
        for r in &removed {
            println!("removed {r}");
        }
        if purge_histories {
            println!("Project histories in {} were deleted.", env.root.join("projects").display());
        } else {
            println!(
                "Project histories in {} are kept (`--purge-histories` deletes them); v1 histories in .git/tracewise are never touched.",
                env.root.join("projects").display()
            );
        }
        println!("The strict sensor, if installed, is removed with `tracewise capture sensor uninstall`.");
    }
}

pub fn run_doctor_all(json: bool) {
    let env = Env::current();
    let path_var = std::env::var("PATH").unwrap_or_default();
    let list = checks(&env, &path_var);
    #[cfg(feature = "platform-stub")]
    let bad = 0;
    #[cfg(not(feature = "platform-stub"))]
    let bad = list.iter().filter(|c| !c.ok).count();
    if json {
        let items: Vec<_> = list.iter().map(|c| serde_json::json!({"ok": c.ok, "warn": c.warn, "what": c.what, "fix": c.fix})).collect();
        println!("{}", serde_json::json!({ "ok": bad == 0, "checks": items }));
    } else {
        for c in &list {
            let mark = if !c.ok {
                "FAIL"
            } else if c.warn {
                "warn"
            } else {
                "ok  "
            };
            if c.fix.is_empty() || (c.ok && !c.warn) {
                println!("{mark}  {}", c.what);
            } else {
                println!("{mark}  {}  ->  {}", c.what, c.fix);
            }
        }
        println!("{}", if bad == 0 { "all good" } else { "problems found" });
    }
    if bad > 0 {
        process::exit(1);
    }
}
