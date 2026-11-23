use super::*;
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;

const BLAME: &str = r#"[
 {"line":1,"text":"fn parse(input: &str) {","change":"baseline","actor":{"kind":"baseline"},"time":"","why":null},
 {"line":2,"text":"    let tokens = lex(input);","change":"01M7ZVPH6CYV3Z7X9D6W7CVH75","actor":{"kind":"agent","name":"claude","session":"s1"},"time":"2026-11-22T10:00:00Z","why":"handle empty input"},
 {"line":3,"text":"    build(tokens) // é","change":"01M7ZVPH6CYV3Z7X9D6W7CVH75","actor":{"kind":"agent","name":"claude","session":"s1"},"time":"2026-11-22T10:00:00Z","why":"handle empty input"},
 {"line":4,"text":"}","change":"baseline","actor":{"kind":"baseline"},"time":"","why":null},
 {"line":5,"text":"// hand edit","change":"01M7ZVPH6CYV3Z7X9D6W7CVH76","actor":{"kind":"human","name":"as"},"time":"2026-11-22T11:00:00Z","why":null}
]"#;

/// A stand-in for `tracewise`: answers `blame` with the rows above and records every call.
fn fake() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("tracewise");
    let calls = dir.path().join("calls.log");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$*\" in\n  *blame*) cat <<'EOF'\n{BLAME}\nEOF\n  ;;\n  *record*) echo '{{\"changes\":[]}}' ;;\n  *) echo '{{}}' ;;\nesac\n",
            calls.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, program, calls)
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn notify(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

fn doc(uri: &str) -> Value {
    json!({ "uri": uri })
}

fn hover(server: &mut Server, uri: &str, line: u64) -> Value {
    server
        .handle(&request(2, "textDocument/hover", json!({ "textDocument": doc(uri), "position": { "line": line, "character": 0 } })))
        .unwrap()["result"]
        .clone()
}

const URI: &str = "file:///work/app/src/parser.rs";

fn open(server: &mut Server, uri: &str) {
    server.handle(&notify("textDocument/didOpen", json!({ "textDocument": doc(uri) })));
}

#[test]
fn messages_are_framed_with_content_length() {
    let mut wire = Vec::new();
    write_message(&mut wire, &json!({"a": "é"})).unwrap();
    assert!(String::from_utf8_lossy(&wire).starts_with("Content-Length: 10\r\n\r\n"), "the length counts bytes, not characters");
    let mut input = Cursor::new(wire);
    assert_eq!(read_message(&mut input).unwrap(), Some(json!({"a": "é"})));
    assert_eq!(read_message(&mut input).unwrap(), None, "the end of the input");
    assert!(read_message(&mut Cursor::new(b"X-Other: 1\r\n\r\n{}".to_vec())).is_err(), "a message without a length is an error");
}

#[test]
fn initialize_offers_hover_and_inlay_hints_only_when_asked() {
    let (_d, program, _) = fake();
    let mut plain = Server::new(program.clone());
    let caps = plain.handle(&request(1, "initialize", json!({}))).unwrap()["result"]["capabilities"].clone();
    assert_eq!(caps["hoverProvider"], true);
    assert_eq!(caps["textDocumentSync"]["save"]["includeText"], false);
    assert!(caps.get("inlayHintProvider").is_none());
    let mut blame = Server::new(program);
    let caps = blame.handle(&request(1, "initialize", json!({"initializationOptions": {"inlayBlame": true}}))).unwrap()["result"]
        ["capabilities"]
        .clone();
    assert_eq!(caps["inlayHintProvider"], true);
}

#[test]
fn hover_says_who_when_and_why_for_a_changed_line_and_nothing_for_the_rest() {
    let (_d, program, _) = fake();
    let mut server = Server::new(program);
    open(&mut server, URI);
    let on_agent_line = hover(&mut server, URI, 1);
    let text = on_agent_line["contents"]["value"].as_str().unwrap();
    assert!(text.contains("**claude**") && text.contains("handle empty input") && text.contains("01M7ZVPH6CYV3Z7X9D6W7CVH75"), "{text}");
    assert_eq!(on_agent_line["contents"]["kind"], "markdown");
    assert_eq!(hover(&mut server, URI, 0), Value::Null, "a line from before Tracewise");
    assert_eq!(hover(&mut server, URI, 99), Value::Null, "a line that is not there");
    let no_reason = hover(&mut server, URI, 4);
    assert!(no_reason["contents"]["value"].as_str().unwrap().contains("_no reason given_"));
}

#[test]
fn a_buffer_with_unsaved_edits_says_nothing_until_it_is_saved() {
    let (_d, program, calls) = fake();
    let mut server = Server::new(program);
    open(&mut server, URI);
    assert_ne!(hover(&mut server, URI, 1), Value::Null);
    server.handle(&notify("textDocument/didChange", json!({ "textDocument": doc(URI) })));
    assert_eq!(hover(&mut server, URI, 1), Value::Null, "the file on disk is not what the editor shows");
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
    assert_ne!(hover(&mut server, URI, 1), Value::Null, "after the save the answer is back, from fresh blame");
    let log = std::fs::read_to_string(calls).unwrap();
    assert!(log.contains("-C /work/app/src record --source lsp /work/app/src/parser.rs --json"), "{log}");
    assert_eq!(log.matches("blame").count(), 2, "blame was asked again after the save");
}

#[test]
fn blame_is_asked_once_until_the_file_changes() {
    let (_d, program, calls) = fake();
    let mut server = Server::new(program);
    open(&mut server, URI);
    hover(&mut server, URI, 1);
    hover(&mut server, URI, 2);
    hover(&mut server, URI, 4);
    assert_eq!(std::fs::read_to_string(calls).unwrap().matches("blame").count(), 1);
}

#[test]
fn inlay_hints_sit_at_the_end_of_the_first_line_of_each_changed_run() {
    let (_d, program, _) = fake();
    let mut server = Server::new(program);
    server.handle(&request(1, "initialize", json!({"initializationOptions": {"inlayBlame": true}})));
    open(&mut server, URI);
    let hints = server.handle(&request(3, "textDocument/inlayHint", json!({ "textDocument": doc(URI) }))).unwrap()["result"].clone();
    let hints = hints.as_array().unwrap();
    assert_eq!(hints.len(), 2, "the agent's two lines are one run; baseline lines have none: {hints:?}");
    assert_eq!(hints[0]["position"], json!({ "line": 1, "character": "    let tokens = lex(input);".len() }));
    assert!(hints[0]["label"].as_str().unwrap().starts_with("claude · "), "{}", hints[0]["label"]);
    assert!(hints[0]["label"].as_str().unwrap().ends_with("handle empty input"));
    assert_eq!(hints[1]["position"]["line"], 4);
    assert_eq!(hints[1]["label"].as_str().unwrap().split(" · ").next(), Some("as"));
}

#[test]
fn hint_positions_count_utf16_units_like_the_protocol_says() {
    assert_eq!(utf16_len("plain"), 5);
    assert_eq!(utf16_len("é"), 1, "one unit, though two bytes");
    assert_eq!(utf16_len("😀"), 2, "two units, though one character");
    assert_eq!(utf16_len("a😀é"), 4);
}

#[test]
fn inlay_hints_are_empty_when_not_asked_for() {
    let (_d, program, _) = fake();
    let mut server = Server::new(program);
    server.handle(&request(1, "initialize", json!({})));
    open(&mut server, URI);
    assert_eq!(server.handle(&request(3, "textDocument/inlayHint", json!({ "textDocument": doc(URI) }))).unwrap()["result"], json!([]));
}

#[test]
fn an_unknown_request_is_answered_with_an_error_and_an_unknown_notification_is_ignored() {
    let (_d, program, _) = fake();
    let mut server = Server::new(program);
    let reply = server.handle(&request(9, "textDocument/codeLens", json!({}))).unwrap();
    assert_eq!(reply["error"]["code"], -32601);
    assert_eq!(reply["id"], 9);
    assert!(server.handle(&notify("$/setTrace", json!({}))).is_none());
    assert!(server.handle(&notify("initialized", json!({}))).is_none());
}

#[test]
fn only_files_have_a_path() {
    assert_eq!(path_of("file:///a/b%20c.rs"), Some(PathBuf::from("/a/b c.rs")));
    assert_eq!(path_of("file://localhost/a/b.rs"), Some(PathBuf::from("/a/b.rs")));
    assert_eq!(path_of("file:///a/%C3%A9.rs"), Some(PathBuf::from("/a/é.rs")));
    assert_eq!(path_of("untitled:Untitled-1"), None);
    assert_eq!(path_of("vscode-remote://host/a.rs"), None);
    assert_eq!(path_of("file://relative/a.rs"), None, "a host that is not localhost is not a path on this machine");
    assert_eq!(path_of("file:///a/%zz.rs"), None, "bad escapes are refused, not guessed");
    assert_eq!(path_of("file:///a/%"), Some(PathBuf::from("/a/%")), "a lone percent sign stays");
}

fn run(server: &mut Server, messages: &[Value]) -> (i32, Vec<Value>) {
    let mut wire = Vec::new();
    for m in messages {
        write_message(&mut wire, m).unwrap();
    }
    let mut out = Vec::new();
    let code = serve(server, &mut Cursor::new(wire), &mut out);
    let mut replies = Vec::new();
    let mut cursor = Cursor::new(out);
    while let Some(m) = read_message(&mut cursor).unwrap() {
        replies.push(m);
    }
    (code, replies)
}

#[test]
fn a_clean_shutdown_exits_with_0_and_exit_without_it_with_1() {
    let (_d, program, _) = fake();
    let (code, replies) = run(
        &mut Server::new(program.clone()),
        &[request(1, "initialize", json!({})), request(2, "shutdown", json!(null)), notify("exit", json!(null))],
    );
    assert_eq!(code, 0);
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[1]["result"], Value::Null);
    let (code, _) = run(&mut Server::new(program.clone()), &[notify("exit", json!(null))]);
    assert_eq!(code, 1);
    let (code, _) = run(&mut Server::new(program), &[]);
    assert_eq!(code, 1, "the editor went away without a shutdown");
}

