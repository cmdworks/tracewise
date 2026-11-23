//! `tracewise record`: an editor says "this file was saved". Only the named files are looked at.
mod common;
use common::Repo;

fn record(repo: &Repo, args: &[&str]) -> common::Run {
    let mut full = vec!["record", "--json"];
    full.extend_from_slice(args);
    repo.me(&full)
}

fn changes_of(run: &common::Run) -> Vec<String> {
    run.json()["changes"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
}

#[test]
fn a_saved_file_is_recorded_with_the_editor_source() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    let run = record(&repo, &["a.txt"]);
    assert!(run.ok(), "{}", run.err);
    assert_eq!(changes_of(&run).len(), 1);
    let row = repo.rows(&[]).into_iter().find(|r| r["path"] == "a.txt").unwrap();
    assert_eq!(row["source"], "editor");
    assert_eq!((row["actor"]["kind"].as_str(), row["confidence"].as_str()), (Some("human"), Some("likely")), "{row}");
}

/// How many changes `verify` counts: unlike `log` it does not catch up on files nothing recorded.
fn recorded(repo: &Repo) -> usize {
    let run = repo.me(&["doctor", "--verify"]);
    run.out
        .split(|c: char| !c.is_ascii_digit())
        .find(|n| !n.is_empty())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{}", run.out))
}

#[test]
fn only_the_named_files_are_looked_at() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    repo.write("b.txt", "two\n");
    let before = recorded(&repo);
    assert_eq!(changes_of(&record(&repo, &["a.txt"])).len(), 1);
    assert_eq!(recorded(&repo), before + 1, "b.txt was not looked at");
}

#[test]
fn absolute_and_relative_paths_both_work() {
    let repo = Repo::new("");
    repo.write("src/a.txt", "one\n");
    repo.write("src/b.txt", "two\n");
    let absolute = repo.dir().join("src/a.txt");
    assert_eq!(changes_of(&record(&repo, &[absolute.to_str().unwrap()])).len(), 1);
    assert_eq!(changes_of(&record(&repo, &["src/b.txt"])).len(), 1);
}

#[test]
fn a_second_record_of_an_unchanged_file_records_nothing() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    assert_eq!(changes_of(&record(&repo, &["a.txt"])).len(), 1);
    assert_eq!(changes_of(&record(&repo, &["a.txt"])).len(), 0);
}

#[test]
fn a_path_outside_the_project_or_an_ignored_one_records_nothing_and_succeeds() {
    let repo = Repo::new("");
    repo.write(".gitignore", "*.log\n");
    repo.write("debug.log", "noise\n");
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("x.txt"), "x\n").unwrap();
    let run = record(&repo, &["debug.log", outside.path().join("x.txt").to_str().unwrap(), "../escape.txt"]);
    assert!(run.ok(), "{}", run.err);
    assert!(changes_of(&run).is_empty());
}

#[test]
fn a_deleted_tracked_file_is_recorded_as_a_delete() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    assert_eq!(changes_of(&record(&repo, &["a.txt"])).len(), 1);
    std::fs::remove_file(repo.dir().join("a.txt")).unwrap();
    assert_eq!(changes_of(&record(&repo, &["a.txt"])).len(), 1);
    let rows = repo.rows(&[]);
    assert!(rows.iter().any(|r| r["path"] == "a.txt" && r["op"] == "delete"), "{rows:?}");
}

#[test]
fn record_in_a_folder_that_is_not_a_project_exits_2() {
    let plain = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tracewise"))
        .args(["record", "a.txt"])
        .current_dir(plain.path())
        .env("TRACEWISE_HOME", home.path())
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn rec_is_the_short_name_and_source_can_be_named() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    let run = repo.me(&["rec", "--source", "zed", "a.txt", "--json"]);
    assert!(run.ok(), "{}", run.err);
    let row = repo.rows(&[]).into_iter().find(|r| r["path"] == "a.txt").unwrap();
    assert_eq!(row["source"], "zed");
    assert!(!repo.me(&["record", "-s", "x", "a.txt"]).ok(), "-s is --since elsewhere: not accepted here");
}

