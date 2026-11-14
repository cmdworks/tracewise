//! `tracewise hook --neutral`: the same hook, answered in a neutral shape instead of one agent's schema. For a program that nests Tracewise
//! inside its own hook (AutoMemory): it passes the agent's raw payload through, gets one decision and some context lines back, and renders
//! the merged answer in the agent's schema itself. Exit code 0 always; anything unexpected is an allow.
use super::{adapter, antigravity, claude, execute_hook, HookInput, HookOutput};
use serde_json::Value;
use tracewise_git::RepoPaths;

/// Version of this shape. A caller that does not know the number must not use the answer.
pub const API: u32 = 1;

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct Neutral {
    pub api: u32,
    /// The Tracewise event the payload stood for, or `none`.
    pub event: String,
    /// `allow`, `ask` or `deny`.
    pub decision: &'static str,
    pub reason: Option<String>,
    /// Lines for the agent to be told (what other actors changed in files it works on, an overlap, a lock warning).
    pub context: Vec<String>,
}

impl Neutral {
    pub fn allow(event: &str) -> Neutral {
        Neutral { api: API, event: event.to_string(), decision: "allow", reason: None, context: Vec::new() }
    }
}

/// A Tracewise answer in the neutral shape.
pub fn from_output(event: &str, res: HookOutput) -> Neutral {
    let decision = match res.decision {
        "deny" | "block" => "deny",
        "ask" => "ask",
        _ => "allow",
    };
    let mut context: Vec<String> = Vec::new();
    if let Some(notice) = res.notice {
        match event {
            // the warning a held file earns: told to the agent that is about to edit it
            "pre-edit" | "pre-command" if notice.starts_with(crate::hook::events::pre_edit::WARN_PREFIX) => context.push(notice),
            "prompt" if crate::hook::events::awareness::is_complete(&notice) => context.push(notice),
            "prompt" => context.push(format!("Tracewise: since your last prompt, other actors changed files you work on:\n{notice}")),
            _ => {}
        }
    }
    Neutral { api: API, event: event.to_string(), decision, reason: res.reason.filter(|_| decision != "allow"), context }
}

/// Runs the hook for a raw payload from `from` and returns the neutral answer.
pub fn handle(event: &str, from: Option<&str>, stdin: &str) -> Neutral {
    let val: Value = serde_json::from_str(stdin).unwrap_or(Value::Null);
    let step: Option<(String, HookInput)> = match from {
        Some(agent @ ("claude" | "codex" | "cursor" | "gemini")) => claude::translate(agent, event, &val).map(|(e, i)| (e.to_string(), i)),
        Some("antigravity") => antigravity::translate(event, &val).map(|(e, i)| (e.to_string(), i)),
        Some(name) => {
            let work_tree = std::env::current_dir().ok().and_then(|d| RepoPaths::discover(&d)).map(|r| r.work_tree);
            match adapter::find(name, work_tree.as_deref()) {
                Ok(Some((a, _))) => {
                    let tr = a.translate(event, &val);
                    tr.event.map(|e| {
                        (
                            e,
                            HookInput {
                                agent: Some(a.name.clone()),
                                session: Some(tr.session),
                                paths: tr.paths,
                                command: tr.command,
                                cwd: tr.cwd,
                            },
                        )
                    })
                }
                _ => None,
            }
        }
        None => None,
    };
    let Some((ev, input)) = step else { return Neutral::allow("none") };
    let res = execute_hook(&ev, input).unwrap_or(HookOutput { decision: "allow", reason: None, notice: None });
    from_output(&ev, res)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(decision: &'static str, reason: Option<&str>, notice: Option<&str>) -> HookOutput {
        HookOutput { decision, reason: reason.map(String::from), notice: notice.map(String::from) }
    }

    #[test]
    fn deny_and_block_are_deny_and_the_reason_only_travels_with_a_refusal() {
        assert_eq!(from_output("pre-edit", out("deny", Some("held by codex"), None)).decision, "deny");
        assert_eq!(from_output("stop", out("block", Some("two changes lack a reason"), None)).decision, "deny");
        let allowed = from_output("pre-edit", out("allow", Some("ignored"), None));
        assert_eq!((allowed.decision, allowed.reason), ("allow", None));
    }

    #[test]
    fn a_prompt_notice_becomes_context_with_its_header_and_a_complete_one_is_kept_as_it_is() {
        let plain = from_output("prompt", out("allow", None, Some("codex modify a.txt [id]")));
        assert_eq!(plain.context.len(), 1);
        assert!(plain.context[0].starts_with("Tracewise: since your last prompt") && plain.context[0].contains("a.txt"));
        let digest = from_output("prompt", out("allow", None, Some("Tracewise digest: since claude last worked here, 2 files changed")));
        assert_eq!(digest.context, vec!["Tracewise digest: since claude last worked here, 2 files changed".to_string()]);
        assert!(from_output("post-edit", out("allow", None, Some("anything"))).context.is_empty());
    }

    #[test]
    fn an_unknown_payload_is_an_allow_with_no_event() {
        assert_eq!(handle("", None, "not json"), Neutral::allow("none"));
        assert_eq!(handle("PreToolUse", Some("claude"), "{}"), Neutral::allow("none"));
    }
}
