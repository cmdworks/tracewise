//! `tracewise init` sets up a project in any folder, with or without git.
mod common;
use std::path::Path;
use std::process::Command;

fn tw(dir: &Path, home: &Path, args: &[&str]) -> common::Run {
    let out = Command::new(env!("CARGO_BIN_EXE_tracewise"))
        .args(args)
        .current_dir(dir)
        .env("TRACEWISE_HOME", home)
        .env("HOME", home)
        .env("TRACEWISE_ACTOR", "human:me")
        .output()
        .unwrap();
    common::Run {
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn init_works_in_a_folder_without_git() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let run = tw(dir.path(), home.path(), &["init"]);
    assert!(run.ok(), "{}", run.err);
    let id = tracewise_git::read_marker(dir.path()).expect("marker");
    assert!(home.path().join("projects").join(&id).join("tracewise.db").is_file(), "{id}");
    assert!(run.out.contains("twrecover1:"), "{}", run.out);
}

#[test]
fn init_from_a_subfolder_uses_the_git_root() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let sub = dir.path().join("app/src");
    std::fs::create_dir_all(&sub).unwrap();
    let run = tw(&sub, home.path(), &["init"]);
    assert!(run.ok(), "{}", run.err);
    assert!(dir.path().join(".tracewiseproject").is_file());
    assert!(!sub.join(".tracewiseproject").exists());
}

#[test]
fn init_twice_prints_the_recovery_key_only_once() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    assert!(tw(dir.path(), home.path(), &["init"]).out.contains("twrecover1:"));
    let again = tw(dir.path(), home.path(), &["init"]);
    assert!(again.ok(), "{}", again.err);
    assert!(!again.out.contains("twrecover1:"));
}

#[test]
fn log_in_a_plain_folder_says_to_run_init() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let run = tw(dir.path(), home.path(), &["log"]);
    assert_eq!(run.code, 2);
    assert!(run.err.contains("tracewise init"), "{}", run.err);
}

#[test]
fn history_survives_git_init() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    assert!(tw(dir.path(), home.path(), &["init"]).ok());
    std::fs::write(dir.path().join("note.txt"), "hello").unwrap();
    assert!(tw(dir.path(), home.path(), &["scan"]).ok());
    assert!(tw(dir.path(), home.path(), &["log"]).out.contains("note.txt"));
    assert!(Command::new("git").args(["init", "-q"]).current_dir(dir.path()).status().unwrap().success());
    let log = tw(dir.path(), home.path(), &["log"]);
    assert!(log.out.contains("note.txt"), "{}{}", log.out, log.err);
}

#[test]
fn init_installs_user_hooks_for_installed_agents_only() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    let stale = dir.path().join(".claude/settings.json");
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
    std::fs::write(&stale, r#"{"theme":"dark","hooks":{"Stop":[{"matcher":".*","hooks":[{"type":"command","command":"tracewise hook Stop --from claude"}]}]}}"#).unwrap();
    let run = tw(dir.path(), home.path(), &["init"]);
    assert!(run.ok(), "{}", run.err);
    assert!(std::fs::read_to_string(home.path().join(".claude/settings.json"))
        .unwrap()
        .contains("tracewise hook PreToolUse --from claude"));
    assert!(home.path().join(".claude/skills/tracewise/SKILL.md").is_file());
    assert!(!home.path().join(".codex").exists());
    assert!(run.out.contains("settings.json"), "{}", run.out);
    let left = std::fs::read_to_string(&stale).unwrap();
    assert!(!left.contains("tracewise hook") && left.contains("dark"), "{left}");
}

#[test]
fn hook_outside_a_project_allows() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut child = Command::new(env!("CARGO_BIN_EXE_tracewise"))
        .args(["hook", "PreToolUse", "--from", "claude"])
        .current_dir(dir.path())
        .env("TRACEWISE_HOME", home.path())
        .env("HOME", home.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let payload = r#"{"hook_event_name":"PreToolUse","session_id":"s","tool_name":"Edit","tool_input":{"file_path":"a.txt"}}"#;
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), payload.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("deny"));
}

