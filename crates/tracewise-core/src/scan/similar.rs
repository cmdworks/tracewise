//! A file that vanished and another that appeared in the same scan, mostly made of the same lines, is a rename with an edit
//! (`mv` then change a line), the way git pairs them. Exact copies are paired by hash before this runs; here only the rest is compared.
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// At least this share (percent) of the larger file's non-blank lines must be in both.
pub(crate) const MIN_SHARED_PERCENT: usize = 50;
/// Files with fewer non-blank lines are never paired by similarity: a few lines in common prove nothing.
pub(crate) const MIN_LINES: usize = 3;
/// Bigger files, and scans with more candidate pairs than this (a formatter or a bulk move), stay a delete and a create:
/// the scan has to stay cheap.
pub(crate) const MAX_BYTES: usize = 1 << 20;
pub(crate) const MAX_PAIRS: usize = 10_000;

/// The non-blank lines of one text file, counted by content.
pub(crate) struct Profile {
    lines: HashMap<u64, usize>,
    count: usize,
}

impl Profile {
    /// `None` for binary, oversized and very short files.
    pub(crate) fn of(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_BYTES || bytes.contains(&0) {
            return None;
        }
        let mut lines = HashMap::new();
        let mut count = 0;
        for line in String::from_utf8_lossy(bytes).lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut h = DefaultHasher::new();
            line.hash(&mut h);
            *lines.entry(h.finish()).or_insert(0) += 1;
            count += 1;
        }
        (count >= MIN_LINES).then_some(Self { lines, count })
    }

    /// Percent of the larger file's lines that both files hold.
    pub(crate) fn shared_percent(&self, other: &Self) -> usize {
        let (small, big) = if self.lines.len() <= other.lines.len() { (self, other) } else { (other, self) };
        let common: usize = small.lines.iter().map(|(h, n)| (*n).min(big.lines.get(h).copied().unwrap_or(0))).sum();
        common * 100 / self.count.max(other.count)
    }
}

/// `(old index, new index)` pairs, best match first, each side used at most once. A `None` profile is never paired.
pub(crate) fn best_pairs(old: &[Option<Profile>], new: &[Option<Profile>]) -> Vec<(usize, usize)> {
    if old.len() * new.len() > MAX_PAIRS {
        return Vec::new();
    }
    let mut scored = Vec::new();
    for (i, o) in old.iter().enumerate() {
        let Some(o) = o else { continue };
        for (j, n) in new.iter().enumerate() {
            let Some(n) = n else { continue };
            let shared = o.shared_percent(n);
            if shared >= MIN_SHARED_PERCENT {
                scored.push((shared, i, j));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let (mut used_old, mut used_new) = (vec![false; old.len()], vec![false; new.len()]);
    let mut pairs = Vec::new();
    for (_, i, j) in scored {
        if !used_old[i] && !used_new[j] {
            used_old[i] = true;
            used_new[j] = true;
            pairs.push((i, j));
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Option<Profile> {
        Profile::of(text.as_bytes())
    }

    #[test]
    fn short_binary_and_blank_files_have_no_profile() {
        assert!(p("one\ntwo\n").is_none());
        assert!(p("\n\n\n\n\n").is_none());
        assert!(Profile::of(&[0u8, 1, 2, 3, 4]).is_none());
    }

    #[test]
    fn shared_percent_counts_the_larger_files_lines() {
        let a = p("a\nb\nc\nd\n").unwrap();
        let b = p("a\nb\nc\nx\n").unwrap();
        assert_eq!(a.shared_percent(&b), 75);
        let c = p("a\nb\nc\n").unwrap();
        assert_eq!(a.shared_percent(&c), 75, "3 of the larger file's 4 lines");
        let d = p("x\ny\nz\n").unwrap();
        assert_eq!(a.shared_percent(&d), 0);
    }

    #[test]
    fn indentation_and_blank_lines_do_not_matter() {
        let a = p("fn a() {\n    x\n}\n\n").unwrap();
        let b = p("fn a() {\n\tx\n}\n").unwrap();
        assert_eq!(a.shared_percent(&b), 100);
    }

    #[test]
    fn each_file_is_paired_once_best_match_first() {
        let old = [p("a\nb\nc\nd\n"), p("a\nb\nc\ne\n")];
        let new = [p("a\nb\nc\nd\n"), p("q\nr\ns\nt\n")];
        // old 0 matches new 0 exactly; old 1 would also match new 0 (75%) but new 0 is taken, and nothing else is close
        assert_eq!(best_pairs(&old, &new), vec![(0, 0)]);
    }

    #[test]
    fn too_many_candidates_are_left_alone() {
        let one = || p("a\nb\nc\n");
        let old: Vec<_> = (0..101).map(|_| one()).collect();
        let new: Vec<_> = (0..100).map(|_| one()).collect();
        assert!(best_pairs(&old, &new).is_empty());
    }
}
