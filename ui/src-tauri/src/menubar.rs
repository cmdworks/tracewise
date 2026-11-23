//! The menu bar: an icon that says whether Tracewise is recording, and a menu to start or stop the watcher, pick a project, see the last
//! change, and open the app or a terminal.
//!
//! The menu is built in three steps so the logic can be tested without a window: [`gather`] asks `tracewise` (the same allow-listed
//! commands the app runs), [`build_model`] turns the answers into a description of the icon and the menu, and `show` hands that
//! description to the system tray. Nothing here changes anything on its own: the watcher is started or stopped only by a click.
use crate::{build_tracewise_command, locate_tracewise_binary, run_bounded, validate_tracewise_args};
use serde_json::Value;
use std::sync::Mutex;
use std::time::Duration;

/// How long one command of a refresh may take.
const ASK_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    /// Something records live (the watcher, or agent hooks).
    Recording,
    /// Nothing records live: no hooks and no watcher.
    Idle,
    /// A problem a person should look at: Tracewise missing, the watcher installed but not running, settings that disagree.
    Attention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Watcher {
    Running,
    Stopped,
    /// No watcher is set up (the `agents` and `balanced` presets): nothing to start.
    NotSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub path: String,
    pub name: String,
    pub changes: Option<u64>,
    /// Where its history is kept; a change of the files in it is how the menu notices a change without asking.
    pub data_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Last {
    /// `you via Zed`, `claude`, ...
    pub who: String,
    pub age: String,
    pub reason: Option<String>,
}

/// Everything the menu shows, as read from `tracewise`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// `tracewise` could not be run: the one thing to fix first.
    pub problem: Option<String>,
    pub preset: Option<String>,
    pub watcher: Option<Watcher>,
    pub hooks: Vec<String>,
    pub warnings: Vec<String>,
    pub projects: Vec<Project>,
    pub current: Option<String>,
    pub last: Option<Last>,
    pub pending: usize,
    /// The newest change without a reason, the one "Add a reason…" is about.
    pub pending_id: Option<String>,
    /// The agent that made the newest agent change, whose last work "Undo" is about.
    pub agent: Option<String>,
    pub jobs: Vec<Job>,
    pub locks: Vec<String>,
    pub share_on: bool,
    pub login_item: bool,
    pub notifications: bool,
}

/// The small picture beside a menu row. Named here, drawn by the system (`NativeIcon`) in the tray code, so the model stays plain data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// A green dot.
    Good,
    /// A yellow dot.
    Warn,
    /// A grey dot.
    Off,
    Caution,
    Folder,
    Play,
    Stop,
    App,
    Terminal,
    Settings,
    Undo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A line of text that does nothing (greyed, as the system draws it).
    Label { text: String, glyph: Option<Glyph> },
    Action { id: String, text: String, enabled: bool, glyph: Option<Glyph>, key: Option<&'static str> },
    Check { id: String, text: String, checked: bool },
    Submenu { text: String, glyph: Option<Glyph>, items: Vec<Item> },
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub icon: IconState,
    pub tooltip: String,
    pub items: Vec<Item>,
}

fn label(text: impl Into<String>) -> Item {
    Item::Label { text: text.into(), glyph: None }
}

fn label_with(glyph: Glyph, text: impl Into<String>) -> Item {
    Item::Label { text: text.into(), glyph: Some(glyph) }
}

fn action(id: &str, text: &str, glyph: Option<Glyph>, key: Option<&'static str>) -> Item {
    Item::Action { id: id.to_string(), text: text.to_string(), enabled: true, glyph, key }
}

/// A reason that fits a menu row: one line, cut at a word with an ellipsis.
fn short(text: &str, max: usize) -> String {
    let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= max {
        return one;
    }
    let cut: String = one.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.rsplit_once(' ').map_or(cut.as_str(), |(a, _)| a).trim_end())
}

