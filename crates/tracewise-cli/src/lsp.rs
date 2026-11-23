//! `tracewise lsp`: a language server for editors that have no Tracewise extension (Zed, Neovim, Helix, Emacs, Sublime). It speaks
//! the Language Server Protocol on standard input and output and does three things:
//!
//! * **hover** on a line says who changed it, when and why;
//! * **inlay hints** (only when the editor passes `{"inlayBlame": true}` as initialization options) put that at the end of each run
//!   of lines;
//! * **saving** a file tells Tracewise (`tracewise record --source lsp`), so the person's own edits are recorded without a watcher.
//!
//! It holds no history: every answer comes from this same program, run as `tracewise -C <folder> blame <file> --json`, so what it
//! says is what `docs/json.md` documents. It is started and stopped by the editor, so it is a background process only while the
//! editor is open.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Deserialize, Clone, Debug)]
struct Actor {
    kind: String,
    name: Option<String>,
}

/// One line of `tracewise blame --json`.
#[derive(Deserialize, Clone, Debug)]
struct Row {
    line: usize,
    text: String,
    change: String,
    actor: Actor,
    time: String,
    why: Option<String>,
    /// What recorded the change (`zed`, `vscode`, ...), when the blame says.
    #[serde(default)]
    source: Option<String>,
}

/// How long a request to this program may take. The first `blame` of a file also catches up on the tree, which can be slow on a
/// big repository; a stuck child must not hold the editor's language-server thread for good.
/// A blame answer is reused this long.
const BLAME_FRESH: Duration = Duration::from_secs(10);
const BLAME_TIMEOUT: Duration = Duration::from_secs(5);
/// The buttons of the question after a save; the last one leaves the change without a reason.
const REASONS: [&str; 4] = ["Bug fix", "Refactor", "New feature", TYPE];
/// Stops the questions until the editor is closed. (Closing the popup with its x already skips one question.)
/// The first line of the file a reason is typed in; it is not part of the reason.
const WHY_HEADER: &str = "<!-- Write why the file changed below, then save (cmd-s). Tracewise stores it as the reason. -->\n";
const MUTE: &str = "Don't ask again";
/// The button that repeats the previous answer, followed by that answer.
const SAME: &str = "Same as last: ";
/// Leaves this change without a reason (it can be explained later with `tracewise note`).
const SKIP: &str = "Skip";
const DEFAULT_PAUSE: Duration = Duration::ZERO;
const DEFAULT_INTERVAL: Duration = Duration::from_secs(60);

/// When the question is asked (the `askWhen` option, one name or a list: whichever happens first). Nothing set = `pause`.
#[derive(Clone, Copy, Default)]
struct When {
    every_save: bool,
    /// Once the person has not saved for this long.
    pause: Option<Duration>,
    /// At most once per file in this time.
    interval: Option<Duration>,
    /// When the file is closed.
    leave: bool,
}

impl When {
    /// `manual` and unknown names switch nothing on; a list with nothing valid in it falls back to `pause`.
    fn parse(options: &Value) -> Self {
        let names: Vec<&str> = match &options["askWhen"] {
            Value::String(n) => vec![n.as_str()],
            Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
            _ => vec!["pause"],
        };
        let secs = |key: &str, default: Duration| options[key].as_f64().filter(|s| *s >= 0.0).map_or(default, Duration::from_secs_f64);
        let mut when = When::default();
        for name in &names {
            match *name {
                "every-save" => when.every_save = true,
                "pause" => when.pause = Some(secs("askAfterSeconds", DEFAULT_PAUSE)),
                "interval" => when.interval = Some(secs("askEverySeconds", DEFAULT_INTERVAL)),
                "leave" => when.leave = true,
                _ => {}
            }
        }
        if names.iter().all(|n| !["every-save", "pause", "interval", "leave", "manual"].contains(n)) {
            when.pause = Some(secs("askAfterSeconds", DEFAULT_PAUSE));
        }
        when
    }
}

/// Which saves are worth a question (`askMin*`, `askIgnoreWhitespace`, `askIgnore`). The defaults ask about everything except
/// whitespace-only changes and files nobody wants to explain.
struct Filters {
    min_chars: usize,
    min_lines: usize,
    ignore_whitespace: bool,
    ignore: Vec<String>,
}

const BUILT_IN_IGNORE: [&str; 9] = ["*.lock", "package-lock.json", "**/target/**", "**/node_modules/**", "**/dist/**", "**/.git/**", "*.min.js", "*.map", "**/__pycache__/**"];

