//! `tracewise capture`: which layers record live, and named presets of them.
//!
//! The presets are data (`presets.toml`, built in; `~/.tracewise/presets.toml` replaces or adds). The code knows the two layers it
//! can switch, `terminal` and `daemon`, and applies a preset through the same `setup` code a person would run, so the two never
//! disagree. The current preset is never stored: it is worked out from what is really installed, so it cannot go stale.
use crate::setup::{self, Env, Login, SetupOptions};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const BUILT_IN: &str = include_str!("../presets.toml");
/// The layers a preset may mention.
const LAYERS: [&str; 2] = ["terminal", "daemon"];

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    preset: BTreeMap<String, Raw>,
}

#[derive(Deserialize, Default)]
struct Raw {
    description: Option<String>,
    #[serde(default)]
    layers: BTreeMap<String, bool>,
}

/// A preset: a name, a sentence for people, and the layers it decides (a layer it does not mention is left as it is).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Preset {
    pub name: String,
    pub description: String,
    pub terminal: Option<bool>,
    pub daemon: Option<bool>,
    pub built_in: bool,
}

/// What is on right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct State {
    pub terminal: bool,
    pub daemon: bool,
    /// The watcher is running now (the layer can be on, installed at login, and not running).
    pub watcher_running: bool,
    /// `capture.mode` as configured.
    pub mode: tracewise_core::capture::CaptureMode,
    /// Agents whose user-scope hooks are installed.
    pub hooks: Vec<String>,
}

fn parse(text: &str, built_in: bool, origin: &str, warnings: &mut Vec<String>) -> Vec<Preset> {
    let file: File = match toml::from_str(text) {
        Ok(f) => f,
        Err(e) => {
            warnings.push(format!("{origin}: not read ({})", e.message()));
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for (name, raw) in file.preset {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            warnings.push(format!("{origin}: preset `{name}` skipped (use lowercase letters, digits and -)"));
            continue;
        }
        for layer in raw.layers.keys().filter(|l| !LAYERS.contains(&l.as_str())) {
            warnings.push(format!("{origin}: preset `{name}`: unknown layer `{layer}` ignored (known: {})", LAYERS.join(", ")));
        }
        let (terminal, daemon) = (raw.layers.get("terminal").copied(), raw.layers.get("daemon").copied());
        if terminal.is_none() && daemon.is_none() {
            warnings.push(format!("{origin}: preset `{name}` skipped (it decides no layer)"));
            continue;
        }
        out.push(Preset { name, description: raw.description.unwrap_or_default(), terminal, daemon, built_in });
    }
    out
}

/// The built-in presets, then the user's file: same name replaces, others come after. Problems in the user's file are returned as
/// warnings and never stop anything.
pub(crate) fn load(user_file: &Path) -> (Vec<Preset>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut presets = parse(BUILT_IN, true, "built-in presets", &mut warnings);
    if let Ok(text) = fs::read_to_string(user_file) {
        let origin = user_file.display().to_string();
        for p in parse(&text, false, &origin, &mut warnings) {
            match presets.iter_mut().find(|q| q.name == p.name) {
                Some(slot) => *slot = p,
                None => presets.push(p),
            }
        }
    }
    (presets, warnings)
}

fn watcher_running(env: &Env) -> bool {
    fs::read_to_string(env.root.join("daemon.pid"))
        .ok()
        .and_then(|t| t.trim().parse::<i32>().ok())
        .is_some_and(tracewise_platform::signal::is_daemon_pid)
}

/// What is really installed: the shell blocks, the login item or a running watcher, the configured mode, the agents' hooks.
pub(crate) fn probe(env: &Env) -> State {
    let has_block = |p: std::path::PathBuf| fs::read_to_string(p).is_ok_and(|t| t.contains(setup::BLOCK_START));
    // the terminal layer is the hook in `.zshrc` (or fish's hook): the PATH lines in `.zshenv` stay with every preset
    let fish_hook = fs::read_to_string(setup::install::fish_file(env)).is_ok_and(|t| t.contains("shell init fish"));
    let terminal = has_block(env.zshrc()) || fish_hook;
    let running = watcher_running(env);
    let config = fs::read_to_string(env.root.join("config.toml")).unwrap_or_default();
    let mode = crate::cmd_config::read_value(&config, "capture.mode")
        .and_then(|v| tracewise_core::capture::CaptureMode::parse(&v))
        .unwrap_or_default();
    let hooks = crate::cmd_agents::AGENT_CONFIGS
        .iter()
        .filter(|(name, cfg)| crate::cmd_agents::user_hooks_exist(&env.home, name, cfg))
        .map(|(name, _)| name.to_string())
        .collect();
    State { terminal, daemon: running || setup::login_item_installed(env), watcher_running: running, mode, hooks }
}

impl Preset {
    fn matches(&self, s: &State) -> bool {
        self.terminal.is_none_or(|t| t == s.terminal) && self.daemon.is_none_or(|d| d == s.daemon)
    }
}

/// The first preset the state matches, or None (`custom`).
pub(crate) fn current<'a>(presets: &'a [Preset], s: &State) -> Option<&'a Preset> {
    presets.iter().find(|p| p.matches(s))
}

