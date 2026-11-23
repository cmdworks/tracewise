//! Which paths Tracewise does not record: what `.gitignore` (the root's and the ones in the folders on the way), `.git/info/exclude`,
//! the global git ignore file and `.tracewiseignore` say, plus the files the operating system writes on its own. A full scan
//! already honours them by walking the tree; the daemon, the git wrapper and the hooks name single paths, so they ask here.
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct IgnoreRules {
    root: PathBuf,
    dirs: HashMap<PathBuf, Option<Gitignore>>,
    exclude: Option<Gitignore>,
    global: Gitignore,
    tracewise: Option<Gitignore>,
    builtin: Option<Gitignore>,
    /// The Tracewise home, relative to the work tree, when it is inside it.
    own: Option<PathBuf>,
}

fn file_rules(dir: &Path, file: &Path) -> Option<Gitignore> {
    if !file.is_file() {
        return None;
    }
    let mut b = GitignoreBuilder::new(dir);
    b.add(file);
    b.build().ok()
}

impl IgnoreRules {
    pub fn load(work_tree: &Path) -> Self {
        Self::load_in(work_tree, &tracewise_platform::paths::tracewise_home_dir())
    }

    /// `load` with the Tracewise home given: when it lies inside the work tree (a project at the home folder) it is never recorded.
    pub fn load_in(work_tree: &Path, tracewise_home: &Path) -> Self {
        let common = work_tree.join(".git");
        let builtin = (!common.exists()).then(|| {
            let mut b = GitignoreBuilder::new(work_tree);
            for p in tracewise_platform::paths::BUILTIN_IGNORES {
                let _ = b.add_line(None, p);
            }
            b.build().ok()
        });
        Self {
            root: work_tree.to_path_buf(),
            dirs: HashMap::new(),
            exclude: file_rules(work_tree, &common.join("info").join("exclude")),
            global: Gitignore::global().0,
            tracewise: file_rules(work_tree, &work_tree.join(".tracewiseignore")),
            builtin: builtin.flatten(),
            own: tracewise_home.strip_prefix(work_tree).ok().filter(|r| !r.as_os_str().is_empty()).map(Path::to_path_buf),
        }
    }

    /// `rel` is relative to the work tree. A path under an ignored folder is ignored (`/target` hides `target/debug/x`).
    pub fn is_ignored(&mut self, rel: &Path) -> bool {
        // the root `.tracewiseproject` (tracewise_git::MARKER_FILE) names the project and is never recorded
        if tracewise_platform::paths::is_os_junk(rel)
            || rel == Path::new(".tracewiseproject")
            || self.own.as_deref().is_some_and(|o| rel.starts_with(o))
        {
            return true;
        }
        // the deepest .gitignore decides first, like git
        let mut dir = self.root.clone();
        let mut chain = vec![dir.clone()];
        if let Some(parent) = rel.parent() {
            for c in parent.components() {
                dir.push(c);
                chain.push(dir.clone());
            }
        }
        for d in chain.iter().rev() {
            let rules = self.dirs.entry(d.clone()).or_insert_with(|| file_rules(d, &d.join(".gitignore")));
            if let Some(g) = rules {
                let full = self.root.join(rel);
                match g.matched_path_or_any_parents(&full, false) {
                    ignore::Match::Ignore(_) => return true,
                    ignore::Match::Whitelist(_) => return false,
                    ignore::Match::None => {}
                }
            }
        }
        let full = self.root.join(rel);
        let mut in_tree = [self.exclude.as_ref(), self.tracewise.as_ref(), self.builtin.as_ref()].into_iter().flatten();
        in_tree.any(|g| matches!(g.matched_path_or_any_parents(&full, false), ignore::Match::Ignore(_)))
            // the global file has no root of its own: it takes the path relative to the work tree
            || matches!(self.global.matched_path_or_any_parents(rel, false), ignore::Match::Ignore(_))
    }
}

#[cfg(test)]
#[path = "test_ignore_rules.rs"]
mod tests;