impl Filters {
    fn parse(options: &Value) -> Self {
        let count = |key: &str| options[key].as_u64().map_or(0, |n| n as usize);
        let mut ignore: Vec<String> = BUILT_IN_IGNORE.iter().map(|s| s.to_string()).collect();
        if let Some(list) = options["askIgnore"].as_array() {
            ignore.extend(list.iter().filter_map(Value::as_str).map(String::from));
        }
        Self { min_chars: count("askMinChars"), min_lines: count("askMinLines"), ignore_whitespace: options["askIgnoreWhitespace"].as_bool().unwrap_or(true), ignore }
    }

    /// The automatic reason for a change that is not worth a question, or None when it is.
    fn skip_reason(&self, path: &str, op: &str, stats: &Value) -> Option<String> {
        if self.ignore.iter().any(|g| tracewise_core::glob_match_anywhere(g, path)) {
            return Some("not asked (ignored file)".to_string());
        }
        if op != "modify" || stats.is_null() {
            return None;
        }
        let n = |k: &str| stats[k].as_u64().unwrap_or(0) as usize;
        if self.ignore_whitespace && stats["whitespace_only"].as_bool() == Some(true) {
            return Some("whitespace".to_string());
        }
        let (chars, lines) = (n("added_chars") + n("removed_chars"), n("added_lines") + n("removed_lines"));
        if self.min_chars > 0 && chars < self.min_chars {
            return Some(format!("minor edit ({})", crate::ui::plural(chars, "character")));
        }
        if self.min_lines > 0 && lines < self.min_lines {
            return Some(format!("minor edit ({})", crate::ui::plural(lines, "line")));
        }
        None
    }
}

/// Changes saved and not asked about yet, for one file.
struct Pending {
    changes: Vec<String>,
    last_save: Instant,
}
/// Zed cannot show a text box: this button opens a terminal window that asks, and writes what is typed.
const TYPE: &str = "Type my own…";
const RECORD_TIMEOUT: Duration = Duration::from_secs(10);
/// After a blame that failed for a reason that may pass (a timeout, a locked database), wait this long before asking again.
const RETRY_AFTER: Duration = Duration::from_secs(30);
/// A frame bigger than this is refused: the length is whatever the other side says.
const MAX_FRAME: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct Doc {
    /// The buffer differs from the file on disk, which blame describes: its lines would be off.
    dirty: bool,
    blame: Option<Vec<Row>>,
    /// When `blame` was asked: after `BLAME_FRESH` it is asked again, so a reason added meanwhile (by another server process, a
    /// terminal, or the reason file) shows up on the next hover.
    blame_at: Option<Instant>,
    /// A blame that failed for a reason that may pass: do not ask again before this.
    retry_at: Option<Instant>,
}

/// What a child printed and how it ended; `None` code means it was killed after the deadline.
struct Answer {
    code: Option<i32>,
    stdout: Vec<u8>,
}

/// `ETXTBSY` ("text file busy") is what a program that was written a moment ago can answer while another thread is between `fork`
/// and `exec` holding its file open; it passes at once, so try a few times.
fn spawn_retrying(program: &Path, args: &[std::ffi::OsString]) -> Option<std::process::Child> {
    for attempt in 0..5 {
        match Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
            Ok(child) => return Some(child),
            Err(e) if e.raw_os_error() == Some(26) && attempt < 4 => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
    None
}

/// Runs `program args…`, waiting at most `timeout`, then kills it. Standard output is read on a helper thread so a long answer
/// cannot fill the pipe and make the wait itself hang.
fn run_with_timeout(program: &Path, args: &[std::ffi::OsString], timeout: Duration) -> Option<Answer> {
    let mut child = spawn_retrying(program, args)?;
    let mut out = child.stdout.take()?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = io::Read::read_to_end(&mut out, &mut buf);
        let _ = sender.send(buf);
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            // it finished: its answer is what was printed. A grandchild that kept the pipe open is not waited for beyond a moment.
            Ok(Some(status)) => {
                let stdout = receiver.recv_timeout(Duration::from_millis(500)).unwrap_or_default();
                return Some(Answer { code: status.code(), stdout });
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            // too slow: kill it and do not wait for anything it left behind (the reader ends when the last holder of the pipe does)
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Some(Answer { code: None, stdout: Vec::new() });
            }
        }
    }
}

