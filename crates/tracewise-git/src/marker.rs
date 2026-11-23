//! The `.tracewiseproject` marker: names a project folder without needing git.

use std::io::{self, Write};
use std::path::Path;

pub const MARKER_FILE: &str = ".tracewiseproject";

/// `tw1:` followed by 26 lowercase hex digits; the id becomes a folder name, so nothing else may pass.
pub fn is_valid_project_id(s: &str) -> bool {
    s.strip_prefix("tw1:").is_some_and(|h| h.len() == 26 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

/// The project id in `dir/.tracewiseproject`; `None` when the file is missing or the id is invalid.
pub fn read_marker(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(MARKER_FILE)).ok()?;
    text.lines()
        .find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == "project").map(|(_, v)| v.trim().to_string()))
        .filter(|id| is_valid_project_id(id))
}

/// Writes the marker through a temp file and a rename, so a reader never sees half of it.
pub fn write_marker(dir: &Path, id: &str) -> io::Result<()> {
    if !is_valid_project_id(id) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid project id"));
    }
    let tmp = dir.join(format!("{MARKER_FILE}.tmp"));
    // a fresh file every time: a link planted at the temp name is removed, never followed
    let _ = std::fs::remove_file(&tmp);
    std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?.write_all(format!("project = {id}\n").as_bytes())?;
    std::fs::rename(&tmp, dir.join(MARKER_FILE))
}

#[cfg(test)]
#[path = "test_marker.rs"]
mod tests;
