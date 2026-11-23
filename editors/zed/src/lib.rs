//! Starts `tracewise lsp` for Zed. All the work is in that program: hover says why a line changed, a save is recorded, and
//! (with `"initialization_options": {"inlayBlame": true}` in Zed's `lsp.tracewise` settings) each run of changed lines gets a hint.
//! The extension finds the program on the user's `PATH`; it never downloads or runs anything else.
use zed_extension_api::{self as zed, LanguageServerId, Result};

const NOT_FOUND: &str = "tracewise was not found on your PATH. Install it (https://github.com/cmdworks/tracewise#download), then restart Zed.";

struct Tracewise;

impl zed::Extension for Tracewise {
    fn new() -> Self {
        Tracewise
    }

    fn language_server_command(&mut self, _id: &LanguageServerId, worktree: &zed::Worktree) -> Result<zed::Command> {
        let command = worktree.which("tracewise").ok_or_else(|| NOT_FOUND.to_string())?;
        Ok(zed::Command { command, args: vec!["lsp".to_string()], env: worktree.shell_env() })
    }

    /// Asks why after each save without any setting; `lsp.tracewise.initialization_options` in the user's settings overrides this.
    fn language_server_initialization_options(&mut self, _id: &LanguageServerId, _worktree: &zed::Worktree) -> Result<Option<zed::serde_json::Value>> {
        Ok(Some(zed::serde_json::json!({ "askWhy": true })))
    }
}

zed::register_extension!(Tracewise);
