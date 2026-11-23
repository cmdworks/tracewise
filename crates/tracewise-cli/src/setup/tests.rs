//! Setup and doctor unit tests.
use super::*;
use std::fs;
use std::path::Path;
use tracewise_core::capture::CaptureMode;

pub(crate) fn fake_env() -> (tempfile::TempDir, Env) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let src = tmp.path().join("src");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&src).unwrap();
    for n in PROGRAMS {
        fs::write(src.join(n), format!("program {n}")).unwrap();
    }
    let env = Env { root: home.join(".tracewise"), home, src_dir: src, path: Some(std::ffi::OsString::new()) };
    (tmp, env)
}

fn opts(login: Login) -> SetupOptions {
    SetupOptions { login, start: false, shell_rc: true, terminal: true, live: false, profile: "core".to_string(), capture: None }
}

const OLD_RC: &str = "export FOO=1\n\n# Tracewise git wrapper and shell integration\nexport PATH=\"$HOME/.tracewise/bin:$PATH\"\neval \"$(tracewise shell init zsh)\"\nexport PATH=\"$HOME/.tracewise/bin:$HOME/.tracewise/shims:$PATH\"\nalias ll='ls -l'\n";

#[test]
fn old_lines_become_one_block_and_nothing_else_changes() {
    let rc = rewrite_zshrc(OLD_RC, "# >>> tracewise >>>\nB\n# <<< tracewise <<<\n");
    assert_eq!(rc.matches(".tracewise").count(), 0);
    assert!(rc.starts_with("export FOO=1\n"));
    assert!(rc.contains("alias ll='ls -l'\n"));
    assert_eq!(rc.matches(BLOCK_START).count(), 1);
    assert_eq!(rewrite_zshrc(&rc, "# >>> tracewise >>>\nB\n# <<< tracewise <<<\n"), rc, "twice is the same");
    assert_eq!(strip_tracewise(&rc), "export FOO=1\n\nalias ll='ls -l'\n");
}

#[test]
fn setup_installs_everything_and_uninstall_removes_exactly_that() {
    let (_tmp, env) = fake_env();
    fs::write(env.zshrc(), OLD_RC).unwrap();
    fs::create_dir_all(env.home.join(".cargo/bin")).unwrap();
    fs::write(env.home.join(".cargo/bin/tracewise"), "old copy").unwrap();
    let m = setup_in(&env, &opts(Login::Yes)).unwrap();
    assert!(env.home.join(".cargo/bin/tracewise").is_symlink(), "the old copy now links to the installed one");
    assert_eq!(m.programs.len(), PROGRAMS.len());
    for n in PROGRAMS {
        assert!(env.bin().join(n).is_file(), "{n}");
    }
    assert!(env.bin().join("git").is_symlink());
    for alias in ["tw", "trace", "tracwise"] {
        assert!(env.bin().join(alias).is_symlink(), "{alias} is another name of tracewise");
    }
    assert_eq!(fs::read_dir(env.shims()).unwrap().count(), crate::cmd_shims::SHIM_TOOLS.len());
    let rc = fs::read_to_string(env.zshrc()).unwrap();
    assert_eq!(rc.matches(BLOCK_START).count(), 1);
    assert!(m.zshrc_backup.is_some() && Path::new(m.zshrc_backup.as_ref().unwrap()).is_file());
    let zshenv = fs::read_to_string(env.zshenv()).unwrap();
    assert!(zshenv.contains(".tracewise/bin") && !zshenv.contains("shell init"), "PATH is in .zshenv: {zshenv}");
    assert!(rc.contains("shell init zsh") && !rc.contains("export PATH"), "only the hook is in .zshrc: {rc}");
    assert!(env.agent_plist().is_file());
    assert!(env.manifest().is_file());

    // again: nothing changes, no second backup
    let again = setup_in(&env, &opts(Login::Yes)).unwrap();
    assert_eq!(again.zshrc_backup, m.zshrc_backup);
    assert_eq!(fs::read_to_string(env.zshrc()).unwrap(), rc);
    assert_eq!(fs::read_to_string(env.zshenv()).unwrap(), zshenv);

    uninstall_in(&env, false, false, false);
    assert!(!env.home.join(".cargo/bin/tracewise").exists());
    assert!(!env.bin().exists() && !env.shims().exists() && !env.manifest().exists());
    assert!(!env.agent_plist().exists());
    assert_eq!(fs::read_to_string(env.zshrc()).unwrap(), "export FOO=1\n\nalias ll='ls -l'\n");
}