#[test]
fn an_open_agent_window_makes_the_save_ambiguous_not_the_persons() {
    let repo = Repo::new("");
    let body = serde_json::json!({"agent": "claude", "session": "s1", "paths": ["a.txt"]}).to_string();
    assert!(repo.with("agent:claude:s1", &["hook", "pre-edit"], Some(&body), &[]).ok());
    repo.write("a.txt", "agent or person\n");
    // an editor sets no TRACEWISE_ACTOR
    assert!(repo.as_program("tracewise", &["record", "--json", "a.txt"], &[]).ok());
    let row = repo.rows(&[]).into_iter().find(|r| r["path"] == "a.txt").unwrap();
    assert_eq!(row["actor"]["kind"], "ambiguous", "an AI extension's edit also arrives as a save: say so, do not pick: {row}");
    let kinds: Vec<&str> = row["candidates"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"agent") && kinds.contains(&"human"), "{kinds:?}");
}

#[test]
fn a_source_that_belongs_to_the_sensor_the_watcher_or_the_hooks_is_refused() {
    // `sensor` makes a scan believe the actor exactly, whatever windows say: it must not be reachable from the command line
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    let before = recorded(&repo);
    for reserved in ["sensor", "watch", "hook", "scan", "catch-up", "background", "git", "shell", "process"] {
        let run = repo.me(&["record", "--source", reserved, "a.txt", "--json"]);
        assert!(!run.ok(), "--source {reserved} was accepted");
    }
    assert_eq!(recorded(&repo), before, "nothing was recorded by the refused calls");
    for editor in ["editor", "lsp", "vscode", "zed", "jetbrains"] {
        repo.write("b.txt", &format!("by {editor}\n"));
        assert!(repo.me(&["record", "--source", editor, "b.txt", "--json"]).ok(), "{editor}");
    }
}

#[test]
fn record_reports_how_big_each_change_is() {
    let repo = Repo::new("");
    repo.write("a.txt", "let value = compute(a, b);\nline two\n");
    assert!(record(&repo, &["a.txt"]).ok());
    repo.write("a.txt", "let value = compute(a, c);\nline two\n");
    let run = record(&repo, &["a.txt"]);
    let stats = &run.json()["stats"][0]["stats"];
    assert_eq!((stats["added_chars"].as_u64(), stats["removed_chars"].as_u64()), (Some(1), Some(1)), "{stats}");
    assert_eq!(stats["whitespace_only"], false);
    repo.write("a.txt", "let value = compute(a, c);\n   line two\n");
    let ws = record(&repo, &["a.txt"]);
    assert_eq!(ws.json()["stats"][0]["stats"]["whitespace_only"], true);
    // a new file is a create: its sizes are there, and an editor never treats it as minor
    repo.write("b.txt", "x\n");
    assert_eq!(record(&repo, &["b.txt"]).json()["stats"][0]["op"], "create");
}

#[test]
fn an_automatic_reason_fills_a_gap_and_never_replaces_a_persons() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    let id = changes_of(&record(&repo, &["a.txt"])).remove(0);
    assert!(repo.me(&["note", &id, "skipped", "--auto"]).ok());
    let row = |repo: &Repo| repo.rows(&[]).into_iter().find(|r| r["id"] == id.as_str()).unwrap();
    assert!(row(&repo)["intent"].is_string(), "the automatic reason is attached");
    let why = |repo: &Repo| repo.me(&["why", "a.txt:1"]).out;
    assert!(why(&repo).contains("skipped"), "{}", why(&repo));
    assert!(repo.me(&["note", &id, "fixing the parser", "--auto"]).ok());
    assert!(why(&repo).contains("skipped") && !why(&repo).contains("fixing the parser"), "an automatic reason does not replace one");
    assert!(repo.me(&["note", &id, "fixing the parser"]).ok(), "a person's note replaces an automatic one");
    assert!(why(&repo).contains("fixing the parser"), "{}", why(&repo));
    // and an automatic reason never replaces a person's
    assert!(repo.me(&["note", &id, "ignored", "--auto"]).ok());
    assert!(why(&repo).contains("fixing the parser"), "{}", why(&repo));
}

