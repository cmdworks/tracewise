//! `capture.mode`: with no watcher running, a hand edit is still recorded: before a command reads the history, and when an
//! agent turn starts, never blamed on the agent whose turn it was.
mod common;
use common::Repo;

const AGENTS: &str = "[capture]\nmode = \"agents\"\n";

fn rows_for(repo: &Repo, file: &str) -> Vec<serde_json::Value> {
    repo.rows(&[]).into_iter().filter(|r| r["path"] == file).collect()
}

#[test]
fn a_hand_edit_shows_in_the_log_with_no_watcher_in_agents_mode() {
    let repo = Repo::new(AGENTS);
    repo.write("a.txt", "by hand\n");
    let rows = rows_for(&repo, "a.txt");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["source"], "catch-up");
    assert_eq!(rows[0]["op"], "create");
}

#[test]
fn full_mode_catches_up_too_when_its_watcher_is_not_running() {
    let repo = Repo::new("");
    repo.write("a.txt", "by hand\n");
    assert_eq!(rows_for(&repo, "a.txt").len(), 1);
}

#[test]
fn blame_knows_a_line_added_by_hand_through_the_catch_up() {
    let repo = Repo::new(AGENTS);
    repo.write("a.txt", "one\n");
    assert!(repo.me(&["blame", "a.txt"]).ok());
    repo.later();
    repo.write("a.txt", "one\ntwo\n");
    let blame = repo.me(&["blame", "a.txt", "--json"]).json();
    let second = &blame.as_array().unwrap()[1];
    assert!(second["change"].as_str().is_some_and(|c| c.len() == 26), "the line has a real change behind it, not a baseline: {second}");
    assert_eq!(second["actor"]["kind"], "human");
}

#[test]
fn status_counts_what_it_caught_up() {
    let repo = Repo::new(AGENTS);
    let before = repo.me(&["status", "--json"]).json()["changes"].as_u64().unwrap();
    repo.write("a.txt", "by hand\n");
    assert_eq!(repo.me(&["status", "--json"]).json()["changes"].as_u64().unwrap(), before + 1);
}

#[test]
fn a_prompt_catches_up_when_an_agent_window_is_still_open_and_names_the_person() {
    let repo = Repo::new(AGENTS);
    let body =
        |event: &str, path: &str| serde_json::json!({"agent": "claude", "session": "s1", "paths": [path], "event": event}).to_string();
    // the agent starts an edit of b.txt and never finishes it: its window stays open
    assert!(repo.with("agent:claude:s1", &["hook", "pre-edit"], Some(&body("pre-edit", "b.txt")), &[]).ok());
    repo.write("a.txt", "mine\n");
    let prompt = serde_json::json!({"agent": "claude", "session": "s1", "paths": []}).to_string();
    assert!(repo.with("agent:claude:s1", &["hook", "prompt"], Some(&prompt), &[]).ok());
    // the stamp is written by the prompt's own catch-up: nothing else has read the history yet
    assert!(repo.data().join("catch-up").is_file(), "the prompt hook did not catch up");
    let rows = repo.rows(&[]);
    let a = rows.iter().find(|r| r["path"] == "a.txt").unwrap_or_else(|| panic!("a.txt was not recorded"));
    assert_eq!(
        (a["source"].as_str(), a["actor"]["kind"].as_str(), a["confidence"].as_str()),
        (Some("catch-up"), Some("human"), Some("likely")),
        "{a}"
    );
}

#[test]
fn an_agent_running_log_records_nothing_and_the_next_prompt_names_the_person() {
    let repo = Repo::new(AGENTS);
    repo.write("a.txt", "mine\n");
    let agent_env = [("CLAUDECODE", "1"), ("CLAUDE_CODE_SESSION_ID", "s1")];
    let log = repo.with("human:me", &["log", "--json"], None, &agent_env);
    assert!(log.ok(), "{}", log.err);
    assert!(log.json().as_array().unwrap().iter().all(|r| r["path"] != "a.txt"), "an agent's `log` must not record the edit");
    let prompt = serde_json::json!({"agent": "claude", "session": "s1", "paths": []}).to_string();
    assert!(repo.with("agent:claude:s1", &["hook", "prompt"], Some(&prompt), &[]).ok());
    let rows = repo.rows(&[]);
    let a = rows.iter().find(|r| r["path"] == "a.txt").expect("recorded at the prompt");
    assert_eq!((a["actor"]["kind"].as_str(), a["confidence"].as_str()), (Some("human"), Some("likely")), "{a}");
}