/// A stand-in whose answer to `blame` is a shell snippet (`sleep 30`, `exit 1`, ...), counting its calls.
fn fake_blame(snippet: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("tracewise");
    let calls = dir.path().join("calls.log");
    std::fs::write(
        &program,
        format!("#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$*\" in\n  *blame*) {snippet} ;;\n  *) echo '{{}}' ;;\nesac\n", calls.display()),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, program, calls)
}

fn count(calls: &Path, what: &str) -> usize {
    std::fs::read_to_string(calls).unwrap_or_default().matches(what).count()
}

#[test]
fn a_stuck_blame_is_killed_at_the_deadline_and_not_asked_again_straight_away() {
    // the deadline is generous: the first run of a program that was just written can take a moment on macOS (it is scanned first)
    let (_d, program, calls) = fake_blame("sleep 30");
    let mut server = Server::new(program).with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    open(&mut server, URI);
    let started = Instant::now();
    assert_eq!(hover(&mut server, URI, 1), Value::Null);
    assert!(started.elapsed() < Duration::from_secs(10), "the editor was held for {:?}", started.elapsed());
    let asked = count(&calls, "blame");
    assert_eq!(asked, 1, "it was asked once and killed");
    let second = Instant::now();
    assert_eq!(hover(&mut server, URI, 1), Value::Null);
    assert!(second.elapsed() < Duration::from_millis(500), "no second wait: a failure that may pass is retried after a while");
    assert_eq!(count(&calls, "blame"), asked, "not asked again on the next hover");
    // a save is a reason to ask again
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
    hover(&mut server, URI, 1);
    assert_eq!(count(&calls, "blame"), asked + 1);
}