#[test]
fn login_no_leaves_no_login_item_and_a_reinstall_can_remove_it() {
    let (_tmp, env) = fake_env();
    setup_in(&env, &opts(Login::Yes)).unwrap();
    assert!(env.agent_plist().is_file());
    let m = setup_in(&env, &opts(Login::No)).unwrap();
    assert!(m.launch_agent.is_none() && !env.agent_plist().exists());
}

#[test]
fn doctor_names_what_is_missing_and_stray_lines() {
    let (_tmp, env) = fake_env();
    fs::write(env.zshrc(), OLD_RC).unwrap();
    let before = checks(&env, "/usr/bin");
    assert!(before.iter().any(|c| !c.ok && c.what.contains("install.json")));
    assert!(before.iter().any(|c| !c.ok && c.what.contains("old Tracewise line")));
    setup_in(&env, &opts(Login::No)).unwrap();
    let path = format!("{}:{}:/usr/bin", env.bin().display(), env.shims().display());
    let after = checks(&env, &path);
    let failing: Vec<_> = after.iter().filter(|c| !c.ok && c.what != "daemon running").map(|c| c.what.clone()).collect();
    assert!(failing.is_empty(), "{failing:?}");
}

#[test]
fn a_missing_tracewise_program_is_an_error() {
    let (_tmp, env) = fake_env();
    fs::remove_file(env.src_dir.join("tracewise")).unwrap();
    assert!(setup_in(&env, &opts(Login::No)).is_err());
}

#[test]
fn setup_profile_writes_config_idempotently() {
    let (_tmp, env) = fake_env();
    let m1 = setup_in(&env, &opts(Login::No)).unwrap();
    assert_eq!(m1.profile, "core");
    let cfg = fs::read_to_string(env.root.join("config.toml")).unwrap();
    assert_eq!(cfg, "[profile]\nname = \"core\"\n");
    let m2 = setup_in(&env, &opts(Login::No)).unwrap();
    assert_eq!(m2.profile, "core");
    assert_eq!(fs::read_to_string(env.root.join("config.toml")).unwrap(), cfg);
}

#[test]
fn doctor_compares_programs_with_the_candidate_checksums() {
    let (_tmp, env) = fake_env();
    let names: Vec<&str> = PROGRAMS.to_vec();
    let sums = std::process::Command::new("shasum").args(["-a", "256"]).args(&names).current_dir(&env.src_dir).output().unwrap();
    fs::write(env.src_dir.join(CHECKSUM_FILE), &sums.stdout).unwrap();
    setup_in(&env, &opts(Login::Yes)).unwrap();
    let find = |env: &Env| doctor::checks(env, "").into_iter().find(|c| c.what.contains("checksums"));
    assert!(find(&env).is_some_and(|c| c.ok), "matching programs pass");
    fs::write(env.bin().join("tracewise-shim"), "tampered").unwrap();
    let c = find(&env).unwrap();
    assert!(!c.ok && c.what.contains("tracewise-shim"), "{}", c.what);
}