pub(crate) struct Server {
    /// The program that answers: this one (tests pass a stand-in).
    program: PathBuf,
    docs: HashMap<String, Doc>,
    inlay_blame: bool,
    /// The `--source` for the saves this editor reports: `zed`, `vscode`, `jetbrains`, else `lsp`.
    source: &'static str,
    /// Ask why after each save (initialization option `askWhy`): the editor shows the question with the reasons as buttons.
    ask_why: bool,
    /// Messages for the editor that are not a reply (the question above); `serve` sends them after each message.
    outbox: Vec<Value>,
    /// Questions sent and not answered yet: request id -> (file, changes it is about).
    asked: HashMap<String, (PathBuf, Vec<String>)>,
    next_question: u64,
    /// The last reason chosen, offered again as the first button.
    last_reason: Option<String>,
    muted: bool,
    /// The editor can open a file for the user (`window/showDocument`): "Type my own…" then opens a file to write the reason in.
    show_document: bool,
    /// The file the user is writing a reason in, the file it is about, and the changes it explains.
    why_file: Option<(PathBuf, PathBuf, Vec<String>)>,
    when: When,
    filters: Filters,
    /// When each file was last asked about, and what was saved since.
    last_asked: HashMap<PathBuf, Instant>,
    pending: HashMap<PathBuf, Pending>,
    /// Added to the clock; tests move time by hand.
    skew: Duration,
    shut_down: bool,
    blame_timeout: Duration,
    record_timeout: Duration,
}