/// The icon and the menu for a snapshot: the state first, then the project and what happened last, then the watcher, then where to go.
pub fn build_model(s: &Snapshot) -> Model {
    let mut items = Vec::new();
    if let Some(problem) = &s.problem {
        items.push(label_with(Glyph::Warn, "Needs attention"));
        items.push(label(format!("   {problem}")));
        items.push(Item::Separator);
        items.push(action("open-app", "Open Tracewise…", Some(Glyph::App), Some("o")));
        items.push(Item::Separator);
        items.push(action("quit", "Quit Tracewise", None, Some("q")));
        return Model { icon: IconState::Attention, tooltip: format!("Tracewise: {problem}"), items };
    }
    let watcher_running = s.watcher == Some(Watcher::Running);
    let watcher_down = s.watcher == Some(Watcher::Stopped);
    let attention = watcher_down || !s.warnings.is_empty();
    let icon = if attention {
        IconState::Attention
    } else if watcher_running || !s.hooks.is_empty() {
        IconState::Recording
    } else {
        IconState::Idle
    };
    let preset = s.preset.as_deref().unwrap_or("custom");

    // 1. the state, in one glance
    let (title, glyph) = match icon {
        IconState::Recording => ("Recording", Glyph::Good),
        IconState::Idle => ("Nothing records live", Glyph::Off),
        IconState::Attention => ("Needs attention", Glyph::Warn),
    };
    items.push(label_with(glyph, title));
    let mut detail = format!("   {preset}");
    if !s.hooks.is_empty() {
        detail.push_str(&format!(" · hooks: {}", s.hooks.join(", ")));
    }
    items.push(label(detail));
    for w in &s.warnings {
        items.push(label_with(Glyph::Caution, short(w, 56)));
    }
    items.push(Item::Separator);

    // 2. the project, what happened last, and what waits for a person
    if s.projects.is_empty() {
        items.push(label("No project yet: run `tracewise init` in a folder"));
    } else {
        let current = s.projects.iter().find(|p| Some(&p.path) == s.current.as_ref()).or(s.projects.first());
        let list: Vec<Item> = s
            .projects
            .iter()
            .map(|p| Item::Check {
                id: format!("project:{}", p.path),
                text: match p.changes {
                    Some(n) => format!("{}  ·  {n} changes", p.name),
                    None => p.name.clone(),
                },
                checked: Some(&p.path) == current.map(|c| &c.path),
            })
            .collect();
        items.push(Item::Submenu { text: current.map(|c| c.name.clone()).unwrap_or_else(|| "Project".to_string()), glyph: Some(Glyph::Folder), items: list });
    }
    if let Some(last) = &s.last {
        items.push(label(format!("   {} · {}", last.who, last.age)));
        if let Some(reason) = last.reason.as_deref() {
            items.push(label(format!("   “{}”", short(reason, 48))));
        }
    }
    if s.pending > 0 {
        items.push(Item::Action {
            id: "open-app".to_string(),
            text: format!("{} change{} without a reason…", s.pending, if s.pending == 1 { "" } else { "s" }),
            enabled: true,
            glyph: Some(Glyph::Caution),
            key: None,
        });
    }
    items.push(Item::Separator);

    // 3. the watcher
    match s.watcher {
        Some(Watcher::Running) => {
            items.push(label_with(Glyph::Good, "Watcher running"));
            items.push(action("watcher:stop", "Stop watcher", Some(Glyph::Stop), None));
        }
        Some(Watcher::Stopped) => {
            items.push(label_with(Glyph::Warn, "Watcher installed, not running"));
            items.push(action("watcher:start", "Start watcher", Some(Glyph::Play), None));
        }
        _ => {
            items.push(label_with(Glyph::Off, "No watcher in this preset"));
            items.push(action("watcher:start", "Start watcher", Some(Glyph::Play), None));
        }
    }
    items.push(Item::Separator);

    // 3b. what can be done about what is there, each asking first
    let mut acted = false;
    if s.pending_id.is_some() {
        items.push(action("add-reason", "Add a reason to the latest change…", Some(Glyph::Settings), None));
        acted = true;
    }
    if let Some(agent) = &s.agent {
        items.push(action("undo-agent", &format!("Undo {agent}’s last half hour…"), Some(Glyph::Undo), None));
        acted = true;
    }
    if !s.jobs.is_empty() {
        let list = s.jobs.iter().map(|j| Item::Action { id: format!("job:{}", j.id), text: format!("Stop {}", short(&j.text, 40)), enabled: true, glyph: Some(Glyph::Stop), key: None }).collect();
        items.push(Item::Submenu { text: format!("Background jobs ({})", s.jobs.len()), glyph: None, items: list });
        acted = true;
    }
    if !s.locks.is_empty() {
        let list = s.locks.iter().map(|l| Item::Action { id: format!("lock:{l}"), text: format!("Release {}", short(l, 40)), enabled: true, glyph: None, key: None }).collect();
        items.push(Item::Submenu { text: format!("Locks ({})", s.locks.len()), glyph: None, items: list });
        acted = true;
    }
    if s.share_on {
        items.push(Item::Submenu {
            text: "Share".to_string(),
            glyph: None,
            items: vec![action("share:push", "Send my history", None, None), action("share:fetch", "Get everyone else’s", None, None)],
        });
        acted = true;
    }
    items.push(action("verify", "Verify history", None, None));
    let _ = acted;
    items.push(Item::Separator);

    // 4. where to go
    items.push(action("open-app", "Open Tracewise…", Some(Glyph::App), Some("o")));
    items.push(Item::Action {
        id: "open-terminal".to_string(),
        text: "Open project in Terminal".to_string(),
        enabled: !s.projects.is_empty(),
        glyph: Some(Glyph::Terminal),
        key: Some("t"),
    });
    items.push(action("change-preset", "Change how much it watches…", Some(Glyph::Settings), None));
    items.push(Item::Submenu {
        text: "Preferences".to_string(),
        glyph: None,
        items: vec![
            Item::Check { id: "pref:login".to_string(), text: "Start at login".to_string(), checked: s.login_item },
            Item::Check { id: "pref:notify".to_string(), text: "Notifications".to_string(), checked: s.notifications },
        ],
    });
    items.push(Item::Separator);
    items.push(action("quit", "Quit Tracewise", None, Some("q")));
    Model { icon, tooltip: format!("Tracewise: {} · {preset}", title.to_lowercase()), items }
}

// ---- reading the state ----

/// One `tracewise` command, run like the app runs them (allow-listed, as the person, with a time limit); the parsed JSON when it
/// succeeded.
fn ask(args: &[&str], repo: Option<&std::path::Path>) -> Result<Value, String> {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    validate_tracewise_args(&args)?;
    let bin = locate_tracewise_binary()?;
    let mut cmd = build_tracewise_command(&bin, &args, repo);
    let out = run_bounded(&mut cmd, ASK_TIMEOUT, MAX_BYTES)?;
    if out.exit_code != 0 {
        return Err(out.stderr.lines().next().unwrap_or("tracewise failed").to_string());
    }
    serde_json::from_str(&out.stdout).map_err(|e| format!("tracewise answered in an unexpected form: {e}"))
}

