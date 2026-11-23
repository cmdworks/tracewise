use super::*;

#[test]
pub fn tracewise_files_and_hook_configs_are_protected() {
    let wt = Path::new("/w/repo");
    for p in [
        ".claude/settings.json",
        ".claude/settings.local.json",
        ".agents/hooks.json",
        ".codex/hooks.json",
        ".gemini/settings.json",
        ".cursor/hooks.json",
        ".git/tracewise/tracewise.db",
        ".git/tracewise",
        ".tracewiseconfig",
        "/w/repo/.git/tracewise/objects",
        "src/../.claude/settings.json",
    ] {
        assert!(is_tracewise_protected(p, wt), "{p}");
    }
    for p in ["src/main.rs", ".claude/skills/x.md", ".gitignore", "/etc/hosts", "../other/.claude/settings.json"] {
        assert!(!is_tracewise_protected(p, wt), "{p}");
    }
}

#[test]
pub fn marker_is_protected_from_agents() {
    let wt = Path::new("/w/repo");
    assert!(is_tracewise_protected(".tracewiseproject", wt));
    let db = tracewise_core::get_tracewise_home().join("projects/tw1:abababababababababababab/tracewise.db");
    assert!(is_tracewise_protected(&db.to_string_lossy(), wt));
}

#[test]
pub fn user_scope_hook_configs_are_protected() {
    let (wt, home, tw) = (Path::new("/w/repo"), Path::new("/u/me"), Path::new("/u/me/.tracewise"));
    for p in [
        "/u/me/.claude/settings.json",
        "/u/me/.claude/settings.local.json",
        "/u/me/.codex/hooks.json",
        "/u/me/.codex/config.toml",
        "/u/me/.gemini/settings.json",
        "/u/me/.cursor/hooks.json",
        "/u/me/.agents/hooks.json",
        "/u/me/.claude/skills/tracewise/SKILL.md",
        "/w/repo/../../u/me/.claude/settings.json",
    ] {
        assert!(is_protected_in(p, wt, home, tw), "{p}");
    }
    for p in ["/u/me/.claude/skills/other/SKILL.md", "/u/me/notes.txt", "/u/me/.gemini/other.json"] {
        assert!(!is_protected_in(p, wt, home, tw), "{p}");
    }
}

#[test]
pub fn a_marker_is_protected_at_any_depth() {
    let (wt, home, tw) = (Path::new("/w/repo"), Path::new("/u/me"), Path::new("/u/me/.tracewise"));
    for p in ["sub/dir/.tracewiseproject", ".tracewiseproject.tmp", "/w/main/.tracewiseproject", "a/.tracewiseproject.tmp"] {
        assert!(is_protected_in(p, wt, home, tw), "{p}");
    }
    assert!(!is_protected_in("sub/.tracewiseproject.md", wt, home, tw));
}

#[test]
pub fn an_empty_home_protects_nothing_extra() {
    let (wt, tw) = (Path::new("/w/repo"), Path::new("/t/.tracewise"));
    assert!(!is_protected_in("/anything/at/all", wt, Path::new(""), tw));
    assert!(!is_protected_in("src/main.rs", wt, Path::new(""), tw));
}