#[test]
fn a_project_with_no_history_is_an_answer_and_is_remembered() {
    let (_d, program, calls) = fake_blame("exit 1");
    let mut server = Server::new(program);
    open(&mut server, URI);
    assert_eq!(hover(&mut server, URI, 1), Value::Null);
    assert_eq!(hover(&mut server, URI, 2), Value::Null);
    assert_eq!(count(&calls, "blame"), 1);
}

#[test]
fn a_child_with_a_long_answer_does_not_hang_the_wait() {
    let big = vec![
        "-c".into(),
        "i=0; while [ $i -lt 3000 ]; do echo 'a line of output that is long enough to fill a pipe quickly, many times over'; i=$((i+1)); done".into(),
    ];
    let answer = run_with_timeout(Path::new("/bin/sh"), &big, Duration::from_secs(10)).unwrap();
    assert_eq!(answer.code, Some(0));
    assert!(answer.stdout.len() > 200_000, "all of the output arrived: {}", answer.stdout.len());
}

#[test]
fn a_buffer_equal_to_the_file_is_clean_and_one_that_differs_is_not() {
    let (_d, program, calls) = fake();
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("a.txt");
    std::fs::write(&file, "on disk\n").unwrap();
    let uri = format!("file://{}", file.display());
    let mut server = Server::new(program);
    // opened with the text the file holds: clean
    server.handle(&notify("textDocument/didOpen", json!({ "textDocument": { "uri": uri, "text": "on disk\n" } })));
    assert_ne!(hover(&mut server, &uri, 1), Value::Null);
    // typing: differs from disk, says nothing
    server.handle(&notify(
        "textDocument/didChange",
        json!({ "textDocument": doc(&uri), "contentChanges": [{ "text": "on disk\nand more" }] }),
    ));
    assert_eq!(hover(&mut server, &uri, 1), Value::Null);
    // an agent rewrote the file and the editor reloaded it: the buffer equals the file again, with no save in between
    std::fs::write(&file, "rewritten by an agent\n").unwrap();
    server.handle(&notify(
        "textDocument/didChange",
        json!({ "textDocument": doc(&uri), "contentChanges": [{ "text": "rewritten by an agent\n" }] }),
    ));
    assert_ne!(hover(&mut server, &uri, 1), Value::Null, "a reload is not unsaved work");
    assert_eq!(count(&calls, "blame"), 2, "blame was asked again for the new content");
    // reopened with unsaved text
    server.handle(&notify("textDocument/didOpen", json!({ "textDocument": { "uri": uri, "text": "restored, never saved\n" } })));
    assert_eq!(hover(&mut server, &uri, 1), Value::Null);
}

