use super::*;
use tempfile::TempDir;

/// A genesis made with this test's own Tracewise home (it holds the device key). The guard holds the process-wide env lock.
fn genesis() -> (std::sync::MutexGuard<'static, ()>, TempDir, Genesis, String) {
    let guard = crate::test_env::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = TempDir::new().unwrap();
    std::env::set_var("TRACEWISE_HOME", home.path());
    let (g, secret) = new_genesis().unwrap();
    (guard, home, g, secret)
}

/// The public key of another valid signing key: well-formed hex, but not the key that signed.
fn other_owner() -> String {
    hex(SigningKey::from_bytes(&[7u8; 32]).verifying_key().as_bytes())
}

#[test]
fn the_id_has_the_tw1_shape() {
    let (_g, _home, g, _) = genesis();
    let id = project_id(&g);
    assert_eq!(id.len(), 30);
    assert!(id.starts_with("tw1:") && id[4..].bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
}

#[test]
fn the_id_is_deterministic_ignores_the_signature_and_follows_the_owner() {
    let (_g, _home, g, _) = genesis();
    let id = project_id(&g);
    assert_eq!(id, project_id(&g.clone()));
    assert_eq!(id, project_id(&Genesis { sig: "x".into(), ..g.clone() }));
    assert_ne!(id, project_id(&Genesis { owner: other_owner(), ..g.clone() }));
}

#[test]
fn the_signature_covers_owner_and_nonce() {
    let (_g, _home, g, _) = genesis();
    assert!(verify_genesis(&g));
    assert_eq!(other_owner().len(), 64);
    assert!(!verify_genesis(&Genesis { owner: other_owner(), ..g.clone() }), "a valid key that did not sign");
    assert!(!verify_genesis(&Genesis { nonce: "00".into(), ..g.clone() }));
}

#[test]
fn the_recovery_secret_belongs_to_the_recovery_key() {
    let (_g, _home, g, secret) = genesis();
    assert_eq!(recovery_public(&secret), Some(g.recovery.clone()));
    assert_eq!(recovery_public("nonsense"), None);
}

#[test]
fn every_genesis_has_its_own_nonce() {
    let (_g, _home, g, _) = genesis();
    assert_ne!(project_id(&g), project_id(&new_genesis().unwrap().0));
}

#[test]
fn a_genesis_is_saved_and_loaded() {
    let (_g, _home, g, _) = genesis();
    let dir = TempDir::new().unwrap();
    assert_eq!(load_genesis(dir.path()), None);
    save_genesis(dir.path(), &g).unwrap();
    assert_eq!(load_genesis(dir.path()), Some(g));
}