#[test]
fn fish_gets_one_file_that_setup_writes_and_uninstall_removes_and_a_foreign_file_is_left_alone() {
    let (_tmp, env) = fake_env();
    fs::create_dir_all(env.home.join(".config/fish")).unwrap(); // fish is installed
    setup_in(&env, &opts(Login::No)).unwrap();
    let file = install::fish_file(&env);
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains(BLOCK_START) && text.contains("tracewise shell init fish | source"), "{text}");
    assert!(text.contains("$HOME/.tracewise/shims") && text.contains("$HOME/.tracewise/bin"), "{text}");
    setup_in(&env, &opts(Login::No)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), text, "twice is the same");
    uninstall_in(&env, false, false, false);
    assert!(!file.exists(), "the file was only Tracewise's");

    // a tracewise.fish the person wrote is never overwritten or removed
    fs::write(&file, "# mine\nset -gx FOO 1\n").unwrap();
    setup_in(&env, &opts(Login::No)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "# mine\nset -gx FOO 1\n");
    uninstall_in(&env, false, false, false);
    assert!(file.exists());
}

#[test]
fn without_fish_no_fish_file_is_written() {
    let (_tmp, env) = fake_env();
    if install::user_shell() == "fish" {
        return; // the machine running the tests uses fish: the file is expected there
    }
    setup_in(&env, &opts(Login::No)).unwrap();
    assert!(!install::fish_file(&env).exists());
}

#[test]
fn purge_keeps_project_histories_unless_asked_to_delete_them() {
    let (_tmp, env) = fake_env();
    setup_in(&env, &opts(Login::No)).unwrap();
    let db = env.root.join("projects/tw1:abababababababababababab/tracewise.db");
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    fs::write(&db, "history").unwrap();
    fs::write(env.root.join("device.key"), "key").unwrap();
    fs::write(env.root.join("repos"), "list").unwrap();
    uninstall_in(&env, true, false, false);
    assert_eq!(fs::read_to_string(&db).unwrap(), "history");
    assert!(env.root.join("device.key").exists(), "the key that owns the projects stays with them");
    assert!(!env.bin().exists() && !env.root.join("repos").exists());
    uninstall_in(&env, true, true, false);
    assert!(!env.root.exists());
}

#[test]
fn uninstall_removes_the_user_scope_agent_hooks() {
    let (_tmp, env) = fake_env();
    let repo = tracewise_git::RepoPaths {
        work_tree: env.home.join("proj"),
        git_dir: env.home.join("proj/.git"),
        git_common_dir: env.home.join("proj/.git"),
    };
    let cfg = env.home.join(".claude/settings.json");
    fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    fs::write(&cfg, r#"{"theme":"dark"}"#).unwrap();
    crate::cmd_agents::install_agent_quiet(&repo, "claude", crate::cmd_agents::AgentScope::User, &env.home).unwrap();
    crate::cmd_agents::install_agent_quiet(&repo, "codex", crate::cmd_agents::AgentScope::User, &env.home).unwrap();
    let removed = uninstall_in(&env, false, false, false);
    for f in [".claude/settings.json", ".codex/hooks.json"] {
        let text = fs::read_to_string(env.home.join(f)).unwrap();
        assert!(!text.contains("tracewise hook"), "{f}: {text}");
    }
    assert!(fs::read_to_string(&cfg).unwrap().contains("dark"));
    assert!(!env.home.join(".claude/skills/tracewise/SKILL.md").exists());
    assert!(removed.iter().any(|r| r.contains(".claude/settings.json")), "{removed:?}");
}

#[test]
fn purge_keeps_the_device_name_the_kept_histories_push_under() {
    let (_tmp, env) = fake_env();
    setup_in(&env, &opts(Login::No)).unwrap();
    fs::create_dir_all(env.root.join("projects/p")).unwrap();
    fs::write(env.root.join("device"), "my-mac").unwrap();
    uninstall_in(&env, true, false, false);
    assert_eq!(fs::read_to_string(env.root.join("device")).unwrap(), "my-mac");
}

#[test]
fn a_short_name_that_another_program_already_has_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let other = tmp.path().join("other");
    let own = tmp.path().join("own");
    fs::create_dir_all(&other).unwrap();
    fs::create_dir_all(&own).unwrap();
    fs::write(other.join("trace"), "#!/bin/sh\n").unwrap();
    fs::write(own.join("tracwise"), "ours").unwrap();
    let path = std::env::join_paths([&other, &own]).unwrap();
    assert!(install::name_is_taken("trace", &own, Some(path.as_os_str())), "another trace is on PATH");
    assert!(!install::name_is_taken("tracwise", &own, Some(path.as_os_str())), "our own folder does not count");
    assert!(!install::name_is_taken("trace", &own, None));
}