fn who_of(row: &Value) -> String {
    let actor = &row["actor"];
    let name = match actor["kind"].as_str() {
        Some("human") => "you".to_string(),
        _ => actor["name"].as_str().or(actor["kind"].as_str()).unwrap_or("unknown").to_string(),
    };
    let via = match row["source"].as_str() {
        Some("zed") => " via Zed",
        Some("vscode") => " via VS Code",
        Some("jetbrains") => " via JetBrains",
        _ => "",
    };
    if actor["kind"] == "human" {
        format!("{name}{via}")
    } else {
        name
    }
}

/// `3 min ago`, from an RFC 3339 time and the current time (both as seconds since 1970).
pub fn age_text(then: i64, now: i64) -> String {
    let secs = (now - then).max(0);
    match secs {
        0..=44 => "just now".to_string(),
        45..=3_599 => format!("{} min ago", (secs + 30) / 60),
        3_600..=86_399 => format!("{} h ago", (secs + 1_800) / 3_600),
        _ => format!("{} d ago", (secs + 43_200) / 86_400),
    }
}

/// Seconds since 1970 of an RFC 3339 UTC time such as `2026-11-23T08:49:02.216Z`; None when it does not read like one.
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let (date, time) = text.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.trim_end_matches('Z');
    let mut t = time.split(':');
    let (h, mi) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let s = t.next()?.split('.').next()?.parse::<i64>().ok()?;
    // days since 1970 (Howard Hinnant's civil-from-days, inverted)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3_600 + mi * 60 + s)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The project the menu is about until one is chosen: the first that is not in the system's temporary folder (a folder that
/// something once initialised by mistake), else the first.
pub fn default_project(projects: &[Project]) -> Option<String> {
    let tmp = std::fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
    let outside = |p: &&Project| !std::fs::canonicalize(&p.path).unwrap_or_else(|_| p.path.clone().into()).starts_with(&tmp);
    projects.iter().find(outside).or(projects.first()).map(|p| p.path.clone())
}

/// Asks `tracewise` for everything the menu shows. A failure of the first question (no program, an old program) is the snapshot's
/// `problem`; a failure of a later one just leaves that line out.
pub fn gather(current: Option<&str>) -> Snapshot {
    let mut s = Snapshot { current: current.map(String::from), ..Snapshot::default() };
    let capture = match ask(&["capture", "--json"], None) {
        Ok(c) => c,
        Err(e) => {
            s.problem = Some(if e.contains("not found") { "tracewise is not installed: open Tracewise to install it".to_string() } else { format!("tracewise: {e}") });
            return s;
        }
    };
    s.preset = capture["preset"].as_str().map(String::from);
    let layers = &capture["layers"];
    let installed = layers["daemon"].as_bool().unwrap_or(false);
    let running = layers["watcher_running"].as_bool().unwrap_or(false);
    s.watcher = Some(match (installed, running) {
        (_, true) => Watcher::Running,
        (true, false) => Watcher::Stopped,
        (false, false) => Watcher::NotSet,
    });
    s.hooks = layers["hooks"].as_array().map(|a| a.iter().filter_map(|h| h.as_str().map(String::from)).collect()).unwrap_or_default();
    s.warnings = capture["problems"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(String::from)).collect()).unwrap_or_default();

    if let Ok(repos) = ask(&["capture", "repos", "--json"], None) {
        s.projects = repos
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|r| {
                        let path = r["work_tree"].as_str()?.to_string();
                        let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
                        Some(Project { path, name, changes: r["changes"].as_u64(), data_dir: r["data_dir"].as_str().map(String::from) })
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
    let repo = s.current.clone().or_else(|| default_project(&s.projects));
    if let Some(repo) = repo {
        s.current = Some(repo.clone());
        let dir = std::path::Path::new(&repo);
        if let Ok(rows) = ask(&["log", "--limit", "50", "--json"], Some(dir)) {
            let rows = rows.as_array().cloned().unwrap_or_default();
            if let Some(row) = rows.first() {
                let age = row["time"]["start"].as_str().and_then(parse_rfc3339).map(|t| age_text(t, now_secs())).unwrap_or_default();
                s.last = Some(Last { who: who_of(row), age, reason: row["why"].as_str().filter(|w| !w.is_empty()).map(String::from) });
            }
            // the agent whose last half hour "Undo" would take back: the one that made the newest agent change
            s.agent = rows.iter().find(|r| r["actor"]["kind"] == "agent").and_then(|r| r["actor"]["name"].as_str()).map(String::from);
        }
        if let Ok(rows) = ask(&["pending", "--json"], Some(dir)) {
            let rows = rows.as_array().cloned().unwrap_or_default();
            s.pending = rows.len();
            s.pending_id = rows.first().and_then(|r| r["id"].as_str()).map(String::from);
        }
        if let Ok(rows) = ask(&["jobs", "--json"], Some(dir)) {
            s.jobs = rows
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|j| Some(Job { id: j["id"].as_str()?.to_string(), text: j["command"].as_str().unwrap_or("background work").to_string() }))
                        .collect()
                })
                .unwrap_or_default();
        }
        if let Ok(rows) = ask(&["lock", "list", "--json"], Some(dir)) {
            s.locks = rows.as_array().map(|a| a.iter().filter_map(|l| l["path"].as_str().map(String::from)).collect()).unwrap_or_default();
        }
        if let Ok(status) = ask(&["status", "--json"], Some(dir)) {
            s.share_on = !status["share"].is_null();
        }
    }
    s
}

