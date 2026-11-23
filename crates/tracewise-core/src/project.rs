//! A project's identity without git: a signed genesis entry. Its hash is the project id (`tw1:` + 26 hex chars); the owner is this
//! device's key, and the recovery key (secret shown once, never stored) can later prove the right to take the project over.
use crate::share::sign::{device_public_hex, hex, hex_to, sign_hex};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

const GENESIS_FILE: &str = "genesis.json";
const SECRET_PREFIX: &str = "twrecover1:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Genesis {
    pub owner: String,
    pub recovery: String,
    pub created: String,
    pub nonce: String,
    pub sig: String,
}

/// The bytes that are hashed into the id and signed: every field but the signature.
fn canonical(g: &Genesis) -> String {
    format!("{}|{}|{}|{}", g.owner, g.recovery, g.created, g.nonce)
}

fn random<const N: usize>() -> io::Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).map_err(|e| io::Error::other(format!("no random numbers: {e}")))?;
    Ok(b)
}

/// A new genesis owned by this device, and the recovery secret (`twrecover1:<64 hex>`), which is returned once and not kept.
pub fn new_genesis() -> io::Result<(Genesis, String)> {
    let seed = random::<32>()?;
    let mut g = Genesis {
        owner: device_public_hex()?,
        recovery: hex(SigningKey::from_bytes(&seed).verifying_key().as_bytes()),
        created: crate::clock::now_text(),
        nonce: hex(&random::<16>()?),
        sig: String::new(),
    };
    g.sig = sign_hex(canonical(&g).as_bytes())?;
    Ok((g, format!("{SECRET_PREFIX}{}", hex(&seed))))
}

pub fn project_id(g: &Genesis) -> String {
    format!("tw1:{}", &blake3::hash(canonical(g).as_bytes()).to_hex()[..26])
}

/// Whether the owner signed this genesis.
pub fn verify_genesis(g: &Genesis) -> bool {
    let (Some(key), Some(sig)) = (hex_to::<32>(&g.owner), hex_to::<64>(&g.sig)) else { return false };
    VerifyingKey::from_bytes(&key).is_ok_and(|k| k.verify(canonical(g).as_bytes(), &Signature::from_bytes(&sig)).is_ok())
}

/// The recovery public key (hex) a recovery secret belongs to.
pub fn recovery_public(secret: &str) -> Option<String> {
    let seed = hex_to::<32>(secret.trim().strip_prefix(SECRET_PREFIX)?)?;
    Some(hex(SigningKey::from_bytes(&seed).verifying_key().as_bytes()))
}

pub fn save_genesis(data_dir: &Path, g: &Genesis) -> io::Result<()> {
    tracewise_platform::private::write(data_dir.join(GENESIS_FILE), serde_json::to_string_pretty(g).unwrap_or_default())
}

pub fn load_genesis(data_dir: &Path) -> Option<Genesis> {
    serde_json::from_str(&std::fs::read_to_string(data_dir.join(GENESIS_FILE)).ok()?).ok()
}

#[cfg(test)]
#[path = "test_project.rs"]
mod tests;
