use super::*;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_agent_install_uninstall_and_idempotence() {
    let tmp = tempdir().unwrap();
    let work_tree = tmp.path().to_path_buf();
    let git_dir = work_tree.join(".git");
    fs::create_dir_all(&git_dir).unwrap();

    let repo_paths = RepoPaths { work_tree: work_tree.clone(), git_dir: git_dir.clone(), git_common_dir: git_dir.clone() };

    // Existing settings with user permissions and custom hook
    let claude_cfg = work_tree.join(".claude/settings.json");
    fs::create_dir_all(claude_cfg.parent().unwrap()).unwrap();
    let initial_settings = serde_json::json!({
        "permissions": {
            "allow": ["Bash(ls:*)"]
        },
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "echo mine"}]
                }
            ]
        }
    });
    fs::write(&claude_cfg, serde_json::to_string(&initial_settings).unwrap()).unwrap();

    let agents_md = work_tree.join("AGENTS.md");
    fs::write(&agents_md, "# My Project Rules\n\nKeep this line.\n").unwrap();

    // 1. Install claude
    handle_agent_install(&repo_paths, "claude", "project", &work_tree.join("nohome"), false).unwrap();

    let installed_settings = fs::read_to_string(&claude_cfg).unwrap();
    assert!(installed_settings.contains("echo mine"));
    assert!(installed_settings.contains("tracewise hook"));
    assert!(installed_settings.contains("--from claude"));

    let skill_file = work_tree.join(".claude/skills/tracewise/SKILL.md");
    assert!(skill_file.exists());
    let skill_text = fs::read_to_string(&skill_file).unwrap();
    assert!(skill_text.contains("tracewise note --why"));
    assert!(skill_text.contains("tracewise run --why"));
    assert!(skill_text.contains("tracewise pending"));
    assert!(skill_text.contains("tracewise note"));

    let agents_text = fs::read_to_string(&agents_md).unwrap();
    assert!(agents_text.contains("Keep this line."));
    assert!(agents_text.contains("<!-- tracewise:start -->"));
    assert!(agents_text.contains("<!-- tracewise:end -->"));
    assert!(agents_text.contains("tracewise note --why"));

    // 2. Install again (idempotent)
    let before_cfg = fs::read_to_string(&claude_cfg).unwrap();
    let before_agents = fs::read_to_string(&agents_md).unwrap();
    handle_agent_install(&repo_paths, "claude", "project", &work_tree.join("nohome"), false).unwrap();
    let after_cfg = fs::read_to_string(&claude_cfg).unwrap();
    let after_agents = fs::read_to_string(&agents_md).unwrap();
    assert_eq!(before_cfg, after_cfg);
    assert_eq!(before_agents, after_agents);

    // 3. Uninstall claude
    handle_agent_uninstall(&repo_paths, "claude", "project", &work_tree.join("nohome"), false).unwrap();
    let uninstalled_settings = fs::read_to_string(&claude_cfg).unwrap();
    assert!(uninstalled_settings.contains("echo mine"));
    assert!(!uninstalled_settings.contains("tracewise hook"));
    assert!(!skill_file.exists());

    let uninstalled_agents = fs::read_to_string(&agents_md).unwrap();
    assert!(uninstalled_agents.contains("Keep this line."));
    assert!(!uninstalled_agents.contains("<!-- tracewise:start -->"));
}

#[test]
fn a_config_missing_hook_events_is_reported() {
    let full = "tracewise hook PreToolUse --from cursor tracewise hook PostToolUse --from cursor tracewise hook Stop --from cursor tracewise hook SessionEnd --from cursor";
    assert!(super::missing_events("cursor", full).is_empty());
    let partial = "tracewise hook PreToolUse --from cursor tracewise hook Stop --from cursor";
    assert_eq!(super::missing_events("cursor", partial), ["PostToolUse", "SessionEnd"]);
    // the same text is for another agent: nothing counts
    assert_eq!(super::missing_events("codex", partial).len(), 4);
    // claude has two more events, antigravity one fewer
    let claude = full.replace("cursor", "claude");
    assert_eq!(super::missing_events("claude", &claude), ["UserPromptSubmit", "PostToolUseFailure"]);
    assert!(super::missing_events(
        "antigravity",
        "tracewise hook PreToolUse --from antigravity tracewise hook PostToolUse --from antigravity tracewise hook Stop --from antigravity"
    )
    .is_empty());
}

