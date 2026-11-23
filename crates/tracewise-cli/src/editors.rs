//! `tracewise editors zed`: the task and key that ask "why?" in Zed's own terminal. An extension cannot ship these, so this writes them.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const LABEL: &str = "Tracewise: why";
const SCRIPT: &str = "printf 'why? '; read r; tracewise note \"$(tracewise log -l 1 | cut -d' ' -f1)\" \"$r\"";

fn task() -> Value {
    json!({ "label": LABEL, "command": "sh", "args": ["-c", SCRIPT], "use_new_terminal": true, "reveal": "always" })
}

fn binding(key: &str) -> Value {
    json!({ "bindings": { key: ["task::Spawn", { "task_name": LABEL }] } })
}

/// The new text of tasks.json: our task added, or replaced when it is already there. None when the file is not plain JSON array.
fn merge_tasks(existing: Option<&str>) -> Option<String> {
    let mut list = match existing {
        None => Vec::new(),
        Some(t) if t.trim().is_empty() => Vec::new(),
        Some(t) => serde_json::from_str::<Value>(t).ok()?.as_array()?.clone(),
    };
    list.retain(|t| t["label"] != LABEL);
    list.push(task());
    serde_json::to_string_pretty(&list).ok().map(|s| s + "\n")
}

/// The new text of keymap.json; Err(reason) when the key is taken by something else, None when the file is not plain JSON.
fn merge_keymap(existing: Option<&str>, key: &str) -> Option<Result<String, String>> {
    let mut list = match existing {
        None => Vec::new(),
        Some(t) if t.trim().is_empty() => Vec::new(),
        Some(t) => serde_json::from_str::<Value>(t).ok()?.as_array()?.clone(),
    };
    let ours = |entry: &Value| entry["bindings"].as_object().is_some_and(|b| b.values().any(|v| v[1]["task_name"] == LABEL));
    list.retain(|e| !ours(e));
    if list.iter().any(|e| e["bindings"].get(key).is_some()) {
        return Some(Err(format!("{key} is already bound to something else in keymap.json")));
    }
    list.push(binding(key));
    Some(Ok(serde_json::to_string_pretty(&list).ok()? + "\n"))
}

fn config_dir(dir: Option<PathBuf>) -> PathBuf {
    dir.or_else(|| std::env::var_os("ZED_CONFIG_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config").join("zed"))
}

fn write_with_backup(path: &Path, text: &str) -> std::io::Result<()> {
    if path.exists() {
        std::fs::copy(path, path.with_extension("json.tracewise-bak"))?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)
}

const WHEN_NAMES: [&str; 5] = ["pause", "every-save", "interval", "leave", "manual"];

#[derive(Default, Clone)]
pub(crate) struct Settings {
    pub when: Vec<String>,
    pub after: Option<f64>,
    pub every: Option<f64>,
    pub hints: bool,
    pub min_chars: Option<u64>,
    pub min_lines: Option<u64>,
    pub ignore: Vec<String>,
}

impl Settings {
    /// True when no option was given: the command then asks.
    fn is_empty(&self) -> bool {
        self.when.is_empty() && self.after.is_none() && self.every.is_none() && !self.hints && self.min_chars.is_none() && self.min_lines.is_none() && self.ignore.is_empty()
    }

    /// The `initialization_options` object. Err names the first thing that is wrong.
    fn options(&self) -> Result<serde_json::Map<String, Value>, String> {
        let when: Vec<String> = if self.when.is_empty() { vec!["pause".into()] } else { self.when.clone() };
        if let Some(bad) = when.iter().find(|w| !WHEN_NAMES.contains(&w.as_str())) {
            return Err(format!("unknown --when value `{bad}`; use {}", WHEN_NAMES.join(", ")));
        }
        for (name, v) in [("--after", self.after), ("--every", self.every)] {
            if v.is_some_and(|v| !v.is_finite() || v < 0.0) {
                return Err(format!("{name} must be a number of seconds, 0 or more"));
            }
        }
        let mut options = serde_json::Map::new();
        options.insert("askWhen".into(), if when.len() == 1 { json!(when[0]) } else { json!(when) });
        if let Some(v) = self.after {
            options.insert("askAfterSeconds".into(), json!(v));
        }
        if let Some(v) = self.every {
            options.insert("askEverySeconds".into(), json!(v));
        }
        if self.hints {
            options.insert("inlayBlame".into(), json!(true));
        }
        if let Some(v) = self.min_chars {
            options.insert("askMinChars".into(), json!(v));
        }
        if let Some(v) = self.min_lines {
            options.insert("askMinLines".into(), json!(v));
        }
        if !self.ignore.is_empty() {
            options.insert("askIgnore".into(), json!(self.ignore));
        }
        Ok(options)
    }
}