/// Things about the state that disagree with each other or with the configured mode, in words. `capture use` fixes them.
pub(crate) fn inconsistencies(s: &State) -> Vec<String> {
    use tracewise_core::capture::CaptureMode;
    let mut out = Vec::new();
    if s.daemon && s.mode == CaptureMode::Agents {
        out.push("capture.mode is `agents` but a watcher is installed or running: it records everything, so the mode and the watcher disagree".to_string());
    }
    if !s.daemon && s.mode == CaptureMode::Full {
        out.push("capture.mode is `full` but no watcher is installed: edits made by other programs are only found by the quick scan".to_string());
    }
    if s.daemon && !s.watcher_running {
        out.push("the watcher is installed but not running (`tracewise capture daemon start`); its gap is found by the quick scan".to_string());
    }
    out
}

/// What changing from `s` to `p` would do, one line each; empty when nothing differs.
pub(crate) fn plan(p: &Preset, s: &State) -> Vec<String> {
    use tracewise_core::capture::CaptureMode;
    let mut steps = Vec::new();
    match p.terminal {
        Some(true) if !s.terminal => steps.push("terminal: on (add Tracewise's lines to your shell startup files; shims and the git wrapper become active)".to_string()),
        Some(false) if s.terminal => steps.push("terminal: off (remove Tracewise's lines from your shell startup files; the programs stay installed)".to_string()),
        _ => {}
    }
    match p.daemon {
        Some(true) if !s.daemon || s.mode != CaptureMode::Full || !s.watcher_running => {
            steps.push("daemon: on (capture.mode = full, start the watcher now and at login)".to_string())
        }
        Some(false) if s.daemon || s.mode != CaptureMode::Agents => {
            steps.push("daemon: off (capture.mode = agents, stop the watcher and remove the login item; history is kept)".to_string())
        }
        _ => {}
    }
    steps
}

/// Applies a preset through `setup` (the same code `tracewise setup` runs), then takes the shell lines out when the terminal layer
/// is to be off. Returns what was done.
pub(crate) fn apply(env: &Env, p: &Preset, live: bool) -> Result<Vec<String>, String> {
    use tracewise_core::capture::CaptureMode;
    let before = probe(env);
    let config = fs::read_to_string(env.root.join("config.toml")).unwrap_or_default();
    let profile = crate::cmd_config::read_value(&config, "profile.name").unwrap_or_else(|| "core".to_string());
    let login = match p.daemon {
        Some(true) => Login::Yes,
        Some(false) => Login::No,
        None if before.daemon => Login::Yes,
        None => Login::No,
    };
    let opts = SetupOptions {
        login,
        start: true,
        shell_rc: true,
        terminal: p.terminal.unwrap_or(before.terminal),
        live,
        profile,
        capture: p.daemon.map(|d| if d { CaptureMode::Full } else { CaptureMode::Agents }),
    };
    let mut done = Vec::new();
    setup::setup_in(env, &opts)?;
    if p.daemon == Some(false) && live {
        if let Some(pid) = fs::read_to_string(env.root.join("daemon.pid")).ok().and_then(|t| t.trim().parse::<i32>().ok()) {
            let _ = tracewise_platform::signal::terminate_daemon(pid);
            done.push("stopped the watcher".to_string());
        }
    }
    Ok(done)
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// `tracewise capture`: the current preset and each layer.
pub(crate) fn print_status(env: &Env, presets: &[Preset], warnings: &[String], json: bool) {
    let s = probe(env);
    let name = current(presets, &s).map(|p| p.name.clone());
    let problems = inconsistencies(&s);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "preset": name,
                "layers": { "terminal": s.terminal, "daemon": s.daemon, "watcher_running": s.watcher_running, "hooks": s.hooks },
                "mode": s.mode.name(),
                "problems": problems,
                "warnings": warnings,
            })
        );
        return;
    }
    use crate::ui::{paint, ACCENT, BOLD, DIM, GREEN};
    let mark = |on: bool| if on { paint(GREEN, "✔") } else { paint(DIM, "○") };
    let row_with = |m: String, label: &str, text: String| println!("  {m} {} {text}", paint(BOLD, &format!("{label:<9}")));
    let row = |on: bool, label: &str, text: String| row_with(mark(on), label, text);
    println!(
        "{} {}",
        paint(DIM, "capture:"),
        match name.as_deref() {
            Some(n) => paint(ACCENT, n),
            None => paint(ACCENT, "custom") + " (no preset matches what is installed)",
        }
    );
    row(s.terminal, "terminal", format!("{}  {}", on_off(s.terminal), paint(DIM, "the command shims and the terminal hook: wrapped and typed commands are recorded")));
    row(
        s.daemon,
        "daemon",
        format!(
            "{}  {}",
            on_off(s.daemon),
            paint(DIM, &format!("the background watcher{}", if s.daemon { if s.watcher_running { " (running)" } else { " (not running)" } } else { "" }))
        ),
    );
    // the mode is a setting, not a layer: a dot, not a tick
    row_with(
        paint(DIM, "·"),
        "mode",
        format!(
            "{}  {}",
            s.mode.name(),
            paint(DIM, if s.mode == tracewise_core::capture::CaptureMode::Full { "capture.mode: the watcher records everything" } else { "capture.mode: no watcher; a quick scan finds other edits" })
        ),
    );
    if s.hooks.is_empty() {
        row(false, "hooks", format!("off  {}", paint(DIM, "no agent hooks installed: `tracewise agent install <name> --scope user`")));
    } else {
        row(true, "hooks", s.hooks.join(", "));
    }
    println!("  {} {}  {}", paint(DIM, "–"), paint(BOLD, &format!("{:<9}", "editor")), paint(DIM, "—    an editor extension runs inside the editor and works with every preset"));
    for p in &problems {
        println!("{}", crate::ui::warn_line(p));
    }
    for w in warnings {
        println!("{}", crate::ui::warn_line(w));
    }
    if name.is_none() || !problems.is_empty() {
        println!("`tracewise capture list` shows the presets, `tracewise capture use <name>` applies one.");
    }
}

