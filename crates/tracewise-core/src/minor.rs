//! What counts as a minor change, in one place: the editor's question, `record` and (later) `gc` all use it.
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

/// How big one change is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeStats {
    pub added_lines: usize,
    pub removed_lines: usize,
    pub added_chars: usize,
    pub removed_chars: usize,
    /// Only whitespace differs (indentation, trailing spaces, blank lines).
    pub whitespace_only: bool,
}

impl ChangeStats {
    pub fn lines(&self) -> usize {
        self.added_lines + self.removed_lines
    }
    pub fn chars(&self) -> usize {
        self.added_chars + self.removed_chars
    }
}

/// Above this many characters changed, a line-level count is used instead of a character diff (which is quadratic).
const CHAR_DIFF_LIMIT: usize = 20_000;

/// The size of the change from `before` to `after`.
pub fn stats(before: &str, after: &str) -> ChangeStats {
    let mut s = ChangeStats::default();
    let diff = TextDiff::from_lines(before, after);
    let (mut gone, mut came) = (String::new(), String::new());
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => {
                s.removed_lines += 1;
                gone.push_str(change.value());
            }
            ChangeTag::Insert => {
                s.added_lines += 1;
                came.push_str(change.value());
            }
            ChangeTag::Equal => {}
        }
    }
    s.whitespace_only = before != after && before.split_whitespace().eq(after.split_whitespace());
    if gone.len() + came.len() <= CHAR_DIFF_LIMIT {
        // the characters that really changed inside the changed lines: a 3-character edit in a long line is 3, not the line
        let inner = TextDiff::from_chars(gone.as_str(), came.as_str());
        for change in inner.iter_all_changes() {
            match change.tag() {
                ChangeTag::Delete => s.removed_chars += change.value().chars().count(),
                ChangeTag::Insert => s.added_chars += change.value().chars().count(),
                ChangeTag::Equal => {}
            }
        }
    } else {
        s.removed_chars = gone.chars().count();
        s.added_chars = came.chars().count();
    }
    s
}

/// The thresholds below which a change is minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinorConfig {
    pub chars: usize,
    pub lines: usize,
    pub ignore_whitespace: bool,
}

impl Default for MinorConfig {
    fn default() -> Self {
        Self { chars: 20, lines: 1, ignore_whitespace: true }
    }
}

/// The automatic reason for a change too small to be worth asking about, or None when it is not minor. A created, deleted or renamed
/// file is never minor.
pub fn minor_reason(op: &str, s: &ChangeStats, cfg: &MinorConfig) -> Option<String> {
    if op != "modify" {
        return None;
    }
    if cfg.ignore_whitespace && s.whitespace_only {
        return Some("whitespace".to_string());
    }
    if s.chars() == 0 {
        return None;
    }
    if s.chars() <= cfg.chars && s.lines() <= cfg.lines.max(1) * 2 {
        let n = s.chars();
        return Some(format!("minor edit ({n} character{})", if n == 1 { "" } else { "s" }));
    }
    None
}

/// One change as far as folding is concerned.
#[derive(Debug, Clone)]
pub struct FoldItem {
    pub path: String,
    /// Who made it: two changes of one actor have the same key.
    pub actor: String,
    /// RFC 3339 with milliseconds, so that strings compare like times.
    pub time: String,
    /// Only an automatic "minor" reason, nothing a person wrote.
    pub minor: bool,
}

/// Runs of minor changes that can be shown as one line: two or more changes of one file, by one actor, each minor and older than
/// `cutoff`, with no other actor's change of that file in between. `items` is in any order but the same order the caller prints
/// (a run is reported by the indices of its members); nothing is changed or dropped, the rows stay in the history.
pub fn fold_runs(items: &[FoldItem], cutoff: &str) -> Vec<Vec<usize>> {
    let mut by_path: std::collections::BTreeMap<&str, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        by_path.entry(item.path.as_str()).or_default().push(i);
    }
    let mut runs = Vec::new();
    for indices in by_path.values() {
        let mut run: Vec<usize> = Vec::new();
        let flush = |run: &mut Vec<usize>, runs: &mut Vec<Vec<usize>>| {
            if run.len() >= 2 {
                runs.push(std::mem::take(run));
            } else {
                run.clear();
            }
        };
        for &i in indices {
            let item = &items[i];
            let foldable = item.minor && item.time.as_str() < cutoff;
            if foldable && run.last().is_none_or(|&last| items[last].actor == item.actor) {
                run.push(i);
            } else {
                flush(&mut run, &mut runs);
                if foldable {
                    run.push(i);
                }
            }
        }
        flush(&mut run, &mut runs);
    }
    runs.sort();
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_the_characters_that_changed_not_the_line() {
        let s = stats("let value = compute(a, b);\n", "let value = compute(a, c);\n");
        assert_eq!((s.added_lines, s.removed_lines), (1, 1));
        assert_eq!((s.added_chars, s.removed_chars), (1, 1));
        assert!(!s.whitespace_only);
    }

    #[test]
    fn whitespace_only_is_noticed() {
        let s = stats("fn a() {\n    x();\n}\n", "fn a() {\n  x();\n}\n\n");
        assert!(s.whitespace_only);
        assert_eq!(minor_reason("modify", &s, &MinorConfig::default()).as_deref(), Some("whitespace"));
    }

    #[test]
    fn a_small_edit_is_minor_and_a_big_one_or_a_new_file_is_not() {
        let cfg = MinorConfig::default();
        let tiny = stats("a\n", "ab\n");
        assert_eq!(minor_reason("modify", &tiny, &cfg).as_deref(), Some("minor edit (1 character)"));
        let big = stats("a\n", &"b".repeat(100));
        assert_eq!(minor_reason("modify", &big, &cfg), None);
        assert_eq!(minor_reason("create", &tiny, &cfg), None);
        assert_eq!(minor_reason("modify", &stats("a\n", "a\n"), &cfg), None, "no change is not minor, it is nothing");
    }

    fn item(path: &str, actor: &str, time: &str, minor: bool) -> FoldItem {
        FoldItem { path: path.into(), actor: actor.into(), time: time.into(), minor }
    }

    #[test]
    fn a_run_of_old_minor_changes_by_one_actor_folds_and_anything_else_splits_it() {
        let old = "2026-01-01T00:00:00.000Z";
        let cutoff = "2026-06-01T00:00:00.000Z";
        let items = vec![
            item("a.rs", "human:as", old, true),
            item("a.rs", "human:as", old, true),
            item("a.rs", "human:as", old, false), // has a person's reason: splits and is not folded
            item("a.rs", "human:as", old, true),  // a run of one stays as it is
            item("a.rs", "agent:claude", old, true),
            item("a.rs", "agent:claude", old, true),
            item("b.rs", "human:as", "2026-07-01T00:00:00.000Z", true), // newer than the cutoff
            item("b.rs", "human:as", "2026-07-01T00:00:00.000Z", true),
            item("b.rs", "human:as", old, true), // b.rs's other changes are not between these
            item("b.rs", "human:as", old, true),
        ];
        assert_eq!(fold_runs(&items, cutoff), vec![vec![0, 1], vec![4, 5], vec![8, 9]]);
        // folding another file's changes in between does not split a file's own run
        let mixed = vec![item("a.rs", "h", old, true), item("b.rs", "h", old, false), item("a.rs", "h", old, true)];
        assert_eq!(fold_runs(&mixed, cutoff), vec![vec![0, 2]]);
        assert!(fold_runs(&[], cutoff).is_empty());
    }
}
