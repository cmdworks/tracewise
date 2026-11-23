use super::*;
use tempfile::tempdir;

fn repo(tmp: &std::path::Path) -> RepoPaths {
    let g = tmp.join(".git");
    fs::create_dir_all(g.join("tracewise")).unwrap();
    RepoPaths { work_tree: tmp.to_path_buf(), git_dir: g.clone(), git_common_dir: g }
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[test]
fn a_gap_is_recorded_only_when_another_daemon_took_over_after_a_long_silence() {
    let prev = Heartbeat { pid: 10, at: "2026-11-06T11:02:00Z".into() };
    let g = gap(&prev, 11, at("2026-11-06T11:04:10Z"), 90).expect("a new pid and 130 s");
    assert_eq!(g.0, "2026-11-06T11:02:00Z");
    assert!(gap(&prev, 11, at("2026-11-06T11:03:00Z"), 90).is_none(), "60 s is an ordinary pause");
    assert!(gap(&prev, 10, at("2026-11-06T18:00:00Z"), 90).is_none(), "the same daemon after a night asleep");
    assert!(gap(&Heartbeat { pid: 1, at: "garbage".into() }, 2, at("2026-11-06T18:00:00Z"), 90).is_none());
}

#[test]
fn the_wanted_list_follows_install_and_uninstall_and_ignores_unknown_agents() {
    let tmp = tempdir().unwrap();
    let r = repo(tmp.path());
    set_wanted(&r, "claude", true);
    set_wanted(&r, "claude", true);
    set_wanted(&r, "codex", true);
    set_wanted(&r, "someone-else", true);
    assert_eq!(wanted(&r), vec!["claude", "codex"]);
    set_wanted(&r, "claude", false);
    assert_eq!(wanted(&r), vec!["codex"]);
}

#[test]
fn removed_hooks_come_back_once_and_only_while_they_are_wanted() {
    let tmp = tempdir().unwrap();
    let r = repo(tmp.path());
    let db = Database::open(&r.data_dir().join("tracewise.db")).unwrap();
    let cfg = tmp.path().join(".claude/settings.json");

    crate::cmd_agents::handle_agent_install(&r, "claude", "project", &tmp.path().join("nohome"), false).unwrap();
    assert_eq!(wanted(&r), vec!["claude"]);
    assert!(repair_hooks(&r, &db, &tmp.path().join("nohome")).is_empty(), "nothing is missing");

    // someone deletes the hooks
    fs::write(&cfg, "{}").unwrap();
    assert_eq!(repair_hooks(&r, &db, &tmp.path().join("nohome")), vec!["claude"]);
    assert!(fs::read_to_string(&cfg).unwrap().contains("tracewise hook PreToolUse --from claude"));
    assert!(repair_hooks(&r, &db, &tmp.path().join("nohome")).is_empty(), "a second sweep finds them back");
    let repairs: Vec<_> = db.list_commands().unwrap().into_iter().filter(|c| c.kind == "repair").collect();
    assert_eq!(repairs.len(), 1, "one record for one repair");

    // a person who uninstalls on purpose is not fought
    crate::cmd_agents::handle_agent_uninstall(&r, "claude", "project", &tmp.path().join("nohome"), false).unwrap();
    assert!(wanted(&r).is_empty());
    assert!(repair_hooks(&r, &db, &tmp.path().join("nohome")).is_empty());
    assert!(!fs::read_to_string(&cfg).unwrap().contains("tracewise hook"));
}

#[test]
fn heartbeat_records_a_restart_and_writes_the_new_beat() {
    let tmp = tempdir().unwrap();
    let r = repo(tmp.path());
    let db = Database::open(&r.data_dir().join("tracewise.db")).unwrap();
    let old = Heartbeat { pid: 10, at: "2020-01-01T00:00:00Z".into() };
    fs::write(r.data_dir().join("heartbeat.json"), serde_json::to_string(&old).unwrap()).unwrap();
    heartbeat(&r, &db, 11);
    heartbeat(&r, &db, 11); // the same daemon beats again: no second record
    let gaps: Vec<_> = db.list_commands().unwrap().into_iter().filter(|c| c.kind == "gap").collect();
    assert_eq!(gaps.len(), 1);
    assert!(gaps[0].text.contains("2020-01-01T00:00:00Z"));
    let beat: Heartbeat = serde_json::from_str(&fs::read_to_string(r.data_dir().join("heartbeat.json")).unwrap()).unwrap();
    assert_eq!(beat.pid, 11);
}

#[test]
fn hooks_installed_for_the_user_are_not_repaired_into_the_project() {
    let (tmp, home) = (tempdir().unwrap(), tempdir().unwrap());
    let r = repo(tmp.path());
    let db = Database::open(&r.data_dir().join("tracewise.db")).unwrap();
    set_wanted(&r, "claude", true);
    crate::cmd_agents::install_agent_quiet(&r, "claude", crate::cmd_agents::AgentScope::User, home.path()).unwrap();
    for _ in 0..2 {
        assert!(repair_hooks(&r, &db, home.path()).is_empty());
    }
    assert!(db.list_commands().unwrap().iter().all(|c| c.kind != "repair"), "no repair note while the user's hooks are there");
    assert!(!tmp.path().join(".claude/settings.json").exists());
}

#[test]
fn installing_for_the_user_stops_the_project_repair() {
    let (tmp, home) = (tempdir().unwrap(), tempdir().unwrap());
    let r = repo(tmp.path());
    set_wanted(&r, "claude", true);
    crate::cmd_agents::handle_agent_install(&r, "claude", "user", home.path(), false).unwrap();
    assert!(wanted(&r).is_empty());
}
