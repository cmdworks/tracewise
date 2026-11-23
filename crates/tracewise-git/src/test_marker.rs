use super::*;

#[test]
fn valid_id_is_tw1_plus_26_lowercase_hex() {
    assert!(is_valid_project_id(&format!("tw1:{}", "a".repeat(26))));
    assert!(!is_valid_project_id(&format!("tw1:{}", "a".repeat(25))));
    assert!(!is_valid_project_id(&format!("tw1:{}", "A".repeat(26))));
    assert!(!is_valid_project_id("../x"));
    assert!(!is_valid_project_id(""));
    assert!(!is_valid_project_id("tw1:../../etc/passwd0000000"));
}

#[test]
fn read_marker_returns_none_for_bad_files() {
    let t = tempfile::tempdir().unwrap();
    assert_eq!(read_marker(t.path()), None);
    std::fs::write(t.path().join(MARKER_FILE), "project = ../../x\n").unwrap();
    assert_eq!(read_marker(t.path()), None);
}

#[test]
fn write_then_read_round_trips() {
    let t = tempfile::tempdir().unwrap();
    let id = format!("tw1:{}", "0123456789abcdef0123456789");
    write_marker(t.path(), &id).unwrap();
    assert_eq!(read_marker(t.path()), Some(id));
    assert!(write_marker(t.path(), "nope").is_err());
}

#[cfg(unix)]
#[test]
fn write_marker_never_follows_a_planted_temp_link() {
    let t = tempfile::tempdir().unwrap();
    let victim = t.path().join("victim");
    std::fs::write(&victim, "keep").unwrap();
    std::os::unix::fs::symlink(&victim, t.path().join(format!("{MARKER_FILE}.tmp"))).unwrap();
    let id = format!("tw1:{}", "0123456789abcdef0123456789");
    write_marker(t.path(), &id).unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
    assert_eq!(read_marker(t.path()), Some(id));
}
