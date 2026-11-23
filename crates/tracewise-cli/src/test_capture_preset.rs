//! Capture preset tests: the data file, the derived current preset, and applying through `setup` without disagreement.
use super::*;
use tracewise_core::capture::CaptureMode;

use crate::setup::tests::fake_env;

fn named<'a>(presets: &'a [Preset], name: &str) -> &'a Preset {
    presets.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no preset {name}"))
}

#[test]
fn the_built_in_presets_are_valid_data_and_each_decides_a_layer() {
    let (presets, warnings) = load(Path::new("/nonexistent/presets.toml"));
    assert!(warnings.is_empty(), "{warnings:?}");
    let names: Vec<_> = presets.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["agents", "balanced", "full"]);
    assert_eq!((named(&presets, "agents").terminal, named(&presets, "agents").daemon), (Some(false), Some(false)));
    assert_eq!((named(&presets, "full").terminal, named(&presets, "full").daemon), (Some(true), Some(true)));
    assert!(presets.iter().all(|p| p.built_in && !p.description.is_empty()));
}

#[test]
fn a_users_file_replaces_by_name_adds_new_ones_and_never_breaks_the_built_in_ones() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("presets.toml");
    fs::write(
        &file,
        "[preset.agents]\ndescription = \"mine\"\nlayers = { terminal = true }\n\n[preset.ci]\ndescription = \"CI\"\nlayers = { daemon = false, hooks = true }\n\n[preset.Bad_Name]\nlayers = { daemon = true }\n\n[preset.empty]\nlayers = {}\n",
    )
    .unwrap();
    let (presets, warnings) = load(&file);
    let agents = named(&presets, "agents");
    assert_eq!((agents.description.as_str(), agents.terminal, agents.daemon, agents.built_in), ("mine", Some(true), None, false));
    assert_eq!(named(&presets, "ci").daemon, Some(false));
    assert!(presets.iter().all(|p| p.name != "Bad_Name" && p.name != "empty"));
    assert!(warnings.iter().any(|w| w.contains("unknown layer `hooks`")), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("Bad_Name")) && warnings.iter().any(|w| w.contains("`empty`")), "{warnings:?}");
    assert!(presets.iter().any(|p| p.name == "full" && p.built_in), "the others are untouched");
    // a file that does not parse is a warning and changes nothing
    fs::write(&file, "[preset.x\nbroken").unwrap();
    let (presets, warnings) = load(&file);
    assert_eq!(presets.len(), 3);
    assert_eq!(warnings.len(), 1);
}

#[test]
fn the_current_preset_is_worked_out_from_what_is_installed_and_follows_setup() {
    let (_tmp, env) = fake_env();
    let (presets, _) = load(&env.root.join("presets.toml"));
    let none = probe(&env);
    assert!(!none.terminal && !none.daemon);
    assert_eq!(current(&presets, &none).unwrap().name, "agents", "nothing installed is the agents preset");
    // plain `setup` with a login item is `full`; the preset is not stored anywhere, so it just follows
    crate::setup::setup_in(&env, &SetupOptions { login: Login::Yes, start: false, shell_rc: true, terminal: true, live: false, profile: "core".into(), capture: None }).unwrap();
    assert_eq!(current(&presets, &probe(&env)).unwrap().name, "full");
    crate::setup::setup_in(&env, &SetupOptions { login: Login::No, start: false, shell_rc: true, terminal: true, live: false, profile: "core".into(), capture: Some(CaptureMode::Agents) }).unwrap();
    assert_eq!(current(&presets, &probe(&env)).unwrap().name, "balanced");
}