#[test]
fn claude_install_adds_deny_rules_for_tracewises_own_files_and_keeps_the_persons_own() {
    let tmp = tempdir().unwrap();
    let work_tree = tmp.path().to_path_buf();
    let git_dir = work_tree.join(".git");
    fs::create_dir_all(&git_dir).unwrap();
    let repo_paths = RepoPaths { work_tree: work_tree.clone(), git_dir: git_dir.clone(), git_common_dir: git_dir };
    let cfg = work_tree.join(".claude/settings.json");
    fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    fs::write(&cfg, r#"{"permissions":{"deny":["Edit(secrets/**)"]}}"#).unwrap();

    handle_agent_install(&repo_paths, "claude", "project", &work_tree.join("nohome"), false).unwrap();
    handle_agent_install(&repo_paths, "claude", "project", &work_tree.join("nohome"), false).unwrap(); // twice: no duplicates
    let v: Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    let deny: Vec<&str> = v["permissions"]["deny"].as_array().unwrap().iter().filter_map(|d| d.as_str()).collect();
    assert_eq!(deny.iter().filter(|d| **d == "Edit(**/.git/tracewise/**)").count(), 1);
    for rule in CLAUDE_DENY {
        assert!(deny.contains(&rule), "{rule} missing");
    }
    assert!(deny.contains(&"Edit(secrets/**)"));

    handle_agent_uninstall(&repo_paths, "claude", "project", &work_tree.join("nohome"), false).unwrap();
    let v: Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    let deny: Vec<&str> = v["permissions"]["deny"].as_array().unwrap().iter().filter_map(|d| d.as_str()).collect();
    assert_eq!(deny, vec!["Edit(secrets/**)"]);
}

fn repo_in(dir: &Path) -> RepoPaths {
    let git = dir.join(".git");
    fs::create_dir_all(&git).unwrap();
    RepoPaths { work_tree: dir.to_path_buf(), git_dir: git.clone(), git_common_dir: git }
}

#[test]
fn skill_has_a_valid_header() {
    assert!(CLAUDE_SKILL_MD.starts_with("---\nname: tracewise\ndescription: "));
    let header = CLAUDE_SKILL_MD.split("---\n").nth(1).unwrap();
    assert_eq!(header.lines().count(), 2, "name and a one-line description");
}

#[test]
fn user_scope_install_writes_under_home_not_the_project() {
    let (proj, home) = (tempdir().unwrap(), tempdir().unwrap());
    let repo = repo_in(proj.path());
    let files = install_agent_quiet(&repo, "claude", AgentScope::User, home.path()).unwrap();
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(fs::read_to_string(home.path().join(".claude/settings.json")).unwrap().contains("tracewise hook PreToolUse --from claude"));
    assert!(fs::read_to_string(home.path().join(".claude/skills/tracewise/SKILL.md")).unwrap().starts_with("---\nname: tracewise"));
    assert!(!proj.path().join(".claude").exists());
    assert!(!proj.path().join("AGENTS.md").exists());
}

#[test]
fn user_scope_uninstall_removes_only_tracewise_entries() {
    let (proj, home) = (tempdir().unwrap(), tempdir().unwrap());
    let repo = repo_in(proj.path());
    let cfg = home.path().join(".claude/settings.json");
    fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    fs::write(&cfg, r#"{"theme":"dark","hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo mine"}]}]}}"#)
        .unwrap();
    install_agent_quiet(&repo, "claude", AgentScope::User, home.path()).unwrap();
    handle_agent_uninstall(&repo, "claude", "user", home.path(), false).unwrap();
    let v: Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    assert_eq!(v["theme"], "dark");
    assert_eq!(v["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    assert!(!fs::read_to_string(&cfg).unwrap().contains("tracewise hook"));
    assert!(!home.path().join(".claude/skills/tracewise/SKILL.md").exists());
}

#[test]
fn project_install_is_skipped_when_user_hooks_exist() {
    let (proj, home) = (tempdir().unwrap(), tempdir().unwrap());
    let repo = repo_in(proj.path());
    install_agent_quiet(&repo, "claude", AgentScope::User, home.path()).unwrap();
    let files = install_agent_quiet(&repo, "claude", AgentScope::Project, home.path()).unwrap();
    assert_eq!(files, vec!["AGENTS.md"]);
    assert!(!proj.path().join(".claude").exists(), "no second copy of the hooks");
}

#[test]
fn claude_is_told_never_to_edit_a_marker() {
    assert!(CLAUDE_DENY.contains(&"Edit(**/.tracewiseproject)") && CLAUDE_DENY.contains(&"Write(**/.tracewiseproject)"));
}

#[test]
fn user_scope_without_a_home_folder_is_an_error() {
    let proj = tempdir().unwrap();
    let repo = repo_in(proj.path());
    assert!(install_agent_quiet(&repo, "claude", AgentScope::User, Path::new("")).is_err());
    assert!(handle_agent_uninstall(&repo, "claude", "user", Path::new(""), false).is_err());
    assert!(!user_hooks_exist(Path::new(""), "claude", ".claude/settings.json"));
    assert!(install_for_init(&repo, Path::new("")).is_empty());
}

#[test]
fn user_scope_install_keeps_the_persons_settings() {
    let (proj, home) = (tempdir().unwrap(), tempdir().unwrap());
    let repo = repo_in(proj.path());
    let cfg = home.path().join(".claude/settings.json");
    fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    fs::write(&cfg, r#"{"model":"opus","permissions":{"deny":["Read(./.env)"],"allow":["Bash(ls)"]}}"#).unwrap();
    install_agent_quiet(&repo, "claude", AgentScope::User, home.path()).unwrap();
    let v: Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    assert_eq!(v["model"], "opus");
    assert_eq!(v["permissions"]["allow"][0], "Bash(ls)");
    assert!(v["permissions"]["deny"].as_array().unwrap().iter().any(|d| d == "Read(./.env)"));
}
