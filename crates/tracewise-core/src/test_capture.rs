use super::*;
use crate::test_env::ENV_LOCK;
use std::fs;

/// Runs `f` with an empty tracewise home, no `TRACEWISE_CAPTURE`, and a project folder; both folders are removed afterwards.
fn with_folders(f: impl FnOnce(&Path, &Path)) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::env::set_var("TRACEWISE_HOME", home.path());
    std::env::remove_var("TRACEWISE_CAPTURE");
    f(home.path(), project.path());
    std::env::remove_var("TRACEWISE_HOME");
    std::env::remove_var("TRACEWISE_CAPTURE");
}

#[test]
fn the_default_is_full() {
    with_folders(|_, project| assert_eq!(capture_mode(project), CaptureMode::Full));
}

#[test]
fn the_global_config_sets_it() {
    with_folders(|home, project| {
        fs::write(home.join("config.toml"), "[capture]\nmode = \"agents\"\n").unwrap();
        assert_eq!(capture_mode(project), CaptureMode::Agents);
    });
}

#[test]
fn the_project_config_beats_the_global_one() {
    with_folders(|home, project| {
        fs::write(home.join("config.toml"), "[capture]\nmode = \"agents\"\n").unwrap();
        fs::write(project.join(".tracewiseconfig"), "[capture]\nmode = \"full\"\n").unwrap();
        assert_eq!(capture_mode(project), CaptureMode::Full);
    });
}

#[test]
fn the_environment_beats_both() {
    with_folders(|home, project| {
        fs::write(home.join("config.toml"), "[capture]\nmode = \"full\"\n").unwrap();
        std::env::set_var("TRACEWISE_CAPTURE", "agents");
        assert_eq!(capture_mode(project), CaptureMode::Agents);
        std::env::set_var("TRACEWISE_CAPTURE", " FULL ");
        fs::write(project.join(".tracewiseconfig"), "[capture]\nmode = \"agents\"\n").unwrap();
        assert_eq!(capture_mode(project), CaptureMode::Full, "case and spaces do not matter");
    });
}

#[test]
fn an_unknown_value_means_full() {
    with_folders(|home, project| {
        fs::write(home.join("config.toml"), "[capture]\nmode = \"sometimes\"\n").unwrap();
        assert_eq!(capture_mode(project), CaptureMode::Full);
        std::env::set_var("TRACEWISE_CAPTURE", "nonsense");
        assert_eq!(capture_mode(project), CaptureMode::Full);
    });
}

#[test]
fn names_round_trip() {
    for mode in [CaptureMode::Full, CaptureMode::Agents] {
        assert_eq!(CaptureMode::parse(mode.name()), Some(mode));
    }
}