#[test]
fn a_hover_without_a_position_is_null_not_a_crash() {
    let (_d, program, _) = fake();
    let mut server = Server::new(program);
    open(&mut server, URI);
    let reply = server.handle(&request(2, "textDocument/hover", json!({ "textDocument": doc(URI) }))).unwrap();
    assert_eq!(reply["result"], Value::Null);
}

#[test]
fn a_message_that_is_not_json_is_answered_and_the_server_goes_on() {
    let (_d, program, _) = fake();
    let mut wire = Vec::new();
    wire.extend_from_slice(b"Content-Length: 5\r\n\r\n{nope");
    write_message(&mut wire, &request(1, "initialize", json!({}))).unwrap();
    write_message(&mut wire, &request(2, "shutdown", json!(null))).unwrap();
    write_message(&mut wire, &notify("exit", json!(null))).unwrap();
    let mut out = Vec::new();
    let code = serve(&mut Server::new(program), &mut Cursor::new(wire), &mut out);
    assert_eq!(code, 0);
    let mut replies = Vec::new();
    let mut cursor = Cursor::new(out);
    while let Some(m) = read_message(&mut cursor).unwrap() {
        replies.push(m);
    }
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies.len(), 3, "the parse error, then the two real replies: {replies:?}");
}

#[test]
fn a_frame_that_claims_to_be_huge_is_refused_without_allocating_it() {
    let err = read_message(&mut Cursor::new(b"Content-Length: 99999999999\r\n\r\n".to_vec())).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    let (_d, program, _) = fake();
    let code = serve(&mut Server::new(program), &mut Cursor::new(b"Content-Length: 99999999999\r\n\r\n".to_vec()), &mut Vec::new());
    assert_eq!(code, 1);
}

