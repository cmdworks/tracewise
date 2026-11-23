//! How much of the work happens live. `full` (the default) has a background watcher record every change from anything;
//! `agents` runs no watcher: agents, the commands you type and git are recorded as they happen, and everything else is
//! picked up by a catch-up scan the next time a command reads history or an agent turn starts.
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureMode {
    #[default]
    Full,
    Agents,
}

impl CaptureMode {
    pub fn parse(text: &str) -> Option<Self> {
        crate::model::parse_choice(text, &[("full", Self::Full), ("agents", Self::Agents)])
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Agents => "agents",
        }
    }
}

/// The mode for the project at `root`, first one found: the `TRACEWISE_CAPTURE` variable, `[capture] mode` in the project's
/// `.tracewiseconfig`, the same in the global `config.toml`. A value that is not a mode means `full`.
pub fn capture_mode(root: &Path) -> CaptureMode {
    let from_env = std::env::var("TRACEWISE_CAPTURE").ok().filter(|v| !v.trim().is_empty());
    let text = from_env.or_else(|| crate::reasons::config_value(root, "capture", "mode"));
    text.as_deref().and_then(CaptureMode::parse).unwrap_or_default()
}

#[cfg(test)]
#[path = "test_capture.rs"]
mod tests;