/// `tracewise capture list`.
pub(crate) fn print_list(env: &Env, presets: &[Preset], warnings: &[String], json: bool) {
    let s = probe(env);
    let now = current(presets, &s).map(|p| p.name.clone());
    if json {
        let rows: Vec<_> = presets
            .iter()
            .map(|p| serde_json::json!({ "name": p.name, "description": p.description, "terminal": p.terminal, "daemon": p.daemon, "built_in": p.built_in, "current": now.as_deref() == Some(&p.name) }))
            .collect();
        println!("{}", serde_json::json!({ "presets": rows, "warnings": warnings }));
        return;
    }
    for p in presets {
        let layers = [("terminal", p.terminal), ("daemon", p.daemon)]
            .iter()
            .filter_map(|(n, v)| v.map(|v| format!("{n} {}", on_off(v))))
            .collect::<Vec<_>>()
            .join(", ");
        println!("{}{}  ({layers}){}", if now.as_deref() == Some(&p.name) { "* " } else { "  " }, p.name, if p.built_in { "" } else { "  [yours]" });
        println!("    {}", p.description);
    }
    for w in warnings {
        println!("! {w}");
    }
}

/// `tracewise capture use <name>`. Returns the exit code.
pub(crate) fn run_use(env: &Env, presets: &[Preset], name: &str, dry_run: bool, yes: bool, json: bool, live: bool) -> i32 {
    use std::io::IsTerminal;
    let Some(preset) = presets.iter().find(|p| p.name == name) else {
        eprintln!("tracewise: no preset `{name}`; `tracewise capture list` shows them");
        return 2;
    };
    let s = probe(env);
    let steps = plan(preset, &s);
    if json && dry_run {
        println!("{}", serde_json::json!({ "preset": name, "steps": steps, "applied": false }));
        return 0;
    }
    if steps.is_empty() {
        println!("Already `{name}`: nothing to change.");
        return 0;
    }
    println!("`{name}` would do:");
    for step in &steps {
        println!("  - {step}");
    }
    if dry_run {
        return 0;
    }
    if !yes {
        if !std::io::stdin().is_terminal() {
            eprintln!("tracewise: not asking without a terminal; add --yes to apply");
            return 2;
        }
        if !crate::ui::confirm("Apply this?") {
            println!("Nothing changed.");
            return 0;
        }
    }
    match apply(env, preset, live) {
        Ok(done) => {
            for d in done {
                println!("  {d}");
            }
            println!("Capture is now `{name}`. Open a new terminal for shell changes to take effect.");
            0
        }
        Err(e) => {
            eprintln!("tracewise: could not apply `{name}`: {e}");
            1
        }
    }
}

#[cfg(test)]
#[path = "test_capture_preset.rs"]
mod tests;
