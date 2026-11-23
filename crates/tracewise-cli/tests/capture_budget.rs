//! How long the catch-up scan takes over 20,000 files. Run by hand, in a release build (a debug build is several times slower):
//!   cargo test --release -p tracewise --test capture_budget -- --ignored --nocapture
mod common;
use common::Repo;
use std::time::Instant;

/// The budget for one scan with nothing (or three files) changed, process start included.
const BUDGET_MS: u128 = 150;

fn timed(repo: &Repo, args: &[&str]) -> u128 {
    let start = Instant::now();
    assert!(repo.me(args).ok());
    start.elapsed().as_millis()
}

#[test]
#[ignore = "slow: creates 20,000 files; run by hand in a release build"]
fn catch_up_over_20000_files_is_fast() {
    let repo = Repo::uninitialised("");
    for d in 0..40 {
        for i in 0..500 {
            repo.write(&format!("src/d{d}/f{i}.txt"), &format!("line {i}\n"));
        }
    }
    assert!(repo.me(&["init"]).ok());
    let unchanged: Vec<u128> = (0..5).map(|_| timed(&repo, &["scan"])).collect();
    for f in ["src/d1/f1.txt", "src/d9/f9.txt", "src/d39/f499.txt"] {
        repo.write(f, "changed\n");
    }
    let three_edits = timed(&repo, &["scan"]);
    let log_after_edit = {
        repo.write("src/d2/f2.txt", "changed too\n");
        timed(&repo, &["log", "--limit", "1"])
    };
    println!("scan, nothing changed (5 runs, ms): {unchanged:?}");
    println!("scan, 3 files edited (ms): {three_edits}");
    println!("log, which catches up 1 edit first (ms): {log_after_edit}");
    if !cfg!(debug_assertions) {
        let slowest = unchanged.iter().copied().max().unwrap();
        assert!(slowest < BUDGET_MS, "a scan of an unchanged tree took {slowest} ms (budget {BUDGET_MS})");
        assert!(three_edits < BUDGET_MS * 2, "a scan with 3 edits took {three_edits} ms");
    }
}