impl Server {
    pub(crate) fn new(program: PathBuf) -> Self {
        Self {
            program,
            docs: HashMap::new(),
            inlay_blame: false,
            source: "lsp",
            ask_why: false,
            outbox: Vec::new(),
            asked: HashMap::new(),
            next_question: 0,
            last_reason: None,
            muted: false,
            show_document: false,
            why_file: None,
            when: When::parse(&Value::Null),
            filters: Filters::parse(&Value::Null),
            last_asked: HashMap::new(),
            pending: HashMap::new(),
            skew: Duration::ZERO,
            shut_down: false,
            blame_timeout: BLAME_TIMEOUT,
            record_timeout: RECORD_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_timeouts(mut self, blame: Duration, record: Duration) -> Self {
        self.blame_timeout = blame;
        self.record_timeout = record;
        self
    }

    /// The reply to one message, when it is a request; notifications have none.
    pub(crate) fn handle(&mut self, message: &Value) -> Option<Value> {
        let Some(method) = message["method"].as_str() else {
            // no method: the editor's answer to a question this server asked
            if let Some(id) = message["id"].as_str() {
                self.answered(id, &message["result"]);
            }
            return None;
        };
        let id = message.get("id").cloned();
        let params = &message["params"];
        if self.shut_down && method != "shutdown" {
            // after `shutdown` only `exit` is allowed; a request still gets an answer, as the protocol asks
            return id
                .map(|id| json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32600, "message": "the server is shutting down" } }));
        }
        let result = match method {
            "initialize" => Some(self.initialize(params)),
            "shutdown" => {
                self.shut_down = true;
                Some(Value::Null)
            }
            "textDocument/didOpen" => {
                let uri = uri_of(&params["textDocument"]);
                let dirty = differs_from_disk(&uri, params["textDocument"]["text"].as_str(), false);
                self.docs.insert(uri, Doc { dirty, ..Doc::default() });
                None
            }
            "textDocument/didChange" => {
                let uri = uri_of(&params["textDocument"]);
                // full sync: the last change carries the whole text. A buffer that equals the file (a reload after an agent rewrote
                // it, an undo back to the saved text) is clean, and what was known about it is stale.
                let text = params["contentChanges"].as_array().and_then(|c| c.last()).and_then(|c| c["text"].as_str());
                let dirty = differs_from_disk(&uri, text, true);
                let doc = self.docs.entry(uri).or_default();
                doc.dirty = dirty;
                doc.blame = None;
                doc.retry_at = None;
                None
            }
            "textDocument/didSave" => {
                let uri = uri_of(&params["textDocument"]);
                if !self.reason_saved(&uri) {
                    self.saved(&uri);
                }
                None
            }
            "textDocument/didClose" => {
                let uri = uri_of(&params["textDocument"]);
                if let Some(path) = path_of(&uri).filter(|_| self.when.leave) {
                    self.flush(&path);
                }
                self.docs.remove(&uri);
                None
            }
            "textDocument/hover" => Some(self.hover(params)),
            "textDocument/inlayHint" => Some(self.inlay_hints(params)),
            _ => {
                return id.map(|id| json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("{method} is not supported") } }));
            }
        };
        id.zip(result).map(|(id, result)| json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    pub(crate) fn wants_exit(&self) -> bool {
        self.shut_down
    }

    fn initialize(&mut self, params: &Value) -> Value {
        self.inlay_blame = params["initializationOptions"]["inlayBlame"].as_bool().unwrap_or(false);
        self.source = source_of(params["clientInfo"]["name"].as_str().unwrap_or(""));
        let options = &params["initializationOptions"];
        self.ask_why = options["askWhy"].as_bool().unwrap_or(!options["askWhen"].is_null());
        self.when = When::parse(options);
        self.filters = Filters::parse(options);
        self.show_document = params["capabilities"]["window"]["showDocument"]["support"].as_bool().unwrap_or(false);
        let mut capabilities = json!({
            "textDocumentSync": { "openClose": true, "change": 1, "save": { "includeText": false } },
            "hoverProvider": true,
        });
        if self.inlay_blame {
            capabilities["inlayHintProvider"] = json!(true);
        }
        json!({ "capabilities": capabilities, "serverInfo": { "name": "tracewise", "version": env!("CARGO_PKG_VERSION") } })
    }

    fn saved(&mut self, uri: &str) {
        let doc = self.docs.entry(uri.to_string()).or_default();
        doc.dirty = false;
        doc.blame = None;
        doc.retry_at = None;
        if let Some(path) = path_of(uri) {
            // the person saved: the one thing a watcher would have told Tracewise. Failures are not the editor's business.
            let answer = self.cli(&path, &["record", "--source", self.source, &path.to_string_lossy()], self.record_timeout);
            if self.ask_why {
                let json = answer.and_then(|(_, json)| json).unwrap_or(Value::Null);
                let mut changes: Vec<String> = Vec::new();
                for row in json["stats"].as_array().into_iter().flatten() {
                    let Some(id) = row["id"].as_str() else { continue };
                    match self.filters.skip_reason(row["path"].as_str().unwrap_or(""), row["op"].as_str().unwrap_or(""), &row["stats"]) {
                        Some(reason) => self.auto_note(&path, id, &reason),
                        None => changes.push(id.to_string()),
                    }
                }
                // an older `tracewise` that reports no sizes: every change is asked about
                if json["stats"].is_null() {
                    changes = json["changes"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect()).unwrap_or_default();
                }
                self.maybe_ask(path, changes);
            }
        }
    }

    /// A message for the person (`kind`: 1 error, 2 warning, 3 info).
    fn message(&mut self, kind: u8, text: &str) {
        self.outbox.push(json!({ "jsonrpc": "2.0", "method": "window/showMessage", "params": { "type": kind, "message": text } }));
    }

    /// An automatic reason for a change nobody was asked about: it fills a gap and never replaces a reason a person gave.
    fn auto_note(&self, file: &Path, id: &str, reason: &str) {
        let _ = self.cli(file, &["note", id, reason, "--auto"], self.record_timeout);
    }

    /// Writes the file the reason is typed in (in the project's data folder, so nothing lands in the project) and asks the editor
    /// to open it. False when that was not possible (the caller falls back to a terminal).
    fn open_why_file(&mut self, path: &Path, changes: &[String]) -> bool {
        let Some(dir) = self.cli(path, &["status"], self.blame_timeout).and_then(|(_, j)| j).and_then(|j| j["data_dir"].as_str().map(PathBuf::from)) else {
            return false;
        };
        let file = dir.join("why.md");
        if std::fs::write(&file, WHY_HEADER).is_err() {
            return false;
        }
        self.next_question += 1;
        let uri = format!("file://{}", file.to_string_lossy().replace(' ', "%20"));
        self.outbox.push(json!({ "jsonrpc": "2.0", "id": format!("tracewise-doc-{}", self.next_question), "method": "window/showDocument", "params": { "uri": uri, "takeFocus": true } }));
        // another `tracewise lsp` (the editor starts one per folder it opens, and the data folder is its own) may get the save of this
        // file: what it is about is written next to it, not only kept here
        let pending = json!({ "file": path, "changes": changes });
        if std::fs::write(file.with_file_name("why.pending.json"), pending.to_string()).is_err() {
            return false;
        }
        self.why_file = Some((file, path.to_path_buf(), changes.to_vec()));
        true
    }

    /// A save of the reason file: what was typed below the first line becomes the reason, and the file is emptied again.
    /// True when the saved file was that file.
    fn reason_saved(&mut self, uri: &str) -> bool {
        let Some(saved) = path_of(uri) else { return false };
        if saved.file_name().is_none_or(|n| n != "why.md") {
            return false;
        }
        let sidecar = saved.with_file_name("why.pending.json");
        let Some(pending) = std::fs::read_to_string(&sidecar).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
            return false;
        };
        let Some(project_file) = pending["file"].as_str().map(PathBuf::from) else { return false };
        let changes: Vec<String> = pending["changes"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect()).unwrap_or_default();
        let text = std::fs::read_to_string(&saved).unwrap_or_default();
        // the first line is the comment this server wrote, even if the person typed into it
        let body = match text.trim_start().strip_prefix("<!--") {
            Some(rest) => rest.split_once("-->").map_or("", |(_, after)| after),
            None => text.as_str(),
        };
        let reason = body.trim().to_string();
        if reason.is_empty() {
            return true;
        }
        let mut rejected = false;
        for change in &changes {
            let stored = self.cli(&project_file, &["note", change, &reason], self.record_timeout);
            rejected |= matches!(stored, Some((Some(1), _)));
        }
        if rejected {
            // what was typed stays in the file to be fixed and saved again
            self.message(2, "Tracewise did not store the reason: write at least 12 characters that say what the change does and why.");
            return true;
        }
        self.last_reason = Some(reason.chars().take(40).collect());
        let _ = std::fs::write(&saved, WHY_HEADER);
        let _ = std::fs::remove_file(&sidecar);
        self.why_file = None;
        true
    }

    fn now(&self) -> Instant {
        Instant::now() + self.skew
    }

    /// A save changed these; asks now, or leaves it for `tick` / closing the file, as `askWhen` says.
    fn maybe_ask(&mut self, path: PathBuf, new: Vec<String>) {
        if self.muted || new.is_empty() {
            return;
        }
        let now = self.now();
        let p = self.pending.entry(path.clone()).or_insert_with(|| Pending { changes: Vec::new(), last_save: now });
        p.changes.extend(new);
        p.last_save = now;
        if self.is_due(&path, true) {
            self.flush(&path);
        }
    }

    /// Whether the question about this file's saved changes is due. `saving`: this is the moment of a save, when `every-save` counts.
    fn is_due(&self, path: &Path, saving: bool) -> bool {
        let Some(p) = self.pending.get(path) else { return false };
        let now = self.now();
        (saving && self.when.every_save)
            || self.when.pause.is_some_and(|d| now >= p.last_save + d)
            || self.when.interval.is_some_and(|d| self.last_asked.get(path).is_none_or(|t| now >= *t + d))
    }

    fn flush(&mut self, path: &Path) {
        let Some(p) = self.pending.remove(path) else { return };
        self.last_asked.insert(path.to_path_buf(), self.now());
        self.question(path.to_path_buf(), p.changes);
    }

    /// Asks about what has become due with time passing (a pause, an interval).
    pub(crate) fn tick(&mut self) {
        let due: Vec<PathBuf> = self.pending.keys().filter(|p| self.is_due(p, false)).cloned().collect();
        for path in due {
            self.flush(&path);
        }
    }

    /// How long until `tick` has something to do; None when nothing is waiting on time.
    pub(crate) fn next_wakeup(&self) -> Option<Duration> {
        let now = self.now();
        self.pending
            .iter()
            .flat_map(|(path, p)| {
                let pause = self.when.pause.map(|d| p.last_save + d);
                let interval = self.when.interval.map(|d| self.last_asked.get(path).map_or(now, |t| *t + d));
                pause.into_iter().chain(interval)
            })
            .min()
            .map(|at| at.saturating_duration_since(now).max(Duration::from_millis(20)))
    }

    /// Asks the editor why the file changed; the answer comes back as a message without a method (see `answered`).
    fn question(&mut self, path: PathBuf, changes: Vec<String>) {
        self.next_question += 1;
        let id = format!("tracewise-why-{}", self.next_question);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let same = self.last_reason.as_ref().map(|r| format!("{SAME}{r}"));
        let actions: Vec<Value> = same.iter().map(String::as_str).chain(REASONS).chain([SKIP, MUTE]).map(|t| json!({ "title": t })).collect();
        self.outbox.push(json!({
            "jsonrpc": "2.0", "id": id, "method": "window/showMessageRequest",
            "params": { "type": 3, "message": format!("Tracewise: why did {name} change?"), "actions": actions },
        }));
        self.asked.insert(id, (path, changes));
    }

    /// The chosen button becomes the reason of the changes that were asked about; Skip, a closed popup and a failure change nothing.
    fn answered(&mut self, id: &str, result: &Value) {
        let Some((path, changes)) = self.asked.remove(id) else { return };
        let Some(title) = result["title"].as_str() else {
            // the popup was closed without an answer
            for change in &changes {
                self.auto_note(&path, change, "no reason given");
            }
            return;
        };
        if title == MUTE {
            self.muted = true;
            self.pending.clear();
            return;
        }
        if title == SKIP {
            for change in &changes {
                self.auto_note(&path, change, "skipped");
            }
            return;
        }
        let title = title.strip_prefix(SAME).unwrap_or(title);
        if title != TYPE {
            self.last_reason = Some(title.to_string());
        }
        if title == TYPE {
            if self.show_document && self.open_why_file(&path, &changes) {
                return;
            }
            if let Some(script) = terminal_script(&self.program, &path, &changes) {
                let _ = std::process::Command::new("osascript").args(["-e", &script]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
            }
            return;
        }
        let mut failed = false;
        for change in changes {
            let stored = self.cli(&path, &["note", &change, title, "--label"], self.record_timeout);
            failed |= !matches!(stored, Some((Some(0), _)));
        }
        if failed {
            self.message(1, "Tracewise could not store the reason (see ~/.tracewise/lsp-debug.log).");
        }
    }

    /// `tracewise -C <the file's folder> <args…> --json`: the exit code and the parsed answer.
    fn cli(&self, file: &Path, args: &[&str], timeout: Duration) -> Option<(Option<i32>, Option<Value>)> {
        let folder = file.parent()?;
        let mut full: Vec<std::ffi::OsString> = vec!["-C".into(), folder.into()];
        full.extend(args.iter().map(std::ffi::OsString::from));
        full.push("--json".into());
        let answer = run_with_timeout(&self.program, &full, timeout);
        log_line(&format!("run {:?} -> {}", args, answer.as_ref().map_or("no answer (timeout or not started)".to_string(), |a| format!("exit {:?}", a.code))));
        let answer = answer?;
        Some((answer.code, serde_json::from_slice(&answer.stdout).ok()))
    }

    /// The blame of a saved file. None for a buffer that differs from disk, a file nothing is known about, and for a failure that
    /// may pass (asked again after a while). Exit code 2 and "no history yet" are answers, and are remembered.
    fn blame(&mut self, uri: &str) -> Option<Vec<Row>> {
        let path = path_of(uri)?;
        let doc = self.docs.entry(uri.to_string()).or_default();
        if doc.dirty {
            return None;
        }
        if doc.blame.is_none() || doc.blame_at.is_some_and(|t| t.elapsed() > BLAME_FRESH) {
            if doc.retry_at.is_some_and(|t| Instant::now() < t) {
                return None;
            }
            let answer = self.cli(&path, &["blame", &path.to_string_lossy()], self.blame_timeout);
            let doc = self.docs.get_mut(uri)?;
            match answer {
                Some((Some(0), Some(v))) => {
                    doc.blame = Some(serde_json::from_value::<Vec<Row>>(v).unwrap_or_default());
                    doc.blame_at = Some(Instant::now());
                }
                // not a project here (2), or a project with no history yet (1): nothing to say about this file, for now
                Some((Some(1 | 2), _)) => {
                    doc.blame = Some(Vec::new());
                    doc.blame_at = Some(Instant::now());
                }
                _ => {
                    doc.retry_at = Some(Instant::now() + RETRY_AFTER);
                    return None;
                }
            }
        }
        self.docs.get(uri)?.blame.clone()
    }

    fn hover(&mut self, params: &Value) -> Value {
        let Some(line) = params["position"]["line"].as_u64().map(|l| l as usize + 1) else { return Value::Null };
        let rows = self.blame(&uri_of(&params["textDocument"])).unwrap_or_default();
        match rows.iter().find(|r| r.line == line).filter(|r| r.change != "baseline") {
            Some(row) => json!({ "contents": { "kind": "markdown", "value": hover_text(row, chrono::Utc::now()) } }),
            None => Value::Null,
        }
    }

    fn inlay_hints(&mut self, params: &Value) -> Value {
        if !self.inlay_blame {
            return json!([]);
        }
        let rows = self.blame(&uri_of(&params["textDocument"])).unwrap_or_default();
        let hints: Vec<Value> = runs(&rows)
            .into_iter()
            .filter(|r| r.change != "baseline")
            .map(|row| {
                json!({
                    "position": { "line": row.line - 1, "character": utf16_len(&row.text) },
                    "label": label(&row, chrono::Utc::now()),
                    "paddingLeft": true,
                    "tooltip": { "kind": "markdown", "value": hover_text(&row, chrono::Utc::now()) },
                })
            })
            .collect();
        json!(hints)
    }
}

