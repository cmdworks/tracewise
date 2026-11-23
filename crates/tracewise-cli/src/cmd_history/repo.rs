//! Repository lifecycle and maintenance: init, status, scan, gc, verify.
use super::*;

/// `recovery_secret` is `Some` only on the run that created the project: it is shown this once and never stored.
pub(crate) fn run_init(json: bool, repo_paths: &RepoPaths, info: &crate::project::ProjectInfo) {
    let secret = info.recovery_secret.as_deref();
    // shown before anything else can fail: it exists nowhere else
    if let Some(secret) = secret.filter(|_| !json) {
        println!("Recovery secret: {secret}");
        println!("Keep it in a password manager; it is shown only this once.");
    }
    let fail = |msg: String| -> ! {
        eprintln!("tracewise: {msg}");
        if let Some(secret) = secret.filter(|_| json) {
            eprintln!("Recovery secret: {secret} (keep it in a password manager; it is shown only this once)");
        }
        process::exit(1);
    };
    let data_dir = repo_paths.data_dir();
    if let Err(e) = fs::create_dir_all(&data_dir) {
        fail(format!("failed to create data directory: {e}"));
    }

    // before the baseline, so the files it writes in the project (AGENTS.md, project-scope hooks) are part of it
    let agent_files = crate::cmd_agents::install_for_init(repo_paths, &crate::cmd_agents::home_dir());

    let files = walk_baseline_files(&repo_paths.work_tree);
    let (branch, head) = get_git_info(repo_paths);
    if let Err(e) = record_baseline(&repo_paths.work_tree, &data_dir, &branch, &head, &files) {
        fail(format!("baseline recording failed: {e}"));
    }
    // the daemon watches every registered repository: tell it about this one
    if tracewise_core::repos::register(&repo_paths.work_tree, &data_dir) {
        tracewise_core::repos::wake_daemon();
    }

    if json {
        let mut doc = serde_json::json!({
            "id": info.id,
            "agent_files": agent_files,
            "repo": repo_paths.work_tree.to_string_lossy(),
            "data_dir": data_dir.to_string_lossy(),
            "files": files.len(),
        });
        if let Some(secret) = secret {
            doc["recovery_secret"] = secret.into();
        }
        println!("{}", doc);
    } else {
        println!("Initialized tracewise in {} ({} baseline files)", data_dir.display(), files.len());
        for f in &agent_files {
            println!("Wrote {f}");
        }
    }
}

