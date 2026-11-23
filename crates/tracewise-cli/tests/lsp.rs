//! `tracewise lsp` against the real program: hover says why a line changed, and a save is recorded.
mod common;
use common::Repo;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Client {
    fn start(repo: &Repo) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tracewise"))
            .arg("lsp")
            .current_dir(repo.dir())
            .env("TRACEWISE_HOME", repo.home())
            .env("HOME", repo.home())
            .env_remove("CLAUDECODE")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self { child, stdin, stdout, next: 1 }
    }

    fn send(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut length = 0;
            loop {
                let mut line = String::new();
                self.stdout.read_line(&mut line).unwrap();
                let line = line.trim_end();
                if line.is_empty() {
                    break;
                }
                if let Some(n) = line.strip_prefix("Content-Length:") {
                    length = n.trim().parse().unwrap();
                }
            }
            let mut body = vec![0u8; length];
            self.stdout.read_exact(&mut body).unwrap();
            let reply: Value = serde_json::from_slice(&body).unwrap();
            if reply["id"] == id {
                return reply["result"].clone();
            }
        }
    }

    fn finish(mut self) -> i32 {
        self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        self.child.wait().unwrap().code().unwrap_or(-1)
    }
}

fn uri(repo: &Repo, file: &str) -> String {
    format!("file://{}", repo.dir().canonicalize().unwrap().join(file).display())
}

#[test]
fn hover_on_a_line_an_agent_wrote_says_who_and_why() {
    let repo = Repo::new("");
    repo.edit_as("agent:claude:s1", "a.txt", "one\ntwo\n");
    assert!(repo.run("agent:claude:s1", &["note", "--last", "write the first two lines of the file"]).ok());
    let mut lsp = Client::start(&repo);
    lsp.request("initialize", json!({ "capabilities": {} }));
    lsp.notify("textDocument/didOpen", json!({ "textDocument": { "uri": uri(&repo, "a.txt") } }));
    let hover = lsp.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri(&repo, "a.txt") }, "position": { "line": 1, "character": 0 } }),
    );
    let text = hover["contents"]["value"].as_str().unwrap_or_else(|| panic!("no hover: {hover}"));
    assert!(text.contains("**claude**") && text.contains("write the first two lines"), "{text}");
    assert_eq!(lsp.finish(), 0);
}

#[test]
fn saving_a_file_records_it_as_the_persons_with_the_lsp_source() {
    let repo = Repo::new("");
    let mut lsp = Client::start(&repo);
    lsp.request("initialize", json!({}));
    repo.write("b.txt", "typed by hand\n");
    lsp.notify("textDocument/didSave", json!({ "textDocument": { "uri": uri(&repo, "b.txt") } }));
    // the shutdown reply comes after the save was handled
    assert_eq!(lsp.finish(), 0);
    let row = repo.rows(&[]).into_iter().find(|r| r["path"] == "b.txt").expect("b.txt was recorded");
    assert_eq!(row["source"], "lsp");
    assert_eq!(row["actor"]["kind"], "human");
}

#[test]
fn a_file_outside_any_project_is_quietly_ignored() {
    let repo = Repo::new("");
    let plain = tempfile::tempdir().unwrap();
    std::fs::write(plain.path().join("x.txt"), "x\n").unwrap();
    let mut lsp = Client::start(&repo);
    lsp.request("initialize", json!({}));
    let outside = format!("file://{}", plain.path().canonicalize().unwrap().join("x.txt").display());
    lsp.notify("textDocument/didOpen", json!({ "textDocument": { "uri": outside } }));
    let hover = lsp.request("textDocument/hover", json!({ "textDocument": { "uri": outside }, "position": { "line": 0, "character": 0 } }));
    assert_eq!(hover, Value::Null);
    lsp.notify("textDocument/didSave", json!({ "textDocument": { "uri": outside } }));
    assert_eq!(lsp.finish(), 0, "the server did not fall over");
}

#[test]
fn with_askwhen_pause_the_question_arrives_by_itself_after_the_pause() {
    let repo = Repo::new("");
    let mut lsp = Client::start(&repo);
    lsp.request("initialize", json!({ "initializationOptions": { "askWhen": "pause", "askAfterSeconds": 0.3 } }));
    repo.write("c.txt", "typed by hand\n");
    lsp.notify("textDocument/didSave", json!({ "textDocument": { "uri": uri(&repo, "c.txt") } }));
    // no further message from the editor: the server must speak on its own
    let mut length = 0;
    loop {
        let mut line = String::new();
        lsp.stdout.read_line(&mut line).unwrap();
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length:") {
            length = n.trim().parse().unwrap();
        }
    }
    let mut body = vec![0u8; length];
    lsp.stdout.read_exact(&mut body).unwrap();
    let asked: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(asked["method"], "window/showMessageRequest", "{asked}");
    assert!(asked["params"]["actions"].as_array().unwrap().iter().any(|a| a["title"] == "Skip"));
    assert_eq!(lsp.finish(), 0);
}