/// The `lsp.tracewise` block for Zed's settings.json.
fn settings_block(o: &Settings) -> Result<String, String> {
    let mut root = json!({ "lsp": { "tracewise": { "initialization_options": o.options()? } } });
    if o.hints {
        root["inlay_hints"] = json!({ "enabled": true });
    }
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

// ---- settings.json is JSON with comments: it is edited as text, never parsed and rewritten, so the user's comments stay ----

fn skip_space(t: &[u8], mut i: usize) -> usize {
    loop {
        match t.get(i) {
            Some(c) if c.is_ascii_whitespace() => i += 1,
            Some(b'/') if t.get(i + 1) == Some(&b'/') => i = t[i..].iter().position(|&c| c == b'\n').map_or(t.len(), |n| i + n),
            Some(b'/') if t.get(i + 1) == Some(&b'*') => i = t[i + 2..].windows(2).position(|w| w == b"*/").map_or(t.len(), |n| i + 2 + n + 2),
            _ => return i,
        }
    }
}

/// Index of the `{` that opens the top-level object.
fn top_open(text: &str) -> Option<usize> {
    let i = skip_space(text.as_bytes(), 0);
    (text.as_bytes().get(i) == Some(&b'{')).then_some(i)
}

/// Where the value of `key` starts, among the direct members of the object that opens at `open`.
fn key_value(text: &str, open: usize, key: &str) -> Option<usize> {
    let t = text.as_bytes();
    let (mut i, mut depth) = (open + 1, 0usize);
    while i < t.len() {
        match t[i] {
            b'"' => {
                let start = i + 1;
                i += 1;
                while i < t.len() && t[i] != b'"' {
                    i += if t[i] == b'\\' { 2 } else { 1 };
                }
                let name = &text[start..i.min(t.len())];
                i += 1;
                if depth == 0 {
                    let after = skip_space(t, i);
                    if t.get(after) == Some(&b':') && name == key {
                        return Some(skip_space(t, after + 1));
                    }
                }
            }
            b'/' if matches!(t.get(i + 1), Some(b'/') | Some(b'*')) => i = skip_space(t, i),
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn insert_member(text: &str, open: usize, member: &str, indent: &str) -> String {
    let body: Vec<String> = member.lines().map(|l| format!("{indent}{l}")).collect();
    format!("{}\n{}{}", &text[..=open], body.join("\n"), &text[open + 1..])
}

/// settings.json with the Tracewise options added, comments and all. Err says why it was not done (and the caller shows the block to
/// paste instead): the file has `lsp.tracewise` already, or is not an object.
fn merge_settings(existing: Option<&str>, o: &Settings) -> Result<String, String> {
    let options = serde_json::to_string_pretty(&json!({ "initialization_options": o.options()? })).map_err(|e| e.to_string())?;
    let Some(text) = existing.filter(|t| !t.trim().is_empty()) else {
        return settings_block(o).map(|b| b + "\n");
    };
    let open = top_open(text).ok_or("the file is not a JSON object")?;
    let mut out = match key_value(text, open, "lsp") {
        None => insert_member(text, open, &format!("\"lsp\": {{\n  \"tracewise\": {}\n}},", options.replace('\n', "\n  ")), "  "),
        Some(v) if text.as_bytes().get(v) == Some(&b'{') => {
            if key_value(text, v, "tracewise").is_some() {
                return Err("the file already has lsp.tracewise; edit it by hand".into());
            }
            insert_member(text, v, &format!("\"tracewise\": {},", options), "    ")
        }
        Some(_) => return Err("`lsp` in the file is not an object".into()),
    };
    if o.hints {
        let open = top_open(&out).ok_or("the file is not a JSON object")?;
        if key_value(&out, open, "inlay_hints").is_none() {
            out = insert_member(&out, open, "\"inlay_hints\": { \"enabled\": true },", "  ");
        }
    }
    Ok(out)
}

/// Asks the questions; an unreadable answer takes the default.
fn ask_settings(input: &mut dyn std::io::BufRead, out: &mut dyn std::io::Write) -> Settings {
    let _ = writeln!(out, "When should the popup 'why did this change?' appear?\n  1 pause (after you stop saving)  2 every save  3 at most every N seconds  4 when I close the file  5 never by itself");
    let choice = crate::ui::ask(input, out, "choose one or more numbers, e.g. 1,4", "1");
    let names = ["pause", "every-save", "interval", "leave", "manual"];
    let mut s = Settings::default();
    for n in choice.split(|c: char| c == ',' || c.is_whitespace()).filter_map(|n| n.parse::<usize>().ok()) {
        if let Some(name) = names.get(n.wrapping_sub(1)) {
            if !s.when.iter().any(|w| w == name) {
                s.when.push((*name).to_string());
            }
        }
    }
    if s.when.is_empty() {
        s.when.push("pause".into());
    }
    let number = |input: &mut dyn std::io::BufRead, out: &mut dyn std::io::Write, q: &str, d: &str| -> Option<f64> { crate::ui::ask(input, out, q, d).parse().ok() };
    if s.when.iter().any(|w| w == "pause") {
        s.after = number(input, out, "seconds without a save before it asks (0 = right after the save)", "0");
    }
    if s.when.iter().any(|w| w == "interval") {
        s.every = number(input, out, "at most one popup per file in this many seconds", "60");
    }
    s.min_chars = number(input, out, "do not ask when fewer characters changed (0 = ask always)", "0").map(|n| n as u64).filter(|n| *n > 0);
    s.min_lines = number(input, out, "do not ask when fewer lines changed (0 = ask always)", "0").map(|n| n as u64).filter(|n| *n > 0);
    let ignore = crate::ui::ask(input, out, "file patterns never asked about, comma separated (empty = none)", "");
    s.ignore = ignore.split(',').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect();
    s.hints = crate::ui::ask(input, out, "also show who changed each run of lines (inlay hints)? y/n", "n").to_lowercase().starts_with('y');
    s
}

/// Opens a file in Zed: `open -a Zed` on macOS, else the `zed` command. False when neither could be started.
fn open_in_zed(path: &Path) -> bool {
    let started = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-a").arg("Zed").arg(path).status()
    } else {
        std::process::Command::new("zed").arg(path).status()
    };
    started.is_ok_and(|s| s.success())
}

/// `editors zed --settings`: with no option given (and a person at the keyboard) it asks; then it prints the block, and with
/// `--write` adds it to settings.json (a backup is kept) and with `--open` opens that file in Zed. Returns the exit code.
pub(crate) fn run_settings(mut s: Settings, write: bool, open: bool, dir: Option<PathBuf>) -> i32 {
    use std::io::IsTerminal;
    let interactive = s.is_empty() && std::io::stdin().is_terminal();
    let (mut write, mut open) = (write, open);
    if interactive {
        s = ask_settings(&mut std::io::stdin().lock(), &mut std::io::stdout());
    }
    let block = match settings_block(&s) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("tracewise: {e}");
            return 2;
        }
    };
    println!("\nAdd this to Zed's settings.json (`zed: open settings file`):\n\n{block}\n");
    if interactive && !write {
        write = crate::ui::ask_here("add it to settings.json now (a backup is kept)? y/n", "n").to_lowercase().starts_with('y');
    }
    let path = config_dir(dir).join("settings.json");
    let mut code = 0;
    if write {
        let existing = std::fs::read_to_string(&path).ok();
        match merge_settings(existing.as_deref(), &s) {
            Ok(text) => match write_with_backup(&path, &text) {
                Ok(()) => println!("added it to {} (backup: settings.json.tracewise-bak). Run `editor: restart language server`.", path.display()),
                Err(e) => {
                    eprintln!("tracewise: cannot write {}: {e}", path.display());
                    code = 1;
                }
            },
            Err(why) => {
                eprintln!("tracewise: {why}; nothing was written. Paste the block above.");
                code = 1;
            }
        }
    }
    if interactive && !open && !write {
        open = crate::ui::ask_here("open settings.json in Zed so you can paste it? y/n", "n").to_lowercase().starts_with('y');
    }
    if open && !open_in_zed(&path) {
        eprintln!("tracewise: could not start Zed; open {} yourself", path.display());
        code = 1;
    }
    if !write && !open {
        println!("Then run `editor: restart language server`.");
    }
    code
}

/// Prints the task and key, or with `apply` writes them (a backup of an existing file is kept next to it). Returns the exit code.
pub(crate) fn run_zed(apply: bool, key: &str, dir: Option<PathBuf>) -> i32 {
    let dir = config_dir(dir);
    let (tasks_path, keymap_path) = (dir.join("tasks.json"), dir.join("keymap.json"));
    let snippet = format!(
        "tasks.json (add to the list):\n{}\n\nkeymap.json (add to the list):\n{}\n",
        serde_json::to_string_pretty(&task()).unwrap(),
        serde_json::to_string_pretty(&binding(key)).unwrap()
    );
    if !apply {
        println!("{snippet}\nRun `tracewise editors zed --apply` to write these into {}", dir.display());
        return 0;
    }
    let mut code = 0;
    let read = |p: &Path| std::fs::read_to_string(p).ok();
    match merge_tasks(read(&tasks_path).as_deref()) {
        Some(text) => match write_with_backup(&tasks_path, &text) {
            Ok(()) => println!("wrote the task \"{LABEL}\" to {}", tasks_path.display()),
            Err(e) => {
                eprintln!("tracewise: cannot write {}: {e}", tasks_path.display());
                code = 1;
            }
        },
        None => {
            eprintln!("tracewise: {} has comments or is not a plain JSON list, so it was left alone; add the task by hand:\n{}", tasks_path.display(), serde_json::to_string_pretty(&task()).unwrap());
            code = 1;
        }
    }
    match merge_keymap(read(&keymap_path).as_deref(), key) {
        Some(Ok(text)) => match write_with_backup(&keymap_path, &text) {
            Ok(()) => println!("bound {key} to it in {}", keymap_path.display()),
            Err(e) => {
                eprintln!("tracewise: cannot write {}: {e}", keymap_path.display());
                code = 1;
            }
        },
        Some(Err(why)) => {
            eprintln!("tracewise: {why}; pick another with --key");
            code = 1;
        }
        None => {
            eprintln!("tracewise: {} has comments or is not a plain JSON list, so it was left alone; add the binding by hand:\n{}", keymap_path.display(), serde_json::to_string_pretty(&binding(key)).unwrap());
            code = 1;
        }
    }
    if code == 0 {
        println!("Nothing to restart: run `task: spawn` or press {key} after saving.");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_are_added_once_and_keep_the_others() {
        let first = merge_tasks(Some(r#"[{"label":"mine","command":"ls"}]"#)).unwrap();
        let again = merge_tasks(Some(&first)).unwrap();
        assert_eq!(first, again);
        let list: Vec<Value> = serde_json::from_str(&first).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["label"], "mine");
        assert!(merge_tasks(Some("// a comment\n[]")).is_none());
        assert_eq!(serde_json::from_str::<Vec<Value>>(&merge_tasks(None).unwrap()).unwrap().len(), 1);
    }

    #[test]
    fn the_key_is_refused_when_taken_and_replaced_when_ours() {
        let taken = r#"[{"bindings":{"cmd-alt-w":"editor::Copy"}}]"#;
        assert!(matches!(merge_keymap(Some(taken), "cmd-alt-w"), Some(Err(_))));
        let ok = merge_keymap(Some(taken), "cmd-alt-y").unwrap().unwrap();
        let moved = merge_keymap(Some(&ok), "cmd-alt-u").unwrap().unwrap();
        let list: Vec<Value> = serde_json::from_str(&moved).unwrap();
        assert_eq!(list.len(), 2, "our binding moved, not duplicated");
        assert!(moved.contains("cmd-alt-u") && !moved.contains("cmd-alt-y"));
        assert!(merge_keymap(Some("/* x */ []"), "k").is_none());
    }

    fn settings(when: &[&str]) -> Settings {
        Settings { when: when.iter().map(|w| w.to_string()).collect(), ..Settings::default() }
    }

    #[test]
    fn the_settings_block_has_the_chosen_options_and_refuses_unknown_ones() {
        let one = settings_block(&Settings { after: Some(5.0), ..settings(&["pause"]) }).unwrap();
        assert!(one.contains("\"askWhen\": \"pause\"") && one.contains("\"askAfterSeconds\": 5.0") && !one.contains("inlay"), "{one}");
        let two = settings_block(&Settings { every: Some(30.0), hints: true, ..settings(&["pause", "leave"]) }).unwrap();
        assert!(two.contains("\"leave\"") && two.contains("askEverySeconds") && two.contains("\"inlay_hints\""), "{two}");
        assert!(settings_block(&settings(&["sometimes"])).is_err());
        assert!(settings_block(&Settings { after: Some(-1.0), ..settings(&["pause"]) }).is_err());
        let sized = Settings { min_chars: Some(20), min_lines: Some(2), ignore: vec!["*.md".into()], ..settings(&["pause"]) };
        let text = settings_block(&sized).unwrap();
        assert!(text.contains("\"askMinChars\": 20") && text.contains("askMinLines") && text.contains("*.md"), "{text}");
        assert!(settings_block(&Settings::default()).unwrap().contains("\"pause\""), "no choice means pause");
    }

    #[test]
    fn settings_are_merged_into_a_file_with_comments_and_kept_intact() {
        let file = "// my settings\n{\n  \"theme\": \"One\", // keep\n  \"lsp\": {\n    \"rust-analyzer\": { \"x\": 1 }\n  },\n  /* tail */\n}\n";
        let out = merge_settings(Some(file), &Settings { hints: true, ..settings(&["pause"]) }).unwrap();
        assert!(out.contains("// my settings") && out.contains("// keep") && out.contains("/* tail */") && out.contains("rust-analyzer"), "{out}");
        assert!(out.contains("\"tracewise\"") && out.contains("\"inlay_hints\""), "{out}");
        assert_eq!(out.matches("\"lsp\"").count(), 1, "added inside the existing lsp: {out}");
        // the keys of a nested object are not mistaken for top-level ones
        let nested = "{ \"other\": { \"lsp\": 1 } }";
        let out = merge_settings(Some(nested), &settings(&["pause"])).unwrap();
        assert_eq!(out.matches("\"lsp\"").count(), 2, "{out}");
        // asked twice, it refuses instead of duplicating
        assert!(merge_settings(Some(&merge_settings(Some(file), &settings(&["pause"])).unwrap()), &settings(&["leave"])).is_err());
        // an empty or missing file gets a whole block
        assert!(merge_settings(None, &settings(&["pause"])).unwrap().contains("initialization_options"));
        assert!(merge_settings(Some("[1]"), &settings(&["pause"])).is_err());
    }

    #[test]
    fn the_questions_turn_answers_into_settings_and_empty_answers_into_defaults() {
        let mut input = std::io::Cursor::new(b"1,4\n7\n20\n3\n*.md, dist/**\ny\n".to_vec());
        let mut out = Vec::new();
        let s = ask_settings(&mut input, &mut out);
        assert_eq!(s.when, ["pause", "leave"]);
        assert_eq!((s.after, s.min_chars, s.min_lines), (Some(7.0), Some(20), Some(3)));
        assert_eq!(s.ignore, ["*.md", "dist/**"]);
        assert!(s.hints);
        let none = ask_settings(&mut std::io::Cursor::new(Vec::new()), &mut Vec::new());
        assert_eq!(none.when, ["pause"]);
        assert!(none.min_chars.is_none() && none.ignore.is_empty() && !none.hints);
    }

    #[test]
    fn apply_writes_both_files_with_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tasks.json"), "[]").unwrap();
        assert_eq!(run_zed(true, "cmd-alt-w", Some(dir.path().to_path_buf())), 0);
        assert!(dir.path().join("tasks.json.tracewise-bak").exists());
        assert!(std::fs::read_to_string(dir.path().join("keymap.json")).unwrap().contains("task::Spawn"));
    }
}