#[test]
fn applying_a_preset_leaves_layers_and_capture_mode_agreeing_in_every_direction() {
    let (_tmp, env) = fake_env();
    let (presets, _) = load(&env.root.join("presets.toml"));
    for name in ["full", "balanced", "agents", "full", "agents", "balanced"] {
        let p = named(&presets, name);
        apply(&env, p, false).unwrap();
        let s = probe(&env);
        assert_eq!(current(&presets, &s).map(|p| p.name.as_str()), Some(name), "after applying {name}");
        assert!(plan(p, &s).iter().all(|step| step.contains("start the watcher")), "nothing left to do but start a watcher that tests do not start: {:?}", plan(p, &s));
        let problems = inconsistencies(&s);
        assert!(problems.iter().all(|p| p.contains("not running")), "{name}: {problems:?}");
        assert_eq!(s.mode, if p.daemon == Some(true) { CaptureMode::Full } else { CaptureMode::Agents }, "{name}: capture.mode follows the daemon layer");
    }
}

#[test]
fn turning_the_terminal_off_takes_only_tracewises_lines_out_and_keeps_the_programs() {
    let (_tmp, env) = fake_env();
    fs::write(env.zshrc(), "export FOO=1\nalias ll='ls -l'\n").unwrap();
    let (presets, _) = load(&env.root.join("presets.toml"));
    apply(&env, named(&presets, "balanced"), false).unwrap();
    assert!(fs::read_to_string(env.zshrc()).unwrap().contains(crate::setup::BLOCK_START));
    apply(&env, named(&presets, "agents"), false).unwrap();
    assert_eq!(fs::read_to_string(env.zshrc()).unwrap(), "export FOO=1\nalias ll='ls -l'\n", "the person's own lines are as they were");
    assert!(env.bin().join("tracewise").is_file(), "the programs stay");
    // agent hooks call `tracewise` by name, so the programs stay on PATH with every preset; only the shims and the hook go
    let zshenv = fs::read_to_string(env.zshenv()).unwrap();
    assert!(zshenv.contains(".tracewise/bin") && !zshenv.contains("shims"), "{zshenv}");
    assert!(probe(&env).terminal == false);
    apply(&env, named(&presets, "balanced"), false).unwrap();
    assert!(fs::read_to_string(env.zshenv()).unwrap().contains("shims"));
    assert!(probe(&env).terminal);
    assert!(!env.agent_plist().exists());
}

#[test]
fn a_mode_that_disagrees_with_the_watcher_is_named_and_a_preset_fixes_it() {
    let (_tmp, env) = fake_env();
    let (presets, _) = load(&env.root.join("presets.toml"));
    apply(&env, named(&presets, "full"), false).unwrap();
    // someone sets the mode by hand to agents while the login item stays
    let cfg = env.root.join("config.toml");
    let text = crate::cmd_config::set_value(&fs::read_to_string(&cfg).unwrap(), "capture.mode", "agents");
    fs::write(&cfg, text).unwrap();
    let s = probe(&env);
    assert!(inconsistencies(&s).iter().any(|p| p.contains("disagree")), "{:?}", inconsistencies(&s));
    assert!(!plan(named(&presets, "full"), &s).is_empty(), "full is not what is configured, so applying it has something to do");
    apply(&env, named(&presets, "full"), false).unwrap();
    assert!(!inconsistencies(&probe(&env)).iter().any(|p| p.contains("disagree")));
}

#[test]
fn dry_run_changes_nothing_and_an_unknown_name_is_refused() {
    let (_tmp, env) = fake_env();
    let (presets, _) = load(&env.root.join("presets.toml"));
    assert_eq!(run_use(&env, &presets, "full", true, false, false, false), 0);
    assert!(!env.agent_plist().exists() && !env.root.join("config.toml").exists());
    assert_eq!(run_use(&env, &presets, "nope", true, false, false, false), 2);
    // nothing installed, but the default mode is `full`: `agents` has the mode to set, and without a terminal it does not ask
    assert_eq!(run_use(&env, &presets, "agents", false, false, false, false), 2);
    assert_eq!(run_use(&env, &presets, "agents", false, true, false, false), 0);
    assert_eq!(run_use(&env, &presets, "agents", false, false, false, false), 0, "now it is agents: nothing to change, nothing to ask");
}