#[test]
fn a_request_after_shutdown_is_answered_with_an_error() {
    let (_d, program, _) = fake();
    let mut server = Server::new(program);
    server.handle(&request(1, "shutdown", json!(null)));
    let reply = server.handle(&request(2, "textDocument/hover", json!({}))).unwrap();
    assert_eq!(reply["error"]["code"], -32600);
    assert!(server.handle(&notify("textDocument/didOpen", json!({}))).is_none());
}

#[test]
fn a_save_asks_why_and_the_chosen_button_becomes_the_note() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("tracewise");
    let calls = dir.path().join("calls.log");
    std::fs::write(
        &program,
        format!("#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$*\" in\n  *record*) echo '{{\"changes\":[\"c1\",\"c2\"]}}' ;;\n  *) echo '{{}}' ;;\nesac\n", calls.display()),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = Server::new(program).with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    server.handle(&request(1, "initialize", json!({ "initializationOptions": { "askWhen": "every-save" } })));
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
    let sent = std::mem::take(&mut server.outbox);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["method"], "window/showMessageRequest");
    let titles: Vec<_> = sent[0]["params"]["actions"].as_array().unwrap().iter().map(|a| a["title"].as_str().unwrap().to_string()).collect();
    assert_eq!(titles, ["Bug fix", "Refactor", "New feature", "Type my own…", "Skip", "Don't ask again"]);
    let id = sent[0]["id"].clone();
    server.handle(&json!({ "jsonrpc": "2.0", "id": id, "result": { "title": "Refactor" } }));
    assert_eq!(count(&calls, "note c1 Refactor --label"), 1);
    assert_eq!(count(&calls, "note c2 Refactor --label"), 1);
    // an answer is used once; a closed popup (no result) writes nothing
    server.handle(&json!({ "jsonrpc": "2.0", "id": id, "result": { "title": "Bug fix" } }));
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
    let second = std::mem::take(&mut server.outbox);
    let titles: Vec<_> = second[0]["params"]["actions"].as_array().unwrap().iter().map(|a| a["title"].as_str().unwrap().to_string()).collect();
    assert_eq!(titles[0], "Same as last: Refactor", "the previous answer is offered first");
    let id2 = second[0]["id"].clone();
    server.handle(&json!({ "jsonrpc": "2.0", "id": id2, "result": null }));
    assert_eq!(count(&calls, "note c1 Refactor --label"), 1, "a closed popup writes no reason of a person's");
    assert_eq!(count(&calls, "note c1 no reason given --auto"), 1, "only an automatic one");
    // "Same as last" writes that reason; "Don't ask again" stops the questions
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
    let id3 = std::mem::take(&mut server.outbox)[0]["id"].clone();
    server.handle(&json!({ "jsonrpc": "2.0", "id": id3, "result": { "title": "Same as last: Refactor" } }));
    assert_eq!(count(&calls, "note c1 Refactor --label"), 2);
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
    let id4 = std::mem::take(&mut server.outbox)[0]["id"].clone();
    server.handle(&json!({ "jsonrpc": "2.0", "id": id4, "result": { "title": "Don't ask again" } }));
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
    assert!(server.outbox.is_empty(), "muted");
}

fn when_server(options: Value) -> (tempfile::TempDir, Server, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("tracewise");
    let calls = dir.path().join("calls.log");
    std::fs::write(&program, format!("#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$*\" in\n  *record*) echo '{{\"changes\":[\"c1\"]}}' ;;\n  *) echo '{{}}' ;;\nesac\n", calls.display())).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = Server::new(program).with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    server.handle(&request(1, "initialize", json!({ "initializationOptions": options })));
    (dir, server, calls)
}

fn save(server: &mut Server) {
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
}

fn secs(server: &mut Server, n: u64) {
    server.skew += Duration::from_secs(n);
    server.tick();
}

fn question_changes(server: &mut Server) -> Vec<Value> {
    std::mem::take(&mut server.outbox)
}

