//! A helper that exists once must stay once: a second copy of a function listed here fails this test.
use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn each_shared_helper_is_defined_in_one_place() {
    // (the text of a definition, the one file that may hold it)
    let once = [
        ("fn glob_match(", "tracewise-core/src/model.rs"),
        ("fn glob_match_anywhere(", "tracewise-core/src/model.rs"),
        ("fn ask_yes_no(", "never"),
        ("fn ask_line(", "never"),
        ("fn plural(", "tracewise-cli/src/ui.rs"),
        ("fn ask(", "tracewise-cli/src/ui.rs"),
        ("fn confirm(", "tracewise-cli/src/ui.rs"),
    ];
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for krate in fs::read_dir(crates).unwrap().flatten() {
        let src = krate.path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files);
        }
    }
    let mut problems = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        let rel = file.strip_prefix(crates).unwrap().to_string_lossy().replace('\\', "/");
        for (definition, allowed) in once {
            if text.lines().any(|l| l.trim_start().trim_start_matches("pub(crate) ").trim_start_matches("pub ").starts_with(definition)) && rel != allowed {
                problems.push(format!("{rel} defines `{definition}…` (only {allowed} may)"));
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
