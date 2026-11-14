//! `tracewise hook --neutral` and `tracewise companion`: what a program that nests Tracewise inside its own hook relies on. The same work
//! as the agent-schema hook (locks, notices), answered as `{api, event, decision, reason, context}`.
mod common;
use common::Repo;

const COORD: &str = "[profile]\ncoordination = true\n";

/// A Claude-style payload for one tool call.
fn payload(event: &str, session: &str, tool: &str, input: serde_json::Value) -> String {
    serde_json::json!({"hook_event_name": event, "session_id": session, "tool_name": tool, "tool_input": input}).to_string()
}

fn neutral(repo: &Repo, from: &str, body: &str) -> serde_json::Value {
    let run = repo.with("human:me", &["hook", "--neutral", "--from", from], Some(body), &[]);
    assert!(run.ok(), "{}", run.err);
    assert_eq!(run.out.lines().count(), 1, "one JSON line: {}", run.out);
    run.json()
}

#[test]
fn companion_says_its_api_and_works_outside_a_repository() {
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_tracewise"))
        .args(["companion", "--json"])
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(run.status.success());
    let v: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(v["api"], 1);
    assert_eq!(v["name"], "tracewise");
    assert!(v["features"].as_array().unwrap().iter().any(|f| f == "neutral-hook"));
}

#[test]
fn a_held_file_is_refused_to_the_second_session_in_neutral_form_and_the_first_is_allowed() {
    let repo = Repo::new(COORD);
    repo.write("a.txt", "one\n");
    let edit = |session: &str| neutral(&repo, "claude", &payload("PreToolUse", session, "Edit", serde_json::json!({"file_path": "a.txt"})));
    let first = edit("s1");
    assert_eq!(
        (first["api"].as_u64(), first["event"].as_str(), first["decision"].as_str()),
        (Some(1), Some("pre-edit"), Some("allow")),
        "{first}"
    );
    let second = edit("s2");
    assert_eq!(second["decision"], "deny", "{second}");
    assert!(
        second["reason"].as_str().unwrap_or("").contains("a.txt") || second["reason"].as_str().unwrap_or("").contains("s1"),
        "{second}"
    );
    assert_eq!(edit("s1")["decision"], "allow", "the holder keeps going");
}

#[test]
fn the_same_hook_answers_an_antigravity_payload_and_ends_the_holders_locks_at_stop() {
    let repo = Repo::new(COORD);
    // Antigravity's CLI event comes as the hook's argument, not inside the payload
    let run = |event: &str, conv: &str, args: serde_json::Value| {
        let body = serde_json::json!({"conversationId": conv, "toolCall": {"name": "write_to_file", "args": args}}).to_string();
        let r = repo.with("human:me", &["hook", event, "--neutral", "--from", "antigravity"], Some(&body), &[]);
        assert!(r.ok(), "{}", r.err);
        r.json()
    };
    let first = run("PreToolUse", "c1", serde_json::json!({"TargetFile": "b.txt"}));
    assert_eq!((first["event"].as_str(), first["decision"].as_str()), (Some("pre-edit"), Some("allow")), "{first}");
    let blocked = run("PreToolUse", "c2", serde_json::json!({"TargetFile": "b.txt"}));
    assert_eq!(blocked["decision"], "deny", "{blocked}");
    let stop = repo.with("human:me", &["hook", "Stop", "--neutral", "--from", "antigravity"], Some(r#"{"conversationId":"c1"}"#), &[]);
    assert_eq!(stop.json()["event"], "stop");
    let after = run("PreToolUse", "c2", serde_json::json!({"TargetFile": "b.txt"}));
    assert_eq!(after["decision"], "allow", "Tracewise releases a session's locks at Stop: {after}");
}

#[test]
fn unknown_agents_bad_json_and_other_events_are_a_quiet_allow() {
    let repo = Repo::new(COORD);
    for (from, body) in [("nobody", "{}"), ("claude", "not json"), ("claude", r#"{"hook_event_name":"Notification"}"#)] {
        let v = neutral(&repo, from, body);
        assert_eq!((v["decision"].as_str(), v["event"].as_str()), (Some("allow"), Some("none")), "{from} {body}");
        assert!(v["context"].as_array().unwrap().is_empty());
    }
    // outside any repository: still an allow, still one line, exit 0
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tracewise"))
        .args(["hook", "--neutral", "--from", "claude"])
        .current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["decision"], "allow");
}

#[test]
fn lock_list_json_carries_the_stable_fields() {
    let repo = Repo::new(COORD);
    repo.write("a.txt", "one\n");
    neutral(&repo, "claude", &payload("PreToolUse", "s1", "Edit", serde_json::json!({"file_path": "a.txt"})));
    let run = repo.me(&["lock", "list", "--json"]);
    assert!(run.ok(), "{}", run.err);
    let rows = run.json();
    let row = &rows.as_array().unwrap()[0];
    assert_eq!(row["path"], "a.txt");
    assert_eq!(row["agent"], "claude");
    assert_eq!(row["session"], "s1");
    assert!(row["expires_in_s"].as_i64().unwrap() > 0, "{row}");
    assert!(row["since"].is_string() && row["holder"].is_object(), "the record's own fields stay: {row}");
}