// ---- the icon ----

/// A template icon (black and transparent, so macOS draws it for the light and dark menu bar) for a state: a filled disc, a ring,
/// or a ring with an exclamation mark. 44x44 pixels for a 22-point icon.
pub fn icon_rgba(state: IconState) -> (Vec<u8>, u32, u32) {
    const N: u32 = 44;
    let mut px = vec![0u8; (N * N * 4) as usize];
    let c = (N as f32 - 1.0) / 2.0;
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 - c, y as f32 - c);
            let r = (dx * dx + dy * dy).sqrt();
            let on = match state {
                IconState::Recording => r <= 13.0,
                IconState::Idle => (9.5..=13.0).contains(&r),
                IconState::Attention => {
                    (9.5..=13.0).contains(&r)
                        // the exclamation mark: a bar and a dot
                        || (dx.abs() <= 2.0 && (-8.0..=2.5).contains(&dy))
                        || (dx.abs() <= 2.2 && (5.0..=9.0).contains(&dy))
                }
            };
            if on {
                let i = ((y * N + x) * 4) as usize;
                px[i + 3] = 255;
            }
        }
    }
    (px, N, N)
}

// ---- the tray ----

/// Which project the menu is about (set by choosing one in the menu).
#[derive(Default)]
pub struct Current(pub Mutex<Option<String>>);

use tauri::menu::{CheckMenuItem, IconMenuItem, Menu, MenuItem, NativeIcon, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime};

fn native(glyph: Glyph) -> NativeIcon {
    match glyph {
        Glyph::Good => NativeIcon::StatusAvailable,
        Glyph::Warn => NativeIcon::StatusPartiallyAvailable,
        Glyph::Off => NativeIcon::StatusNone,
        Glyph::Caution => NativeIcon::Caution,
        Glyph::Folder => NativeIcon::Folder,
        Glyph::Play => NativeIcon::RightFacingTriangle,
        Glyph::Stop => NativeIcon::StopProgress,
        Glyph::App => NativeIcon::Computer,
        Glyph::Terminal => NativeIcon::Path,
        Glyph::Settings => NativeIcon::PreferencesGeneral,
        Glyph::Undo => NativeIcon::GoLeft,
    }
}

/// Adds one row to a menu or a submenu.
macro_rules! add_item {
    ($app:expr, $target:expr, $item:expr) => {
        match $item {
            Item::Label { text, glyph: None } => $target.append(&MenuItem::new($app, text, false, None::<&str>)?),
            Item::Label { text, glyph: Some(g) } => $target.append(&IconMenuItem::with_native_icon($app, text, false, Some(native(*g)), None::<&str>)?),
            Item::Action { id, text, enabled, glyph, key } => {
                let accel = key.map(|k| format!("CmdOrCtrl+{}", k.to_uppercase()));
                match glyph {
                    Some(g) => $target.append(&IconMenuItem::with_id_and_native_icon($app, id, text, *enabled, Some(native(*g)), accel.as_deref())?),
                    None => $target.append(&MenuItem::with_id($app, id, text, *enabled, accel.as_deref())?),
                }
            }
            Item::Check { id, text, checked } => $target.append(&CheckMenuItem::with_id($app, id, text, true, *checked, None::<&str>)?),
            Item::Separator => $target.append(&PredefinedMenuItem::separator($app)?),
            Item::Submenu { .. } => Ok(()),
        }
    };
}

fn menu_of<R: Runtime>(app: &AppHandle<R>, items: &[Item]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for item in items {
        match item {
            Item::Submenu { text, glyph, items } => {
                let sub = Submenu::new(app, text, true)?;
                for inner in items {
                    add_item!(app, sub, inner)?;
                }
                // a submenu has no picture of its own in this toolkit: the folder is part of its title
                let _ = glyph;
                menu.append(&sub)?;
            }
            other => add_item!(app, menu, other)?,
        }
    }
    Ok(menu)
}

/// Puts the model on the tray icon (on the main thread, which the menu needs).
fn show<R: Runtime>(app: &AppHandle<R>, model: Model) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id("tracewise") else { return };
        let (rgba, w, h) = icon_rgba(model.icon);
        let _ = tray.set_icon(Some(tauri::image::Image::new_owned(rgba, w, h)));
        let _ = tray.set_icon_as_template(true);
        let _ = tray.set_tooltip(Some(&model.tooltip));
        if let Ok(menu) = menu_of(&handle, &model.items) {
            let _ = tray.set_menu(Some(menu));
        }
    });
}

/// What the refresh remembers from one time to the next.
#[derive(Default)]
pub struct Live {
    model: Mutex<Option<Model>>,
    snapshot: Mutex<Option<Snapshot>>,
    last_pending_note: Mutex<Option<std::time::Instant>>,
}

/// How many changes without a reason are worth a notification, and how often at most.
const PENDING_NOTE_AT: usize = 5;
const PENDING_NOTE_EVERY: Duration = Duration::from_secs(3_600);

/// The notifications a change from `prev` to `now` deserves: the watcher stopped on its own, a new warning, many changes waiting for
/// a reason (once an hour at most). Nothing for the first snapshot or for ordinary changes.
pub fn notifications(prev: Option<&Snapshot>, now: &Snapshot, pending_note_allowed: bool) -> Vec<(String, String)> {
    let Some(prev) = prev else { return Vec::new() };
    let mut out = Vec::new();
    if prev.watcher == Some(Watcher::Running) && now.watcher == Some(Watcher::Stopped) {
        out.push(("Tracewise".to_string(), "The watcher stopped. Open the menu to start it again.".to_string()));
    }
    for w in now.warnings.iter().filter(|w| !prev.warnings.contains(w)) {
        out.push(("Tracewise needs a look".to_string(), short(w, 120)));
    }
    if now.pending >= PENDING_NOTE_AT && pending_note_allowed {
        out.push(("Tracewise".to_string(), format!("{} changes have no reason yet.", now.pending)));
    }
    out
}