#[test]
fn a_command_in_an_unadopted_copy_does_not_write_into_the_original() {
    let repo = Repo::new(AGENTS);
    repo.write("a.txt", "one\n");
    assert!(repo.me(&["scan"]).ok());
    let before = repo.rows(&[]).len();
    let copy = tempfile::tempdir().unwrap();
    let copied = std::process::Command::new("cp").arg("-R").arg(format!("{}/.", repo.dir().display())).arg(copy.path()).status().unwrap();
    assert!(copied.success());
    std::fs::write(copy.path().join("a.txt"), "one\nand a change that is not the original's\n").unwrap();
    std::fs::write(copy.path().join("new.txt"), "new in the copy\n").unwrap();
    let copy_path = copy.path().to_str().unwrap();
    assert!(repo.me(&["-C", copy_path, "status"]).ok());
    assert!(repo.me(&["-C", copy_path, "log"]).ok());
    assert_eq!(repo.rows(&[]).len(), before, "the copy's files are not changes of the original");
}

#[test]
fn switching_the_mode_back_and_forth_keeps_the_history() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    assert!(repo.me(&["scan"]).ok());
    let paths = |repo: &Repo| -> Vec<String> { repo.rows(&[]).iter().map(|r| r["path"].as_str().unwrap_or("").to_string()).collect() };
    assert!(paths(&repo).contains(&"a.txt".to_string()));
    assert!(repo.me(&["config", "set", "capture.mode", "agents"]).ok());
    assert!(repo.me(&["config", "get", "capture.mode"]).out.starts_with("capture.mode = agents"));
    repo.write("b.txt", "two\n");
    let in_agents_mode = paths(&repo);
    assert!(in_agents_mode.contains(&"b.txt".to_string()), "agents mode catches b.txt up: {in_agents_mode:?}");
    assert!(repo.me(&["config", "set", "capture.mode", "full"]).ok());
    let back_in_full = paths(&repo);
    for p in ["a.txt", "b.txt"] {
        assert!(back_in_full.contains(&p.to_string()), "{p} is still in the history: {back_in_full:?}");
    }
}

#[test]
fn the_capture_setting_is_checked() {
    let repo = Repo::new("");
    assert!(repo.me(&["config", "get", "capture.mode"]).out.starts_with("capture.mode = full"));
    let bad = repo.me(&["config", "set", "capture.mode", "sometimes"]);
    assert!(!bad.ok(), "an unknown mode is refused");
    assert!(repo.me(&["config", "get", "capture.mode"]).out.starts_with("capture.mode = full"));
}

#[test]
fn status_and_doctor_say_which_mode_is_on_and_what_it_misses() {
    let repo = Repo::new(AGENTS);
    let status = repo.me(&["status", "--json"]).json();
    assert_eq!(status["capture"]["mode"], "agents");
    let doctor = repo.me(&["doctor", "--json"]).json();
    let capture = &doctor["coverage"]["capture"];
    assert_eq!(capture["mode"], "agents");
    assert_eq!(capture["live"].as_array().unwrap().len(), 4);
    assert_eq!(capture["caught_up"].as_array().unwrap().len(), 1);

    let text = repo.me(&["doctor"]).out;
    assert!(text.contains("capture mode: agents") && text.contains("recorded when found"), "{text}");
    assert!(repo.me(&["status"]).out.contains("capture:  agents"));
}

#[test]
fn full_mode_without_a_watcher_says_edits_are_caught_up() {
    let repo = Repo::new("");
    let status = repo.me(&["status"]).out;
    assert!(status.contains("capture:  full"), "{status}");
    assert!(status.contains("daemon:   stopped (edits are caught up"), "{status}");
    assert_eq!(repo.me(&["status", "--json"]).json()["capture"]["mode"], "full");
}