/// The length the protocol uses for a column: UTF-16 code units, not characters or bytes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// AppleScript that opens a Terminal window asking for a reason and noting `changes` with it (macOS only; None elsewhere).
fn terminal_script(program: &Path, file: &Path, changes: &[String]) -> Option<String> {
    if !cfg!(target_os = "macos") || changes.iter().any(|c| !c.chars().all(|ch| ch.is_ascii_alphanumeric())) {
        return None;
    }
    let mut shell = format!("printf 'why? '; read r; [ -n \"$r\" ] && for c in {}; do {} -C {} note \"$c\" \"$r\"; done; exit", changes.join(" "), shell_quote(&program.to_string_lossy()), shell_quote(&file.parent()?.to_string_lossy()));
    shell = shell.replace('\\', "\\\\").replace('"', "\\\"");
    Some(format!("tell application \"Terminal\" to activate\ntell application \"Terminal\" to do script \"{shell}\""))
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The first line of each run of lines one change wrote.
fn runs(rows: &[Row]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    let mut last: Option<&Row> = None;
    for row in rows {
        if !last.is_some_and(|l| l.change == row.change && l.line + 1 == row.line) {
            out.push(row.clone());
        }
        last = Some(row);
    }
    out
}

/// The `--source` for a language-server client, from the name it gives in `initialize`.
fn source_of(client: &str) -> &'static str {
    let c = client.to_ascii_lowercase();
    if c.contains("zed") {
        "zed"
    } else if c.contains("visual studio code") || c == "code" || c.contains("vscode") {
        "vscode"
    } else if c.contains("jetbrains") || c.contains("intellij") {
        "jetbrains"
    } else {
        "lsp"
    }
}