fn tracewise_home() -> std::path::PathBuf {
    match std::env::var_os("TRACEWISE_HOME") {
        Some(h) => std::path::PathBuf::from(h),
        None => std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".tracewise"),
    }
}

fn home() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// Reads the state and shows it, unless nothing changed since the last time; sends the notifications that are due. Returns the
/// directories to watch for changes (the histories of the listed projects, and Tracewise's own home).
fn refresh<R: Runtime>(app: &AppHandle<R>) -> Vec<std::path::PathBuf> {
    let live = app.state::<Live>();
    let current = app.state::<Current>().0.lock().ok().and_then(|c| c.clone());
    let mut snapshot = gather(current.as_deref());
    snapshot.login_item = crate::os::login_item_enabled(&home());
    snapshot.notifications = crate::os::read_prefs(&tracewise_home()).notifications;
    let dirs: Vec<std::path::PathBuf> =
        snapshot.projects.iter().filter_map(|p| p.data_dir.as_ref().map(std::path::PathBuf::from)).chain(std::iter::once(tracewise_home())).collect();
    if snapshot.notifications {
        let allowed = live.last_pending_note.lock().ok().is_none_or(|t| t.is_none_or(|t| t.elapsed() >= PENDING_NOTE_EVERY));
        let prev = live.snapshot.lock().ok().and_then(|s| s.clone());
        let due = notifications(prev.as_ref(), &snapshot, allowed);
        if due.iter().any(|(_, text)| text.ends_with("have no reason yet.")) {
            if let Ok(mut t) = live.last_pending_note.lock() {
                *t = Some(std::time::Instant::now());
            }
        }
        for (title, text) in due {
            crate::os::notify(&title, &text);
        }
    }
    if let Ok(mut s) = live.snapshot.lock() {
        *s = Some(snapshot.clone());
    }
    let model = build_model(&snapshot);
    let changed = live.model.lock().map(|mut last| {
        let changed = last.as_ref() != Some(&model);
        if changed {
            *last = Some(model.clone());
        }
        changed
    });
    if changed.unwrap_or(true) {
        show(app, model);
    }
    dirs
}

/// One `tracewise` command the menu runs: its output, or what went wrong.
fn run_text(args: &[&str], repo: Option<&str>) -> Result<(String, String, i32), String> {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    validate_tracewise_args(&args)?;
    let bin = locate_tracewise_binary()?;
    let mut cmd = build_tracewise_command(&bin, &args, repo.map(std::path::Path::new));
    run_bounded(&mut cmd, Duration::from_secs(60), MAX_BYTES).map(|r| (r.stdout, r.stderr, r.exit_code))
}

/// The first few lines of an output, for a dialog.
fn first_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut out = lines.iter().take(n).copied().collect::<Vec<_>>().join("\n");
    if lines.len() > n {
        out.push_str(&format!("\n… and {} more", lines.len() - n));
    }
    out
}

/// What a click on one of the rows that change something does: say what will happen, ask, run, and say how it went. Runs on its own
/// thread, because a dialog waits for the person.
fn perform(id: &str, repo: Option<String>) {
    use crate::os::{ask_text, confirm, inform};
    let repo = repo.as_deref();
    let result = |what: &str, r: Result<(String, String, i32), String>| match r {
        Ok((out, _, 0)) => inform(what, &first_lines(&out, 6)),
        Ok((out, err, _)) => inform(what, &format!("It did not work:\n{}", first_lines(if err.trim().is_empty() { &out } else { &err }, 6))),
        Err(e) => inform(what, &format!("It did not work: {e}")),
    };
    if id == "verify" {
        result("Verify history", run_text(&["doctor", "--verify"], repo));
    } else if let Some(job) = id.strip_prefix("job:") {
        if confirm("Stop background job", "Stop attributing later writes to this background job to the agent that started it?", "Stop") {
            result("Stop background job", run_text(&["jobs", "--stop", job], repo));
        }
    } else if let Some(path) = id.strip_prefix("lock:") {
        if confirm("Release lock", &format!("Release the lock on {path}? Whoever holds it may then be edited over."), "Release") {
            result("Release lock", run_text(&["lock", "release", path], repo));
        }
    } else if id == "share:push" || id == "share:fetch" {
        let which = if id.ends_with("push") { "push" } else { "fetch" };
        result(if which == "push" { "Send history" } else { "Get history" }, run_text(&["share", which], repo));
    } else if id == "add-reason" {
        let Ok((out, _, 0)) = run_text(&["pending", "--json"], repo) else { return inform("Add a reason", "Could not read the changes without a reason.") };
        let rows: Value = serde_json::from_str(&out).unwrap_or_default();
        let Some(change) = rows.as_array().and_then(|a| a.first()) else { return inform("Add a reason", "Every change has a reason.") };
        let (cid, path) = (change["id"].as_str().unwrap_or(""), change["path"].as_str().unwrap_or(""));
        if let Some(text) = ask_text("Add a reason", &format!("Why was {path} changed? (at least 12 characters, say why and not what)"), "Save") {
            result("Add a reason", run_text(&["note", cid, &text], repo));
        }
    } else if id == "undo-agent" {
        let Ok((out, _, 0)) = run_text(&["log", "--limit", "50", "--json"], repo) else { return inform("Undo", "Could not read the history.") };
        let rows: Value = serde_json::from_str(&out).unwrap_or_default();
        let Some(agent) = rows.as_array().and_then(|a| a.iter().find(|r| r["actor"]["kind"] == "agent")).and_then(|r| r["actor"]["name"].as_str()).map(String::from) else {
            return inform("Undo", "No agent has changed anything here.");
        };
        // what would happen first: the dry run, so the question says what will be undone
        let dry = run_text(&["undo", "--actor", &agent, "--since", "30m", "--dry-run"], repo);
        let plan = match &dry {
            Ok((out, _, 0)) if !out.trim().is_empty() => first_lines(out, 8),
            Ok(_) => return inform("Undo", &format!("{agent} has changed nothing in the last half hour.")),
            Err(e) => return inform("Undo", &format!("It did not work: {e}")),
        };
        if confirm("Undo", &format!("Undo everything {agent} did in the last 30 minutes? Your own edits stay.\n\n{plan}"), "Undo") {
            result("Undo", run_text(&["undo", "--actor", &agent, "--since", "30m"], repo));
        }
    }
}