#[test]
fn agents_mode_writes_the_setting_and_installs_no_login_item() {
    let (_tmp, env) = fake_env();
    let m = setup_in(&env, &SetupOptions { capture: Some(CaptureMode::Agents), ..opts(Login::Yes) }).unwrap();
    let cfg = fs::read_to_string(env.root.join("config.toml")).unwrap();
    assert_eq!(crate::cmd_config::read_value(&cfg, "capture.mode").as_deref(), Some("agents"));
    assert_eq!(crate::cmd_config::read_value(&cfg, "profile.name").as_deref(), Some("core"));
    assert!(m.launch_agent.is_none(), "even with --login yes");
    assert!(!env.agent_plist().exists());
}

#[test]
fn a_later_full_setup_switches_back_and_keeps_other_settings() {
    let (_tmp, env) = fake_env();
    fs::create_dir_all(&env.root).unwrap();
    fs::write(env.root.join("config.toml"), "[reasons]\nmode = \"enforce\"\n").unwrap();
    setup_in(&env, &SetupOptions { capture: Some(CaptureMode::Agents), ..opts(Login::No) }).unwrap();
    let m = setup_in(&env, &SetupOptions { capture: Some(CaptureMode::Full), ..opts(Login::Yes) }).unwrap();
    let cfg = fs::read_to_string(env.root.join("config.toml")).unwrap();
    assert_eq!(crate::cmd_config::read_value(&cfg, "capture.mode").as_deref(), Some("full"));
    assert_eq!(crate::cmd_config::read_value(&cfg, "reasons.mode").as_deref(), Some("enforce"), "a setting the person made survives setup");
    assert!(m.launch_agent.is_some() && env.agent_plist().is_file());
}

#[test]
fn a_setup_that_cannot_ask_keeps_the_mode_that_was_chosen() {
    let (_tmp, env) = fake_env();
    setup_in(&env, &SetupOptions { capture: Some(CaptureMode::Agents), ..opts(Login::No) }).unwrap();
    // run again by a script: `--capture ask` without a terminal resolves to nothing
    let again = resolve_capture("ask", false, || panic!("must not ask"));
    assert_eq!(again, None);
    let m = setup_in(&env, &SetupOptions { capture: again, ..opts(Login::Yes) }).unwrap();
    let cfg = fs::read_to_string(env.root.join("config.toml")).unwrap();
    assert_eq!(crate::cmd_config::read_value(&cfg, "capture.mode").as_deref(), Some("agents"));
    assert!(m.launch_agent.is_none(), "still no login item: the mode is still agents");
}

#[test]
fn the_capture_choice_resolves_without_asking_unless_it_can() {
    let never = || -> String { panic!("must not ask") };
    assert_eq!(resolve_capture("agents", true, never), Some(CaptureMode::Agents));
    assert_eq!(resolve_capture("full", true, never), Some(CaptureMode::Full));
    assert_eq!(resolve_capture("ask", false, never), None, "no terminal: no question, nothing changes");
    assert_eq!(resolve_capture("ask", true, || "agents\n".to_string()), Some(CaptureMode::Agents));
    assert_eq!(resolve_capture("ask", true, || "A".to_string()), Some(CaptureMode::Agents));
    assert_eq!(resolve_capture("ask", true, || "\n".to_string()), Some(CaptureMode::Full), "enter is the default");
    assert_eq!(resolve_capture("ask", true, || "nonsense".to_string()), Some(CaptureMode::Full));
}
