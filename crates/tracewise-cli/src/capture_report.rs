//! What the capture mode records live and what it only finds later: the words `status` and `doctor` use, so nobody has to guess.
use tracewise_core::capture::CaptureMode;

const HOOKS: [&str; 4] = ["agent hooks", "commands you type in a hooked shell", "command wrappers", "git"];
const WATCHER: &str = "every file change, from anything (the watcher)";
const CAUGHT_UP: &str =
    "edits you or other programs make in between (found when you next run a command or start an agent turn, stamped with the time found)";

pub(crate) struct CaptureReport {
    pub mode: CaptureMode,
    /// Recorded as it happens.
    pub live: Vec<&'static str>,
    /// Recorded later, by the catch-up scan.
    pub caught_up: Vec<&'static str>,
}

/// `watcher_running` matters in `full` mode: a watcher that is not running leaves the catch-up to do its job.
pub(crate) fn describe(mode: CaptureMode, watcher_running: bool) -> CaptureReport {
    let (live, caught_up) = match (mode, watcher_running) {
        (CaptureMode::Full, true) => (HOOKS.iter().copied().chain([WATCHER]).collect(), Vec::new()),
        _ => (HOOKS.to_vec(), vec![CAUGHT_UP]),
    };
    CaptureReport { mode, live, caught_up }
}

impl CaptureReport {
    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::json!({ "mode": self.mode.name(), "live": self.live, "caught_up": self.caught_up })
    }
}

/// An explanation to print next to the daemon's state when the state does not match the mode.
pub(crate) fn daemon_note(mode: CaptureMode, watcher_running: bool) -> Option<&'static str> {
    match (mode, watcher_running) {
        (CaptureMode::Agents, true) => Some("not needed in agents mode: tracewise capture daemon stop"),
        (CaptureMode::Full, false) => {
            Some("edits are caught up when a command reads the history; tracewise capture daemon start records them live")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_mode_lists_what_is_live_and_what_is_found_later() {
        for running in [false, true] {
            let r = describe(CaptureMode::Agents, running);
            assert_eq!(r.live, HOOKS.to_vec());
            assert_eq!(r.caught_up.len(), 1, "a running watcher does not change what agents mode promises");
        }
    }

    #[test]
    fn full_mode_with_a_watcher_has_nothing_to_catch_up() {
        let r = describe(CaptureMode::Full, true);
        assert!(r.live.contains(&WATCHER) && r.caught_up.is_empty());
    }

    #[test]
    fn full_mode_with_a_stopped_watcher_says_the_catch_up_covers_it() {
        let r = describe(CaptureMode::Full, false);
        assert_eq!(r.caught_up.len(), 1);
        assert!(!r.live.contains(&WATCHER));
    }

    #[test]
    fn the_json_has_the_mode_and_both_lists() {
        let j = describe(CaptureMode::Agents, false).json();
        assert_eq!(j["mode"], "agents");
        assert_eq!(j["live"].as_array().unwrap().len(), 4);
        assert_eq!(j["caught_up"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn the_daemon_note_only_appears_when_state_and_mode_disagree() {
        assert!(daemon_note(CaptureMode::Agents, true).unwrap().contains("daemon stop"));
        assert!(daemon_note(CaptureMode::Full, false).unwrap().contains("daemon start"));
        assert_eq!(daemon_note(CaptureMode::Full, true), None);
        assert_eq!(daemon_note(CaptureMode::Agents, false), None);
    }
}