pub(crate) fn run_status(json: bool, repo_paths: &RepoPaths) {
    let db_path = repo_paths.data_dir().join("tracewise.db");
    let (change_count, lock_count) = if db_path.is_file() {
        Database::open(&db_path).map(|db| (db.count_changes().unwrap_or(0), db.count_active_locks().unwrap_or(0))).unwrap_or((0, 0))
    } else {
        (0, 0)
    };

    let share_cfg = tracewise_core::share::read_tracewiseconfig(&repo_paths.work_tree);
    let share_json = if let Some(ref s) = share_cfg {
        serde_json::json!({
            "remote": s.remote,
            "level": s.level,
        })
    } else {
        serde_json::Value::Null
    };

    let watcher_running = is_daemon_running();
    let daemon_status = if watcher_running { "running" } else { "stopped" };
    let mode = tracewise_core::capture::capture_mode(&repo_paths.work_tree);

    let profile = tracewise_core::profile::ProfileConfig::from_root(&repo_paths.work_tree);
    // the capture preset the installation matches (see `tracewise capture`), or none
    let preset = {
        let env = crate::setup::Env::current();
        let (presets, _) = crate::capture_preset::load(&env.root.join("presets.toml"));
        crate::capture_preset::current(&presets, &crate::capture_preset::probe(&env)).map(|p| p.name.clone())
    };

    if json {
        let doc = serde_json::json!({
            "repo": repo_paths.work_tree.to_string_lossy(),
            "data_dir": repo_paths.data_dir().to_string_lossy(),
            "changes": change_count,
            "locks": lock_count,
            "daemon": daemon_status,
            "capture": { "mode": mode.name(), "preset": preset },
            "repo_id": tracewise_core::identity::repo_id(
                &repo_paths.data_dir(),
                &main_work_tree(repo_paths),
            ),
            "share": share_json,
            "profile": {
                "name": profile.description(),
                "core": profile.core,
                "coordination": profile.coordination,
                "policy": profile.policy,
                "strict": profile.strict,
                "share": profile.share,
            },
        });
        println!("{}", doc);
    } else {
        // the same words and columns when piped; the labels and the state only get colour on a terminal
        let label = |l: &str| crate::ui::paint(crate::ui::DIM, l);
        let state = if watcher_running { crate::ui::paint(crate::ui::GREEN, daemon_status) } else { crate::ui::paint(crate::ui::YELLOW, daemon_status) };
        println!("{} {}", label("repo:    "), crate::ui::paint(crate::ui::BOLD, &repo_paths.work_tree.display().to_string()));
        println!("{} {}", label("data_dir:"), repo_paths.data_dir().display());
        println!("{} {}", label("profile: "), profile.description());
        println!("{} {}", label("changes: "), change_count);
        println!("{} {}", label("locks:   "), lock_count);
        match crate::capture_report::daemon_note(mode, watcher_running) {
            Some(note) => println!("{} {state} ({note})", label("daemon:  ")),
            None => println!("{} {state}", label("daemon:  ")),
        }
        println!("{} {}", label("capture: "), mode.name());
        println!("{} {}", label("preset:  "), preset.as_deref().unwrap_or("custom"));
        if let Some(ref s) = share_cfg {
            println!("{} {} ({})", label("share:   "), s.remote, s.level);
        } else {
            println!("{} null", label("share:   "));
        }
    }
}

pub(crate) fn run_scan(json: bool, repo_paths: &RepoPaths) {
    let files = walk_baseline_files(&repo_paths.work_tree);
    let (branch, head) = get_git_info(repo_paths);
    let options = ScanOptions::default();
    match scan_repository(&repo_paths.work_tree, &repo_paths.data_dir(), &branch, &head, &options, &files) {
        Ok(changes) => {
            if json {
                let doc = serde_json::json!({ "changes": changes });
                println!("{}", doc);
            } else if changes.is_empty() {
                println!("No changes detected.");
            } else {
                println!("Recorded {} change(s):", changes.len());
                for id in changes {
                    println!("  {id}");
                }
            }
        }
        Err(e) => {
            eprintln!("tracewise: scan failed: {e}");
            process::exit(1);
        }
    }
}

/// `tracewise record <paths>`: an editor says these files were saved. Only they are looked at; a path outside the project, one
/// that git ignores, or one that is not there and was never tracked is skipped without an error.
pub(crate) fn run_record(json: bool, repo_paths: &RepoPaths, source: String, paths: Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    let rel_paths: Vec<std::path::PathBuf> = paths
        .iter()
        .filter_map(|p| util::normalize_and_validate_path(p, &repo_paths.work_tree))
        .filter(|p| seen.insert(p.clone()))
        .map(std::path::PathBuf::from)
        .collect();
    let (branch, head) = get_git_info(repo_paths);
    // the person saved it, as far as anyone can tell: an agent window that is open at the same time makes it ambiguous, not the agent's
    let options = ScanOptions {
        source,
        actor: Some(util::human_actor_from_system()),
        confidence: Some("likely".to_string()),
        ..ScanOptions::default()
    };
    let changes = if rel_paths.is_empty() {
        Vec::new()
    } else {
        tracewise_core::scan_specific_paths(&repo_paths.work_tree, &repo_paths.data_dir(), &branch, &head, &options, &rel_paths)
            .unwrap_or_else(|e| {
                eprintln!("tracewise: record failed: {e}");
                process::exit(1);
            })
    };
    if json {
        println!("{}", serde_json::json!({ "changes": changes, "stats": change_stats(repo_paths, &changes) }));
    } else if changes.is_empty() {
        println!("Nothing to record.");
    } else {
        println!("Recorded {} change(s).", changes.len());
    }
}