#[test]
fn pause_waits_until_the_person_stops_saving_then_asks_once() {
    let (_d, mut server, _) = when_server(json!({ "askWhen": "pause", "askAfterSeconds": 10 }));
    save(&mut server);
    secs(&mut server, 4);
    save(&mut server);
    secs(&mut server, 4);
    save(&mut server);
    secs(&mut server, 9);
    assert!(question_changes(&mut server).is_empty(), "still saving: nothing asked");
    assert!(server.next_wakeup().is_some(), "the loop is told to wake");
    secs(&mut server, 2);
    let q = question_changes(&mut server);
    assert_eq!(q.len(), 1, "one question for the burst");
    secs(&mut server, 60);
    assert!(question_changes(&mut server).is_empty(), "and not again");
    assert!(server.next_wakeup().is_none());
}

#[test]
fn every_save_asks_each_time_and_manual_never() {
    let (_d, mut every, _) = when_server(json!({ "askWhen": "every-save" }));
    save(&mut every);
    save(&mut every);
    assert_eq!(question_changes(&mut every).len(), 2);
    let (_d2, mut manual, _) = when_server(json!({ "askWhen": "manual" }));
    save(&mut manual);
    secs(&mut manual, 3600);
    assert!(question_changes(&mut manual).is_empty());
}

#[test]
fn interval_asks_at_once_then_at_most_once_per_interval() {
    let (_d, mut server, _) = when_server(json!({ "askWhen": "interval", "askEverySeconds": 60 }));
    save(&mut server);
    assert_eq!(question_changes(&mut server).len(), 1);
    secs(&mut server, 10);
    save(&mut server);
    assert!(question_changes(&mut server).is_empty(), "inside the interval it waits");
    secs(&mut server, 51);
    assert_eq!(question_changes(&mut server).len(), 1, "and is asked when the interval has passed");
}

#[test]
fn leave_asks_when_the_file_is_closed_and_a_list_means_whichever_comes_first() {
    let (_d, mut server, _) = when_server(json!({ "askWhen": ["leave"] }));
    save(&mut server);
    secs(&mut server, 3600);
    assert!(question_changes(&mut server).is_empty(), "no timer in this mode");
    server.handle(&notify("textDocument/didClose", json!({ "textDocument": doc(URI) })));
    assert_eq!(question_changes(&mut server).len(), 1);
    let (_d2, mut both, _) = when_server(json!({ "askWhen": ["pause", "leave"], "askAfterSeconds": 10 }));
    save(&mut both);
    both.handle(&notify("textDocument/didClose", json!({ "textDocument": doc(URI) })));
    assert_eq!(question_changes(&mut both).len(), 1, "closed before the pause");
    save(&mut both);
    secs(&mut both, 11);
    assert_eq!(question_changes(&mut both).len(), 1, "or the pause comes first");
}

#[test]
fn an_unknown_askwhen_falls_back_to_pause_and_skip_leaves_no_note() {
    let (_d, mut server, calls) = when_server(json!({ "askWhen": "sometimes" }));
    save(&mut server);
    secs(&mut server, 0);
    let q = question_changes(&mut server);
    assert_eq!(q.len(), 1);
    server.handle(&json!({ "jsonrpc": "2.0", "id": q[0]["id"], "result": { "title": "Skip" } }));
    assert_eq!(count(&calls, "note c1 skipped --auto"), 1);
}