/// Who, and for a change recorded by an editor, where from: `as via Zed`.
fn who_via(row: &Row) -> String {
    match row.source.as_deref().and_then(tracewise_core::editor_name) {
        Some(editor) if row.actor.kind == "human" => format!("{} via {editor}", who(&row.actor)),
        _ => who(&row.actor),
    }
}

fn who(actor: &Actor) -> String {
    match actor.kind.as_str() {
        "human" => actor.name.clone().unwrap_or_else(|| "someone".into()),
        "agent" => actor.name.clone().unwrap_or_else(|| "an agent".into()),
        "baseline" => "before Tracewise".into(),
        "ambiguous" => "unclear who".into(),
        _ => "unknown".into(),
    }
}

fn ago(time: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Ok(then) = chrono::DateTime::parse_from_rfc3339(time) else { return String::new() };
    let secs = (now - then.with_timezone(&chrono::Utc)).num_seconds().max(0);
    match secs {
        0..=44 => "just now".into(),
        45..=3_599 => format!("{}m ago", (secs + 30) / 60),
        3_600..=86_399 => format!("{}h ago", (secs + 1_800) / 3_600),
        _ => format!("{}d ago", (secs + 43_200) / 86_400),
    }
}

/// `claude · 2h ago · handle empty input`.
fn label(row: &Row, now: chrono::DateTime<chrono::Utc>) -> String {
    let mut parts = vec![who_via(row)];
    let when = ago(&row.time, now);
    if !when.is_empty() {
        parts.push(when);
    }
    if let Some(why) = row.why.as_deref().filter(|w| !w.is_empty()) {
        parts.push(why.to_string());
    }
    parts.join(" · ")
}