/// git with no user config from the real home.
fn git(dir: &Path, home: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("TRACEWISE_GIT_DEPTH", "1")
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

fn copy_dir(from: &Path, to: &Path) {
    assert!(Command::new("cp").arg("-R").arg(from).arg(to).status().unwrap().success());
}

#[test]
fn init_in_a_linked_worktree_uses_the_main_checkouts_project() {
    let (t, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let main = t.path().join("main");
    std::fs::create_dir(&main).unwrap();
    git(&main, home.path(), &["init", "-q"]);
    std::fs::write(main.join("a.txt"), "a").unwrap();
    git(&main, home.path(), &["add", "a.txt"]);
    git(&main, home.path(), &["commit", "-q", "-m", "one"]);
    let linked = t.path().join("linked");
    git(&main, home.path(), &["worktree", "add", "-q", linked.to_str().unwrap()]);
    let run = tw(&linked, home.path(), &["init"]);
    assert!(run.ok(), "{}", run.err);
    assert!(!linked.join(".tracewiseproject").exists(), "no marker in the linked worktree");
    let id = tracewise_git::read_marker(&main).expect("the marker is in the main checkout");
    assert_eq!(std::fs::read_dir(home.path().join("projects")).unwrap().count(), 1, "one project");
    for dir in [&linked, &main] {
        let status = tw(dir, home.path(), &["status", "--json"]);
        assert!(!status.err.contains("copy"), "{}: {}", dir.display(), status.err);
        assert!(status.out.contains(&id), "{}", status.out);
    }
}

#[test]
fn init_in_a_copy_refuses_and_points_at_adopt_copy() {
    let (t, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = t.path().join("a");
    std::fs::create_dir(&a).unwrap();
    assert!(tw(&a, home.path(), &["init"]).ok());
    let b = t.path().join("b");
    copy_dir(&a, &b);
    let run = tw(&b, home.path(), &["init"]);
    assert_eq!(run.code, 1, "{}", run.err);
    assert!(run.err.contains("adopt-copy"), "{}", run.err);
}

/// Runs `tracewise hook <event> --from claude` in `dir` with `payload`; returns stdout.
fn claude_hook(dir: &Path, home: &Path, event: &str, payload: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tracewise"))
        .args(["hook", event, "--from", "claude"])
        .current_dir(dir)
        .env("TRACEWISE_HOME", home)
        .env("HOME", home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), payload.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A Write of `file` in `dir`, after the edit.
fn post_write(dir: &Path, home: &Path, file: &str) {
    std::fs::write(dir.join(file), file).unwrap();
    let payload = format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"s","tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        dir.join(file).display()
    );
    claude_hook(dir, home, "PostToolUse", &payload);
}

fn changes_of(home: &Path, id: &str) -> Vec<String> {
    let db = tracewise_core::db::Database::open(&home.join("projects").join(id).join("tracewise.db")).unwrap();
    let _ = db.import_spool(&home.join("projects").join(id));
    db.list_changes(None, None).unwrap().into_iter().map(|c| c.path).collect()
}

#[test]
fn an_agent_hook_in_an_unadopted_copy_records_nothing() {
    let (t, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = t.path().join("a");
    std::fs::create_dir(&a).unwrap();
    assert!(tw(&a, home.path(), &["init"]).ok());
    let id = tracewise_git::read_marker(&a).unwrap();
    let b = t.path().join("b");
    copy_dir(&a, &b);
    post_write(&b, home.path(), "only-in-b.txt");
    // the control: the same hook in the original records
    post_write(&a, home.path(), "in-a.txt");
    let changes = changes_of(home.path(), &id);
    assert!(changes.contains(&"in-a.txt".to_string()), "{changes:?}");
    assert!(!changes.contains(&"only-in-b.txt".to_string()), "the copy's edit went into the original's history: {changes:?}");
}

#[test]
fn an_agent_hook_in_a_moved_project_still_records() {
    let (t, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = t.path().join("a");
    std::fs::create_dir(&a).unwrap();
    assert!(tw(&a, home.path(), &["init"]).ok());
    let id = tracewise_git::read_marker(&a).unwrap();
    let b = t.path().join("b");
    std::fs::rename(&a, &b).unwrap();
    post_write(&b, home.path(), "after-move.txt");
    let changes = changes_of(home.path(), &id);
    assert!(changes.contains(&"after-move.txt".to_string()), "{changes:?}");
}

#[test]
fn protected_files_stay_refused_in_an_unadopted_copy() {
    let (t, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = t.path().join("a");
    std::fs::create_dir(&a).unwrap();
    std::fs::write(a.join(".tracewiseconfig"), "[profile]\npolicy = true\n").unwrap();
    assert!(tw(&a, home.path(), &["init"]).ok());
    let b = t.path().join("b");
    copy_dir(&a, &b);
    let edit = |path: String| {
        let payload =
            format!(r#"{{"hook_event_name":"PreToolUse","session_id":"s","tool_name":"Edit","tool_input":{{"file_path":"{path}"}}}}"#);
        claude_hook(&b, home.path(), "PreToolUse", &payload)
    };
    for p in [
        b.join(".tracewiseproject").display().to_string(),
        b.join(".tracewiseproject.tmp").display().to_string(),
        home.path().join(".claude/settings.json").display().to_string(),
    ] {
        assert!(edit(p.clone()).contains("deny"), "{p}");
    }
    assert!(!edit(b.join("src.txt").display().to_string()).contains("deny"));
    let cmd =
        r#"{"hook_event_name":"PreToolUse","session_id":"s","tool_name":"Bash","tool_input":{"command":"echo x > .tracewiseproject"}}"#;
    assert!(claude_hook(&b, home.path(), "PreToolUse", cmd).contains("deny"));
}

#[test]
fn init_tells_agents_without_a_skill_through_agents_md() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    assert!(tw(dir.path(), home.path(), &["init"]).ok());
    assert!(std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap().contains("tracewise:start"));
}

#[test]
fn init_gives_other_agents_project_hooks_and_claude_user_hooks() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for d in [".claude", ".codex", ".cursor"] {
        std::fs::create_dir(home.path().join(d)).unwrap();
    }
    let run = tw(dir.path(), home.path(), &["init"]);
    assert!(run.ok(), "{}", run.err);
    assert!(std::fs::read_to_string(home.path().join(".claude/settings.json")).unwrap().contains("--from claude"));
    for (agent, cfg) in [("codex", ".codex/hooks.json"), ("cursor", ".cursor/hooks.json")] {
        assert!(std::fs::read_to_string(dir.path().join(cfg)).unwrap().contains(&format!("--from {agent}")), "{cfg}");
        assert!(!home.path().join(cfg).exists(), "{cfg} is not written for the user");
    }
    assert!(!dir.path().join(".claude/settings.json").exists());
}

#[test]
fn init_refuses_the_home_folder() {
    let home = tempfile::tempdir().unwrap();
    let run = tw(home.path(), home.path(), &["init"]);
    assert_eq!(run.code, 1, "{}", run.err);
    assert!(run.err.contains("home folder"), "{}", run.err);
    assert!(!home.path().join(".tracewiseproject").exists());
}

#[test]
fn init_json_has_the_recovery_secret_and_id_only_the_first_time() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let first = tw(dir.path(), home.path(), &["init", "--json"]).json();
    assert!(first["recovery_secret"].as_str().unwrap().starts_with("twrecover1:"));
    assert_eq!(first["id"].as_str(), tracewise_git::read_marker(dir.path()).as_deref());
    let second = tw(dir.path(), home.path(), &["init", "--json"]).json();
    assert!(second.get("recovery_secret").is_none(), "{second}");
}

#[test]
fn the_recovery_secret_is_printed_before_the_baseline() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let out = tw(dir.path(), home.path(), &["init"]).out;
    let (secret, done) = (out.find("twrecover1:").unwrap(), out.find("Initialized").unwrap());
    assert!(secret < done, "{out}");
}

#[test]
fn init_in_a_fresh_clone_reuses_the_marker() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let id = format!("tw1:{}", "ab".repeat(13));
    tracewise_git::write_marker(dir.path(), &id).unwrap();
    let run = tw(dir.path(), home.path(), &["init"]);
    assert!(run.ok(), "{}", run.err);
    assert!(!run.out.contains("twrecover1:"));
    assert!(home.path().join("projects").join(&id).join("tracewise.db").is_file());
}

#[test]
fn init_with_dash_c_sets_up_that_folder() {
    let (dir, home, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let run = tw(elsewhere.path(), home.path(), &["-C", dir.path().to_str().unwrap(), "init"]);
    assert!(run.ok(), "{}", run.err);
    assert!(tracewise_git::read_marker(dir.path()).is_some());
    assert!(tracewise_git::read_marker(elsewhere.path()).is_none());
}

#[test]
fn init_leaves_an_unreadable_user_settings_file_alone() {
    let (dir, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join(".claude/settings.json"), "{ not json").unwrap();
    let run = tw(dir.path(), home.path(), &["init"]);
    assert!(run.ok(), "{}", run.err);
    assert_eq!(std::fs::read_to_string(home.path().join(".claude/settings.json")).unwrap(), "{ not json");
}

#[test]
fn adopt_copy_json_names_the_new_project() {
    let (t, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = t.path().join("a");
    std::fs::create_dir(&a).unwrap();
    assert!(tw(&a, home.path(), &["init"]).ok());
    let b = t.path().join("b");
    copy_dir(&a, &b);
    let run = tw(&b, home.path(), &["doctor", "--adopt-copy", "--json"]);
    assert!(run.ok(), "{}", run.err);
    let doc = run.json();
    assert_eq!(doc["id"].as_str(), tracewise_git::read_marker(&b).as_deref());
    assert_ne!(tracewise_git::read_marker(&a), tracewise_git::read_marker(&b));
}

#[test]
fn adopt_copy_of_a_v1_copy_that_has_not_moved_yet() {
    let (t, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = t.path().join("a");
    std::fs::create_dir_all(a.join(".git/tracewise")).unwrap();
    drop(tracewise_core::db::Database::open(&a.join(".git/tracewise/tracewise.db")).unwrap());
    tracewise_core::identity::repo_id(&a.join(".git/tracewise"), &a);
    let b = t.path().join("b");
    copy_dir(&a, &b);
    let run = tw(&b, home.path(), &["doctor", "--adopt-copy"]);
    assert!(run.ok(), "{}{}", run.out, run.err);
    assert!(a.join(".git/tracewise/tracewise.db").exists() && !a.join(".git/tracewise/MOVED").exists(), "the original is untouched");
    assert!(tracewise_git::read_marker(&b).is_some());
    assert!(tracewise_git::read_marker(&a).is_none());
    let status = tw(&b, home.path(), &["status"]);
    assert!(!status.err.contains("copy"), "{}", status.err);
}