#[test]
fn type_my_own_opens_a_file_and_saving_it_notes_what_was_typed() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let program = dir.path().join("tracewise");
    let calls = dir.path().join("calls.log");
    std::fs::write(&program, format!("#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$*\" in\n  *record*) echo '{{\"changes\":[\"c1\"]}}' ;;\n  *status*) echo '{{\"data_dir\":\"{}\"}}' ;;\n  *) echo '{{}}' ;;\nesac\n", calls.display(), data.path().display())).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = Server::new(program).with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    server.handle(&request(1, "initialize", json!({ "capabilities": { "window": { "showDocument": { "support": true } } }, "initializationOptions": { "askWhen": "every-save" } })));
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
    let id = std::mem::take(&mut server.outbox)[0]["id"].clone();
    server.handle(&json!({ "jsonrpc": "2.0", "id": id, "result": { "title": "Type my own…" } }));
    let sent = std::mem::take(&mut server.outbox);
    assert_eq!(sent[0]["method"], "window/showDocument");
    let file = data.path().join("why.md");
    assert!(std::fs::read_to_string(&file).unwrap().starts_with("<!--"));
    // saving without typing does nothing; typing and saving notes the change and empties the file
    let uri = format!("file://{}", file.display());
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": { "uri": uri } })));
    assert_eq!(count(&calls, "note"), 0);
    std::fs::write(&file, format!("{}fixing the parser crash\n", WHY_HEADER_FOR_TEST)).unwrap();
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": { "uri": uri } })));
    assert_eq!(count(&calls, "note c1 fixing the parser crash"), 1);
    assert_eq!(count(&calls, "record"), 1, "saving the reason file is not recorded as a change");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), WHY_HEADER_FOR_TEST);
}
const WHY_HEADER_FOR_TEST: &str = super::WHY_HEADER;