pub(crate) fn run_gc(json: bool, repo_paths: &RepoPaths) {
    let data_dir = repo_paths.data_dir();
    let db_path = data_dir.join("tracewise.db");
    let mut db_gc_report = None;
    if db_path.exists() {
        if let Ok(db) = Database::open(&db_path) {
            if let Ok(report) = db.gc(Some(&repo_paths.work_tree)) {
                db_gc_report = Some(report);
            }
        }
    }

    let blobs = BlobStore::new(&data_dir);
    let packed = blobs.gc();
    tracewise_platform::private::strip_tree(&data_dir); // git's pack files are read-only for everyone, not private
    match packed {
        Ok(mut report) => {
            if let Some(dbrep) = db_gc_report {
                report.commands_pruned = Some(dbrep.commands_pruned);
            }
            if json {
                println!("{}", serde_json::to_string(&report).unwrap_or_default());
            } else {
                println!("Packed {} object(s): {} bytes -> {} bytes", report.packed, report.bytes_before, report.bytes_after);
                if let Some(n) = report.commands_pruned {
                    if n > 0 {
                        println!("Pruned {n} expired command record(s)");
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("tracewise: gc failed: {e}");
            process::exit(1);
        }
    }
}

pub(crate) fn run_verify(json: bool, repo_paths: &RepoPaths) {
    let db_path = repo_paths.data_dir().join("tracewise.db");
    if !db_path.exists() {
        if repo_paths.data_dir().join("chain-head").exists() {
            eprintln!("tracewise: FAILED: tracewise.db is missing but its chain-head file exists; the history was deleted");
        } else {
            eprintln!("tracewise: not initialized; run tracewise init first");
        }
        process::exit(1);
    }
    let db = match Database::open(&db_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("tracewise: failed to open db: {e}");
            process::exit(1);
        }
    };
    let report = match db.verify_hash_chain() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("tracewise: verify error: {e}");
            process::exit(1);
        }
    };
    let missing = match db.changes_with_missing_blobs(&tracewise_core::blob::BlobStore::new(&repo_paths.data_dir())) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("tracewise: verify error: {e}");
            process::exit(1);
        }
    };
    if json {
        let mut value = serde_json::to_value(&report).unwrap_or_default();
        value["missing_blobs"] = serde_json::json!(missing);
        println!("{value}");
    } else if report.ok && missing.is_empty() {
        println!("ok ({} changes verified)", report.changes);
    } else if report.ok {
        eprintln!("FAILED (chain ok, but {} changes point to a missing file version, first: {})", missing.len(), missing[0]);
    } else {
        eprintln!("FAILED ({} changes verified, first bad: {})", report.changes, report.first_bad.as_deref().unwrap_or("unknown"));
    }
    if !report.ok || !missing.is_empty() {
        process::exit(1);
    }
}

/// How big each recorded change is (lines and characters added and removed, whitespace only), for an editor deciding whether a change
/// is worth asking about. A change whose texts are not stored (a huge or binary file) has `null` sizes.
fn change_stats(repo_paths: &RepoPaths, ids: &[String]) -> Vec<serde_json::Value> {
    let Ok(db) = Database::open(&repo_paths.data_dir().join("tracewise.db")) else { return Vec::new() };
    let blobs = tracewise_core::blob::BlobStore::new(&repo_paths.data_dir());
    let text = |hash: &Option<String>| -> Option<String> {
        match hash {
            None => Some(String::new()),
            Some(h) => blobs.read(h).ok().and_then(|b| String::from_utf8(b).ok()),
        }
    };
    ids.iter()
        .filter_map(|id| {
            let c = db.get_change(id).ok().flatten()?;
            let stats = text(&c.before).zip(text(&c.after)).map(|(b, a)| tracewise_core::minor::stats(&b, &a));
            Some(serde_json::json!({ "id": c.id, "path": c.path, "op": c.op, "stats": stats }))
        })
        .collect()
}