fn hover_text(row: &Row, now: chrono::DateTime<chrono::Utc>) -> String {
    let when = ago(&row.time, now);
    let head = if when.is_empty() { format!("**{}**", who_via(row)) } else { format!("**{}** · {when}", who_via(row)) };
    let why = row.why.as_deref().filter(|w| !w.is_empty()).unwrap_or("_no reason given_");
    format!("{head}\n\n{why}\n\nchange `{}`", row.change)
}

/// True when the editor's text is not what the file on disk holds. A client that sends no text cannot be compared: `unknown` is
/// the answer then (a change is assumed to differ, an opened file to match). A file that cannot be read has nothing to differ from.
fn differs_from_disk(uri: &str, text: Option<&str>, unknown: bool) -> bool {
    let Some(path) = path_of(uri) else { return false };
    match (text, std::fs::read(&path)) {
        (Some(text), Ok(disk)) => text.as_bytes() != disk.as_slice(),
        (None, _) => unknown,
        (Some(_), Err(_)) => false,
    }
}

fn uri_of(text_document: &Value) -> String {
    text_document["uri"].as_str().unwrap_or("").to_string()
}

/// `file:///a/b%20c.rs` → `/a/b c.rs`. Other schemes (an untitled buffer, a remote file) have no path.
fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let path = PathBuf::from(String::from_utf8(out).ok()?);
    path.is_absolute().then_some(path)
}

/// One framed message: `Content-Length: n`, a blank line, n bytes of JSON. `Ok(None)` at the end of the input; `Ok(Some(Err))`
/// for a body that is not JSON (the stream is still in step, so the caller can answer and go on).
fn read_frame(input: &mut impl BufRead) -> io::Result<Option<Result<Value, ()>>> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length:") {
            length = n.trim().parse().ok();
        }
    }
    let Some(length) = length else { return Err(io::Error::new(io::ErrorKind::InvalidData, "a message without Content-Length")) };
    if length > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "a message that big is refused"));
    }
    let mut body = vec![0u8; length];
    input.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body).map_err(|_| ())))
}