/// Shows the main window and the Dock icon.
fn open_app<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Hides the window and the Dock icon: the app goes on living in the menu bar.
pub fn hide_to_menu_bar<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

fn open_terminal(path: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").args(["-a", "Terminal", path]).spawn();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = path;
}

/// Sets the tray up: the icon, its menu, the clicks, and a refresh when the system reports a write to a history.
pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    use tauri::tray::TrayIconBuilder;
    let (rgba, w, h) = icon_rgba(IconState::Idle);
    let initial = menu_of(app, &[label("Tracewise"), Item::Separator, action("open-app", "Open Tracewise…", Some(Glyph::App), Some("o")), action("quit", "Quit Tracewise", None, Some("q"))])?;
    TrayIconBuilder::with_id("tracewise")
        .icon(tauri::image::Image::new_owned(rgba, w, h))
        .icon_as_template(true)
        .menu(&initial)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref().to_string();
            let handle = app.clone();
            let current = app.state::<Current>().0.lock().ok().and_then(|c| c.clone());
            match id.as_str() {
                "quit" => app.exit(0),
                "open-app" => open_app(app),
                "watcher:start" | "watcher:stop" => {
                    let which = if id.ends_with("start") { "start" } else { "stop" };
                    std::thread::spawn(move || {
                        let _ = run_text(&["capture", "daemon", which], None);
                        handle.state::<crate::watch::Watcher>().poke();
                    });
                }
                "open-terminal" | "change-preset" => open_terminal(&current.unwrap_or_else(|| std::env::var("HOME").unwrap_or_default())),
                "pref:login" => {
                    std::thread::spawn(move || {
                        if let Ok(exe) = std::env::current_exe() {
                            let on = crate::os::login_item_enabled(&home());
                            let _ = crate::os::set_login_item(&home(), &exe, !on);
                        }
                        refresh(&handle);
                    });
                }
                "pref:notify" => {
                    std::thread::spawn(move || {
                        let prefs = crate::os::read_prefs(&tracewise_home());
                        let _ = crate::os::write_prefs(&tracewise_home(), crate::os::Prefs { notifications: !prefs.notifications });
                        refresh(&handle);
                    });
                }
                other => {
                    if let Some(path) = other.strip_prefix("project:") {
                        if let Ok(mut c) = app.state::<Current>().0.lock() {
                            *c = Some(path.to_string());
                        }
                        // the open window follows the choice
                        use tauri::Emitter;
                        let _ = app.emit("switch-repo", path.to_string());
                        std::thread::spawn(move || {
                            refresh(&handle);
                        });
                    } else {
                        let other = other.to_string();
                        std::thread::spawn(move || {
                            perform(&other, current);
                            refresh(&handle);
                        });
                    }
                }
            }
        })
        .build(app)?;
    // Read again when the system reports a write to a history (not on a timer), and now and then in case a report was missed.
    let (watcher, signals) = crate::watch::Watcher::new();
    app.manage(watcher);
    let handle = app.clone();
    std::thread::spawn(move || {
        use tauri::Emitter;
        loop {
            let dirs = refresh(&handle);
            handle.state::<crate::watch::Watcher>().ensure(&dirs);
            match signals.recv_timeout(Duration::from_secs(60)) {
                Ok(()) => {
                    // the open window reads again too
                    let _ = handle.emit("history-changed", ());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            preset: Some("balanced".into()),
            watcher: Some(Watcher::NotSet),
            hooks: vec!["claude".into()],
            projects: vec![
                Project { path: "/a/tw".into(), name: "tw".into(), changes: Some(40), data_dir: None },
                Project { path: "/a/other".into(), name: "other".into(), changes: None, data_dir: None },
            ],
            current: Some("/a/tw".into()),
            last: Some(Last { who: "you via Zed".into(), age: "2 min ago".into(), reason: Some("New feature".into()) }),
            ..Snapshot::default()
        }
    }

    fn texts(m: &Model) -> Vec<String> {
        m.items
            .iter()
            .filter_map(|i| match i {
                Item::Label { text: t, .. } | Item::Action { text: t, .. } | Item::Submenu { text: t, .. } => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_working_setup_shows_recording_the_project_the_last_change_and_the_actions() {
        let m = build_model(&snapshot());
        assert_eq!(m.icon, IconState::Recording);
        let t = texts(&m);
        assert_eq!(t[0], "Recording");
        assert!(t[1].contains("balanced") && t[1].contains("hooks: claude"), "{t:?}");
        assert!(t.iter().any(|x| x == "tw"), "the project is the title of its submenu: {t:?}");
        assert!(t.iter().any(|x| x.contains("you via Zed · 2 min ago")), "{t:?}");
        assert!(t.iter().any(|x| x.contains("“New feature”")), "{t:?}");
        assert!(t.iter().any(|x| x == "No watcher in this preset"), "{t:?}");
        assert!(t.iter().any(|x| x == "Open Tracewise…") && t.iter().any(|x| x == "Quit Tracewise"));
        assert!(!t.iter().any(|x| x.to_lowercase().contains("controller")), "the menu never calls itself a controller: {t:?}");
        // the current project is ticked in the submenu
        let Some(Item::Submenu { items, .. }) = m.items.iter().find(|i| matches!(i, Item::Submenu { .. })) else { panic!("no project menu") };
        assert!(matches!(&items[0], Item::Check { checked: true, id, .. } if id == "project:/a/tw"));
        assert!(matches!(&items[1], Item::Check { checked: false, .. }));
    }

    #[test]
    fn the_watcher_row_offers_stop_when_it_runs_and_start_when_it_does_not() {
        let mut s = snapshot();
        s.watcher = Some(Watcher::Running);
        let ids = |m: &Model| m.items.iter().filter_map(|i| if let Item::Action { id, .. } = i { Some(id.clone()) } else { None }).collect::<Vec<_>>();
        let running = build_model(&s);
        assert!(ids(&running).contains(&"watcher:stop".to_string()) && !ids(&running).contains(&"watcher:start".to_string()));
        assert_eq!(running.icon, IconState::Recording);
        s.watcher = Some(Watcher::Stopped);
        let stopped = build_model(&s);
        assert!(ids(&stopped).contains(&"watcher:start".to_string()));
        assert_eq!(stopped.icon, IconState::Attention, "a watcher that is installed but not running needs a look");
    }

    #[test]
    fn warnings_pending_changes_and_a_missing_program_are_shown() {
        let mut s = snapshot();
        s.warnings = vec!["capture.mode is `full` but no watcher is installed".into()];
        s.pending = 3;
        let m = build_model(&s);
        assert_eq!(m.icon, IconState::Attention);
        let t = texts(&m);
        assert!(t.iter().any(|x| x.contains("capture.mode is `full`")), "{t:?}");
        assert!(t.iter().any(|x| x == "3 changes without a reason…"), "{t:?}");
        let missing = build_model(&Snapshot { problem: Some("tracewise is not installed: open Tracewise to install it".into()), ..Snapshot::default() });
        assert_eq!(missing.icon, IconState::Attention);
        assert!(texts(&missing).iter().any(|x| x.contains("not installed")));
        assert!(texts(&missing).iter().any(|x| x == "Open Tracewise…"));
        let idle = build_model(&Snapshot { preset: Some("agents".into()), watcher: Some(Watcher::NotSet), ..Snapshot::default() });
        assert_eq!(idle.icon, IconState::Idle);
        assert!(texts(&idle).iter().any(|x| x.starts_with("No project")));
        assert!(idle.items.iter().any(|i| matches!(i, Item::Action { id, enabled: false, .. } if id == "open-terminal")), "no project: no terminal");
    }

    #[test]
    fn a_long_reason_is_cut_at_a_word_to_fit_a_menu_row() {
        assert_eq!(short("fix the empty-input crash", 48), "fix the empty-input crash");
        let cut = short("make the very long reason of this change fit on one line of the menu bar and not run off", 30);
        assert!(cut.chars().count() <= 30 && cut.ends_with('…') && !cut.contains("  "), "{cut}");
        assert_eq!(short("two\nlines   here", 48), "two lines here");
    }

    #[test]
    fn rows_that_change_something_appear_only_when_there_is_something_to_change() {
        let plain = build_model(&snapshot());
        let ids = |m: &Model| -> Vec<String> {
            fn walk(items: &[Item], out: &mut Vec<String>) {
                for i in items {
                    match i {
                        Item::Action { id, .. } | Item::Check { id, .. } => out.push(id.clone()),
                        Item::Submenu { items, .. } => walk(items, out),
                        _ => {}
                    }
                }
            }
            let mut out = Vec::new();
            walk(&m.items, &mut out);
            out
        };
        let plain_ids = ids(&plain);
        for absent in ["add-reason", "undo-agent", "share:push"] {
            assert!(!plain_ids.iter().any(|i| i == absent), "{absent} without anything to act on: {plain_ids:?}");
        }
        assert!(plain_ids.contains(&"verify".to_string()) && plain_ids.contains(&"pref:login".to_string()) && plain_ids.contains(&"pref:notify".to_string()));
        let mut s = snapshot();
        s.pending = 2;
        s.pending_id = Some("c1".into());
        s.agent = Some("claude".into());
        s.jobs = vec![Job { id: "j1".into(), text: "cargo watch -x test".into() }];
        s.locks = vec!["src/a.rs".into()];
        s.share_on = true;
        s.login_item = true;
        let busy = ids(&build_model(&s));
        for present in ["add-reason", "undo-agent", "job:j1", "lock:src/a.rs", "share:push", "share:fetch"] {
            assert!(busy.iter().any(|i| i == present), "{present}: {busy:?}");
        }
        let t = texts(&build_model(&s));
        assert!(t.iter().any(|x| x == "Undo claude’s last half hour…") && t.iter().any(|x| x == "Background jobs (1)") && t.iter().any(|x| x == "Locks (1)"), "{t:?}");
        let Some(Item::Submenu { items, .. }) = build_model(&s).items.iter().find(|i| matches!(i, Item::Submenu { text, .. } if text == "Preferences")).cloned() else { panic!("no preferences") };
        assert!(matches!(&items[0], Item::Check { id, checked: true, .. } if id == "pref:login"));
        assert!(matches!(&items[1], Item::Check { id, checked: false, .. } if id == "pref:notify"), "notifications are off until turned on");
    }

    #[test]
    fn notifications_are_for_a_stopped_watcher_a_new_warning_and_many_changes_waiting_and_never_the_first_time() {
        let mut before = snapshot();
        before.watcher = Some(Watcher::Running);
        assert!(notifications(None, &before, true).is_empty(), "the first snapshot is only remembered");
        let mut now = before.clone();
        assert!(notifications(Some(&before), &now, true).is_empty(), "nothing changed: nothing to say");
        now.watcher = Some(Watcher::Stopped);
        assert!(notifications(Some(&before), &now, true).iter().any(|(_, t)| t.contains("watcher stopped")));
        now.watcher = Some(Watcher::Running);
        now.warnings = vec!["capture.mode is `full` but no watcher is installed".into()];
        assert_eq!(notifications(Some(&before), &now, true).len(), 1);
        assert!(notifications(Some(&now), &now, true).is_empty(), "a warning is said once");
        now.warnings.clear();
        now.pending = 7;
        assert!(notifications(Some(&before), &now, true).iter().any(|(_, t)| t == "7 changes have no reason yet."));
        assert!(notifications(Some(&before), &now, false).is_empty(), "once an hour at most");
        now.pending = 2;
        assert!(notifications(Some(&before), &now, true).is_empty(), "a few are not worth a notification");
    }

    #[test]
    fn the_default_project_skips_the_temporary_folder() {
        let tmp = std::env::temp_dir().to_string_lossy().into_owned();
        let projects = vec![Project { path: tmp, name: "T".into(), changes: None, data_dir: None }, Project { path: "/Users/x/real".into(), name: "real".into(), changes: None, data_dir: None }];
        assert_eq!(default_project(&projects).as_deref(), Some("/Users/x/real"));
        assert_eq!(default_project(&projects[..1]).as_deref(), projects[0].path.as_str().into(), "only a temporary project: it is still the default");
        assert_eq!(default_project(&[]), None);
    }

    #[test]
    fn times_are_read_and_worded() {
        let then = parse_rfc3339("2026-11-23T08:49:02.216Z").unwrap();
        assert_eq!(then, 1_795_423_742);
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("not a time"), None);
        assert_eq!(age_text(then, then + 10), "just now");
        assert_eq!(age_text(then, then + 120), "2 min ago");
        assert_eq!(age_text(then, then + 7_300), "2 h ago");
        assert_eq!(age_text(then, then + 3 * 86_400), "3 d ago");
        assert_eq!(age_text(then, then - 50), "just now");
    }

    #[test]
    fn the_person_and_the_editor_are_named_and_agents_are_not_via_an_editor() {
        let person = serde_json::json!({ "actor": { "kind": "human", "name": "as" }, "source": "zed" });
        assert_eq!(who_of(&person), "you via Zed");
        let agent = serde_json::json!({ "actor": { "kind": "agent", "name": "claude" }, "source": "zed" });
        assert_eq!(who_of(&agent), "claude");
        assert_eq!(who_of(&serde_json::json!({ "actor": { "kind": "human" }, "source": "scan" })), "you");
    }

    #[test]
    fn each_state_has_its_own_icon_of_the_right_size() {
        let sizes: Vec<usize> = [IconState::Recording, IconState::Idle, IconState::Attention].iter().map(|s| icon_rgba(*s).0.iter().skip(3).step_by(4).filter(|a| **a > 0).count()).collect();
        assert!(sizes[0] > sizes[1], "a filled disc has more pixels than a ring: {sizes:?}");
        assert_ne!(sizes[1], sizes[2]);
        let (px, w, h) = icon_rgba(IconState::Idle);
        assert_eq!((w, h, px.len()), (44, 44, 44 * 44 * 4));
    }

    #[test]
    fn every_command_the_menu_runs_is_allowed_and_nothing_else_is() {
        for args in [vec!["jobs", "--json"], vec!["jobs", "--stop", "j1"], vec!["lock", "list", "--json"], vec!["lock", "release", "a.rs"], vec!["doctor", "--verify"], vec!["share", "push"], vec!["share", "fetch"], vec!["note", "c1", "why this changed"], vec!["undo", "--actor", "claude", "--since", "30m", "--dry-run"], vec!["undo", "--actor", "claude", "--since", "30m"], vec!["status", "--json"], vec!["capture", "--json"], vec!["capture", "repos", "--json"], vec!["capture", "daemon", "start"], vec!["capture", "daemon", "stop"], vec!["log", "--limit", "1", "--json"], vec!["pending", "--json"]] {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            assert!(validate_tracewise_args(&args).is_ok(), "{args:?}");
        }
        let refused: Vec<String> = ["capture", "use", "full"].iter().map(|a| a.to_string()).collect();
        assert!(validate_tracewise_args(&refused).is_err(), "the menu never applies a preset");
    }
}