fn stats_server(stats: &str, options: Value) -> (tempfile::TempDir, Server, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("tracewise");
    let calls = dir.path().join("calls.log");
    std::fs::write(
        &program,
        format!("#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$*\" in\n  *record*) echo '{{\"changes\":[\"c1\"],\"stats\":[{{\"id\":\"c1\",\"path\":\"{}\",\"op\":\"modify\",\"stats\":{}}}]}}' ;;\n  *) echo '{{}}' ;;\nesac\n", calls.display(), "src/a.rs", stats),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut server = Server::new(program).with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    server.handle(&request(1, "initialize", json!({ "initializationOptions": options })));
    (dir, server, calls)
}

#[test]
fn a_whitespace_only_save_is_not_asked_about_and_gets_an_automatic_reason() {
    let (_d, mut server, calls) = stats_server(r#"{"added_lines":1,"removed_lines":1,"added_chars":2,"removed_chars":4,"whitespace_only":true}"#, json!({ "askWhen": "every-save" }));
    save(&mut server);
    assert!(server.outbox.is_empty());
    assert_eq!(count(&calls, "note c1 whitespace --auto"), 1);
}

#[test]
fn askminchars_skips_small_saves_and_asks_about_big_ones() {
    let small = r#"{"added_lines":1,"removed_lines":1,"added_chars":1,"removed_chars":1,"whitespace_only":false}"#;
    let (_d, mut server, calls) = stats_server(small, json!({ "askWhen": "every-save", "askMinChars": 5 }));
    save(&mut server);
    assert!(server.outbox.is_empty());
    assert_eq!(count(&calls, "note c1 minor edit (2 characters) --auto"), 1);
    let big = r#"{"added_lines":3,"removed_lines":0,"added_chars":90,"removed_chars":0,"whitespace_only":false}"#;
    let (_d2, mut server, calls) = stats_server(big, json!({ "askWhen": "every-save", "askMinChars": 5 }));
    save(&mut server);
    assert_eq!(server.outbox.len(), 1);
    assert_eq!(count(&calls, "--auto"), 0);
}

#[test]
fn ignored_files_are_not_asked_about_and_globs_match_as_documented() {
    let big = r#"{"added_lines":3,"removed_lines":0,"added_chars":90,"removed_chars":0,"whitespace_only":false}"#;
    let (_d, mut server, calls) = stats_server(big, json!({ "askWhen": "every-save", "askIgnore": ["src/*.rs"] }));
    save(&mut server);
    assert!(server.outbox.is_empty());
    assert_eq!(count(&calls, "not asked (ignored file) --auto"), 1);
    assert!(tracewise_core::glob_match_anywhere("*.lock", "a/b/Cargo.lock"));
    assert!(tracewise_core::glob_match_anywhere("**/target/**", "target/debug/x"));
    assert!(tracewise_core::glob_match_anywhere("**/target/**", "crates/x/target/debug/x"));
    assert!(tracewise_core::glob_match_anywhere("src/*.rs", "src/a.rs") && !tracewise_core::glob_match_anywhere("src/*.rs", "src/deep/a.rs"));
    assert!(tracewise_core::glob_match_anywhere("dist/**", "dist/a/b.js") && !tracewise_core::glob_match_anywhere("dist/**", "distant/a.js"));
    assert!(!tracewise_core::glob_match_anywhere("*.md", "docs/readme.txt"));
}

#[test]
fn another_server_process_can_take_the_save_of_the_reason_file() {
    // Zed starts one `tracewise lsp` per folder it opens, and the reason file is in a folder of its own
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let program = dir.path().join("tracewise");
    let calls = dir.path().join("calls.log");
    std::fs::write(&program, format!("#!/bin/sh\necho \"$@\" >> '{}'\necho '{{}}'\n", calls.display())).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let file = data.path().join("why.md");
    std::fs::write(data.path().join("why.pending.json"), json!({ "file": "/p/src/a.rs", "changes": ["c1", "c2"] }).to_string()).unwrap();
    // the person typed into the first line as well: it is still a comment, not part of the reason
    std::fs::write(&file, "<!-- Wite why. -->\nchanged and saved from why.md\n").unwrap();
    let mut server = Server::new(program).with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": { "uri": format!("file://{}", file.display()) } })));
    assert_eq!(count(&calls, "note c1 changed and saved from why.md"), 1);
    assert_eq!(count(&calls, "note c2 changed and saved from why.md"), 1);
    assert_eq!(count(&calls, "record"), 0, "the reason file is not recorded as a change");
    assert!(!data.path().join("why.pending.json").exists());
}

#[test]
fn a_typed_reason_that_is_refused_is_explained_and_kept_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let program = dir.path().join("tracewise");
    std::fs::write(&program, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let file = data.path().join("why.md");
    std::fs::write(data.path().join("why.pending.json"), json!({ "file": "/p/a.rs", "changes": ["c1"] }).to_string()).unwrap();
    std::fs::write(&file, format!("{}dvfb\n", WHY_HEADER_FOR_TEST)).unwrap();
    let mut server = Server::new(program).with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": { "uri": format!("file://{}", file.display()) } })));
    let sent = std::mem::take(&mut server.outbox);
    assert_eq!(sent[0]["method"], "window/showMessage");
    assert!(sent[0]["params"]["message"].as_str().unwrap().contains("12 characters"));
    assert!(std::fs::read_to_string(&file).unwrap().contains("dvfb"), "the text stays for the person to fix");
    assert!(data.path().join("why.pending.json").exists());
}

#[test]
fn the_client_name_decides_the_source_and_hover_says_where_a_person_edited_from() {
    assert_eq!((source_of("Zed"), source_of("Visual Studio Code"), source_of("IntelliJ IDEA"), source_of("neovim")), ("zed", "vscode", "jetbrains", "lsp"));
    let (_d, program, calls) = fake();
    let mut server = Server::new(program).with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    server.handle(&request(1, "initialize", json!({ "clientInfo": { "name": "Zed", "version": "1" } })));
    server.handle(&notify("textDocument/didSave", json!({ "textDocument": doc(URI) })));
    assert!(std::fs::read_to_string(&calls).unwrap().contains("record --source zed "), "{}", std::fs::read_to_string(&calls).unwrap());
    let human = Row {
        line: 5,
        text: String::new(),
        change: "c".into(),
        actor: Actor { kind: "human".into(), name: Some("as".into()) },
        time: String::new(),
        why: None,
        source: Some("zed".into()),
    };
    assert!(hover_text(&human, chrono::Utc::now()).starts_with("**as via Zed**"), "{}", hover_text(&human, chrono::Utc::now()));
    let agent = Row { actor: Actor { kind: "agent".into(), name: Some("claude".into()) }, ..human };
    assert!(hover_text(&agent, chrono::Utc::now()).starts_with("**claude**"), "only a person is 'via' an editor");
}
