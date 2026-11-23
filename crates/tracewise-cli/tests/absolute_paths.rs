//! An editor knows a file by its absolute path. Every command that takes a path accepts that, in either spelling of the folder
//! (on macOS `/var/...` and `/private/var/...` are the same place), and answers as it does for the relative path.
mod common;
use common::Repo;

fn repo() -> Repo {
    let repo = Repo::new("");
    repo.edit_as("agent:claude:s1", "src/a.txt", "one\ntwo\n");
    assert!(repo.run("agent:claude:s1", &["note", "--last", "write the first two lines of the file"]).ok());
    repo
}

/// The same folder, spelled with and without the `/private` that macOS puts in front of `/var`.
fn spellings(repo: &Repo, file: &str) -> Vec<String> {
    let as_given = repo.dir().join(file).display().to_string();
    let canonical = repo.dir().canonicalize().unwrap().join(file).display().to_string();
    if as_given == canonical {
        vec![as_given]
    } else {
        vec![as_given, canonical]
    }
}

#[test]
fn blame_with_an_absolute_path_is_the_same_as_with_a_relative_one() {
    let repo = repo();
    let relative = repo.me(&["blame", "src/a.txt", "--json"]).json();
    assert_eq!(relative[0]["actor"]["kind"], "agent", "the fixture itself: {relative}");
    for abs in spellings(&repo, "src/a.txt") {
        assert_eq!(repo.me(&["blame", &abs, "--json"]).json(), relative, "{abs}");
    }
}

#[test]
fn why_log_file_and_diff_take_absolute_paths_too() {
    let repo = repo();
    let rel_why = repo.me(&["why", "src/a.txt:2", "--json"]).json();
    let rel_log = repo.me(&["log", "src/a.txt", "--json"]).json();
    let rel_file = repo.me(&["status", "src/a.txt", "--json"]).json();
    for abs in spellings(&repo, "src/a.txt") {
        assert_eq!(repo.me(&["why", &format!("{abs}:2"), "--json"]).json(), rel_why, "why {abs}");
        assert_eq!(repo.me(&["log", &abs, "--json"]).json(), rel_log, "log {abs}");
        assert_eq!(repo.me(&["status", &abs, "--json"]).json(), rel_file, "file {abs}");
        assert!(repo.me(&["diff", &abs, "--since", "1h"]).ok(), "diff {abs}");
    }
}

#[test]
fn a_path_outside_the_project_is_left_as_it_is() {
    let repo = repo();
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("x.txt");
    std::fs::write(&file, "x\n").unwrap();
    // nothing is known about it: the answer is "no history", not a crash
    let run = repo.me(&["log", file.to_str().unwrap(), "--json"]);
    assert!(run.ok(), "{}", run.err);
    assert!(run.json().as_array().unwrap().is_empty());
}

#[test]
fn restore_takes_an_absolute_path_too() {
    let repo = repo();
    let first = repo.rows(&[]).into_iter().rev().find(|r| r["path"] == "src/a.txt").unwrap()["id"].as_str().unwrap().to_string();
    repo.later();
    repo.edit_as("human:me", "src/a.txt", "one\ntwo\nthree\n");
    for abs in spellings(&repo, "src/a.txt") {
        repo.edit_as("human:me", "src/a.txt", "one\ntwo\nthree\nfour\n");
        let run = repo.me(&["restore", &abs, "--at", &first]);
        assert!(run.ok(), "restore {abs}: {}", run.err);
        assert_eq!(repo.read("src/a.txt"), "one\ntwo\n", "{abs}");
    }
}