#[test]
fn log_shows_a_run_of_old_minor_edits_as_one_line_and_unfold_shows_each() {
    let repo = Repo::new("");
    repo.write("a.txt", "start\n");
    let mut ids = changes_of(&record(&repo, &["a.txt"]));
    for i in 1..=4 {
        repo.write("a.txt", &format!("start{}\n", "x".repeat(i)));
        ids.extend(changes_of(&record(&repo, &["a.txt"])));
    }
    // the four edits after the creation are minor: an automatic reason each
    for id in &ids[1..] {
        assert!(repo.me(&["note", id, "minor edit (1 character)", "--auto"]).ok());
    }
    let lines = |args: &[&str]| -> Vec<String> {
        let mut full = vec!["log"];
        full.extend_from_slice(args);
        repo.me(&full).out.lines().map(String::from).collect()
    };
    let folded = lines(&["--fold-after", "0"]);
    assert_eq!(folded.len(), 2, "the creation and one fold line: {folded:?}");
    assert!(folded.iter().any(|l| l.contains(" fold ") && l.contains("4 minor edits")), "{folded:?}");
    assert_eq!(lines(&["--fold-after", "0", "--unfold"]).len(), 5, "unfold shows every row");
    assert_eq!(lines(&[]).len(), 5, "recent changes are not folded by default (1 day)");
    let json = repo.me(&["log", "--json", "--fold-after", "0"]).json();
    assert_eq!(json.as_array().unwrap().len(), 5, "JSON always has every row");
    // a person's reason in the middle splits the run
    assert!(repo.me(&["note", &ids[2], "renamed the variable to make it clearer"]).ok());
    let split = lines(&["--fold-after", "0"]);
    assert!(split.iter().all(|l| !l.contains("4 minor edits")), "{split:?}");
}

#[test]
fn a_short_label_is_accepted_only_with_the_label_flag() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    let id = changes_of(&record(&repo, &["a.txt"])).remove(0);
    assert!(!repo.me(&["note", &id, "Bug fix"]).ok(), "a typed reason must be long enough");
    assert!(repo.me(&["note", &id, "Bug fix", "--label"]).ok(), "a label from a list may be short");
    assert!(repo.me(&["why", "a.txt:1"]).out.contains("Bug fix"));
}

#[test]
fn log_says_whether_a_reason_is_automatic() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    let id = changes_of(&record(&repo, &["a.txt"])).remove(0);
    let row = |repo: &Repo| repo.me(&["log", "--json"]).json().as_array().unwrap().iter().find(|r| r["id"] == id.as_str()).cloned().unwrap();
    assert_eq!(row(&repo)["why_auto"], false, "no reason at all");
    assert!(repo.me(&["note", &id, "whitespace", "--auto"]).ok());
    assert_eq!(row(&repo)["why_auto"], true);
    assert!(repo.me(&["note", &id, "renamed the variable to make it clearer"]).ok());
    assert_eq!(row(&repo)["why_auto"], false, "a person's words replace it");
}

#[test]
fn log_and_blame_say_which_editor_a_change_came_from() {
    let repo = Repo::new("");
    repo.write("a.txt", "one\n");
    assert!(repo.me(&["record", "--source", "zed", "a.txt"]).ok());
    let log = repo.me(&["log"]).out;
    assert!(log.contains("via Zed"), "{log}");
    let blame = repo.me(&["blame", "a.txt", "--json"]).json();
    assert_eq!(blame[0]["source"], "zed");
    // an agent's or a script's change says nothing about an editor
    repo.write("b.txt", "x\n");
    assert!(repo.run("agent:claude:s1", &["scan"]).ok());
    assert!(!repo.me(&["log", "b.txt"]).out.contains(" via "));
}

#[test]
fn completions_are_printed_for_the_shells_and_status_names_the_preset() {
    let repo = Repo::new("");
    for shell in ["bash", "zsh", "fish"] {
        let out = repo.me(&["completions", shell]);
        assert!(out.ok() && out.out.contains("tracewise"), "{shell}: {}", out.err);
    }
    assert!(!repo.me(&["completions", "csh"]).ok(), "an unknown shell is refused");
    let status = repo.me(&["status", "--json"]).json();
    assert!(status["capture"]["mode"].is_string());
    assert!(status["capture"].get("preset").is_some(), "{status}");
    assert!(repo.me(&["status"]).out.contains("preset:"));
}