/// For tests: the message, or an error for anything that is not one.
#[cfg(test)]
pub(crate) fn read_message(input: &mut impl BufRead) -> io::Result<Option<Value>> {
    match read_frame(input)? {
        None => Ok(None),
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(())) => Err(io::Error::new(io::ErrorKind::InvalidData, "not JSON")),
    }
}

pub(crate) fn write_message(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

/// With `TRACEWISE_LSP_LOG=<file>` set, one line per message in or out (method, id, uri; never file contents), to see what an editor does.
fn log_line(text: &str) {
    // or an existing `~/.tracewise/lsp-debug.log` (create it to turn logging on, delete it to turn it off): an editor that was not
    // started from a shell cannot be given an environment variable
    let Some(file) = std::env::var_os("TRACEWISE_LSP_LOG").map(PathBuf::from).or_else(|| {
        let f = PathBuf::from(std::env::var_os("HOME")?).join(".tracewise").join("lsp-debug.log");
        f.is_file().then_some(f)
    }) else {
        return;
    };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
        let _ = writeln!(f, "{} pid={} {text}", tracewise_core::clock::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true), std::process::id());
    }
}

fn debug_log(direction: &str, message: &Value) {
    log_line(&format!(
        "{direction} {} id={} uri={} title={}",
        message["method"].as_str().unwrap_or("(reply)"),
        message["id"],
        message["params"]["textDocument"]["uri"].as_str().or(message["params"]["uri"].as_str()).unwrap_or("-"),
        message["result"]["title"].as_str().unwrap_or("-"),
    ));
}

enum Next {
    Message(Value),
    NotJson,
    End,
    /// Nothing arrived within the wait: time to look at what has become due.
    Tick,
}

/// The loop shared by the real server and the tests: `next` gives the next event, waiting at most the duration it is given.
fn pump(server: &mut Server, mut next: impl FnMut(Option<Duration>) -> Next, output: &mut impl Write) -> i32 {
    let exit_code = |server: &Server| if server.wants_exit() { 0 } else { 1 };
    loop {
        let message = match next(server.next_wakeup()) {
            Next::Message(m) => m,
            Next::NotJson => {
                let error = json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "the message is not JSON" } });
                if write_message(output, &error).is_err() {
                    return 1;
                }
                continue;
            }
            Next::End => return exit_code(server),
            Next::Tick => {
                server.tick();
                for out in server.outbox.drain(..) {
                    debug_log("->", &out);
                    if write_message(output, &out).is_err() {
                        return 1;
                    }
                }
                continue;
            }
        };
        if message["method"] == "exit" {
            return exit_code(server);
        }
        debug_log("<-", &message);
        let reply = server.handle(&message);
        for out in reply.into_iter().chain(server.outbox.drain(..)) {
            debug_log("->", &out);
            if write_message(output, &out).is_err() {
                return 1;
            }
        }
    }
}

/// Serves until the input ends or the editor sends `exit`, without timers (what the tests use). The exit code the editor expects:
/// 0 after `shutdown`, else 1.
#[cfg(test)]
pub(crate) fn serve(server: &mut Server, input: &mut impl BufRead, output: &mut impl Write) -> i32 {
    pump(
        server,
        |_| match read_frame(input) {
            Ok(Some(Ok(m))) => Next::Message(m),
            Ok(Some(Err(()))) => Next::NotJson,
            Ok(None) | Err(_) => Next::End,
        },
        output,
    )
}

/// The same with timers: a thread reads the input so the loop can wake when a question has become due.
pub(crate) fn serve_live(server: &mut Server, mut input: impl BufRead + Send + 'static, output: &mut impl Write) -> i32 {
    let (tx, rx) = std::sync::mpsc::channel::<Next>();
    std::thread::spawn(move || loop {
        let event = match read_frame(&mut input) {
            Ok(Some(Ok(m))) => Next::Message(m),
            Ok(Some(Err(()))) => Next::NotJson,
            Ok(None) | Err(_) => Next::End,
        };
        let end = matches!(event, Next::End);
        if tx.send(event).is_err() || end {
            return;
        }
    });
    pump(
        server,
        |wait| match wait {
            Some(wait) => rx.recv_timeout(wait).unwrap_or_else(|e| if matches!(e, std::sync::mpsc::RecvTimeoutError::Timeout) { Next::Tick } else { Next::End }),
            None => rx.recv().unwrap_or(Next::End),
        },
        output,
    )
}

pub fn run_lsp() -> i32 {
    let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tracewise"));
    let mut output = io::stdout().lock();
    serve_live(&mut Server::new(program), io::BufReader::new(io::stdin()), &mut output)
}

#[cfg(test)]
#[path = "test_lsp.rs"]
mod tests;
