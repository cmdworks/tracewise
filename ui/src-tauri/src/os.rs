//! The few things the menu asks of macOS: a question in a dialog, a notification, and a login item. All of them go through
//! `osascript` and a plist file, so the app needs no extra crate and no permission beyond what the system asks for itself.
use std::path::{Path, PathBuf};
use std::process::Command;

/// `text` as an AppleScript string literal. Text that comes from the history (a reason, a path) is never put into a script as it is.
pub fn applescript_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn osascript(script: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("osascript").arg("-e").arg(script).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = script;
        None
    }
}

/// The script for a question with two buttons; the answer is the text of the button that was pressed.
pub fn confirm_script(title: &str, text: &str, ok: &str) -> String {
    format!(
        "display dialog {} with title {} buttons {{\"Cancel\", {}}} default button \"Cancel\" cancel button \"Cancel\" with icon caution",
        applescript_quote(text),
        applescript_quote(title),
        applescript_quote(ok)
    )
}

/// Asks the person to confirm; false on Cancel, on a closed dialog, and where there is no dialog to show.
pub fn confirm(title: &str, text: &str, ok: &str) -> bool {
    osascript(&confirm_script(title, text, ok)).is_some_and(|answer| answer.contains(&format!("button returned:{ok}")))
}

/// The script for a question with a text field.
pub fn ask_script(title: &str, prompt: &str, ok: &str) -> String {
    format!(
        "text returned of (display dialog {} with title {} default answer \"\" buttons {{\"Cancel\", {}}} default button {} cancel button \"Cancel\")",
        applescript_quote(prompt),
        applescript_quote(title),
        applescript_quote(ok),
        applescript_quote(ok)
    )
}

/// Asks for a line of text; None on Cancel.
pub fn ask_text(title: &str, prompt: &str, ok: &str) -> Option<String> {
    osascript(&ask_script(title, prompt, ok)).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// A message the person only has to read.
pub fn inform(title: &str, text: &str) {
    let _ = osascript(&format!("display dialog {} with title {} buttons {{\"OK\"}} default button \"OK\"", applescript_quote(text), applescript_quote(title)));
}

/// A notification in the corner of the screen.
pub fn notify(title: &str, text: &str) {
    let _ = osascript(&format!("display notification {} with title {}", applescript_quote(text), applescript_quote(title)));
}

// ---- the login item ----

pub const LOGIN_LABEL: &str = "dev.tracewise.menubar";

pub fn login_item_path(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents").join(format!("{LOGIN_LABEL}.plist"))
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The launch agent that starts the app at login with only its icon (`--menubar`).
pub fn login_item_plist(exe: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>{LOGIN_LABEL}</string>\n  <key>ProgramArguments</key>\n  <array>\n    <string>{}</string>\n    <string>--menubar</string>\n  </array>\n  <key>RunAtLoad</key><true/>\n  <key>ProcessType</key><string>Interactive</string>\n</dict>\n</plist>\n",
        xml_escape(&exe.to_string_lossy())
    )
}

pub fn login_item_enabled(home: &Path) -> bool {
    login_item_path(home).is_file()
}

/// Starts the app at login (writes the launch agent) or stops doing so (removes it). Takes effect at the next login; nothing is
/// started or stopped now.
pub fn set_login_item(home: &Path, exe: &Path, enabled: bool) -> std::io::Result<()> {
    let path = login_item_path(home);
    if enabled {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, login_item_plist(exe))
    } else {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

// ---- the menu's own settings ----

/// What the person chose in the menu, kept in `<Tracewise home>/menubar.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Prefs {
    /// Notifications are off until the person turns them on.
    pub notifications: bool,
}

pub fn prefs_path(tracewise_home: &Path) -> PathBuf {
    tracewise_home.join("menubar.json")
}

pub fn read_prefs(tracewise_home: &Path) -> Prefs {
    let text = std::fs::read_to_string(prefs_path(tracewise_home)).unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    Prefs { notifications: value["notifications"].as_bool().unwrap_or(false) }
}

pub fn write_prefs(tracewise_home: &Path, prefs: Prefs) -> std::io::Result<()> {
    std::fs::create_dir_all(tracewise_home)?;
    std::fs::write(prefs_path(tracewise_home), serde_json::json!({ "notifications": prefs.notifications }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_from_the_history_cannot_break_out_of_an_applescript_string() {
        assert_eq!(applescript_quote("plain"), "\"plain\"");
        assert_eq!(applescript_quote("say \"hi\" \\ there"), "\"say \\\"hi\\\" \\\\ there\"");
        assert_eq!(applescript_quote("two\nlines"), "\"two lines\"");
        let evil = "\" & (do shell script \"rm -rf ~\") & \"";
        let quoted = applescript_quote(evil);
        // every quote inside is escaped, so the whole thing stays one string
        let inner = &quoted[1..quoted.len() - 1];
        assert!(inner.matches('"').count() == inner.matches("\\\"").count(), "{quoted}");
        assert!(confirm_script("t", evil, "Undo").contains("\\\" & (do shell script"));
    }

    #[test]
    fn the_scripts_have_the_buttons_and_the_default_the_dialogs_need() {
        let s = confirm_script("Undo", "Undo 3 changes by claude?", "Undo");
        assert!(s.contains("buttons {\"Cancel\", \"Undo\"}") && s.contains("default button \"Cancel\""), "a confirmation defaults to the safe button: {s}");
        assert!(ask_script("Reason", "Why?", "Save").starts_with("text returned of (display dialog"));
    }

    #[test]
    fn the_login_item_starts_the_app_with_only_its_icon_and_is_written_and_removed() {
        let home = std::env::temp_dir().join(format!("tracewise-login-{}", std::process::id()));
        let exe = Path::new("/Applications/Tracewise.app/Contents/MacOS/ui & co");
        let plist = login_item_plist(exe);
        assert!(plist.contains("<string>--menubar</string>") && plist.contains("<key>RunAtLoad</key><true/>"), "{plist}");
        assert!(plist.contains("ui &amp; co"), "the path is escaped for XML");
        assert!(!login_item_enabled(&home));
        set_login_item(&home, exe, true).unwrap();
        assert!(login_item_enabled(&home));
        set_login_item(&home, exe, false).unwrap();
        assert!(!login_item_enabled(&home));
        set_login_item(&home, exe, false).unwrap();
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_menus_settings_default_to_quiet_and_survive_a_restart() {
        let home = std::env::temp_dir().join(format!("tracewise-prefs-{}", std::process::id()));
        assert_eq!(read_prefs(&home), Prefs::default());
        assert!(!read_prefs(&home).notifications);
        write_prefs(&home, Prefs { notifications: true }).unwrap();
        assert!(read_prefs(&home).notifications);
        std::fs::write(prefs_path(&home), "not json").unwrap();
        assert!(!read_prefs(&home).notifications, "a file that does not read is the default");
        let _ = std::fs::remove_dir_all(&home);
    }
}
