//! Installation and uninstallation logic for tracewise setup.
use super::*;

/// The login shell's name (`zsh` when `$SHELL` is not set).
pub(crate) fn user_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|s| s.rsplit('/').next().map(String::from))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "zsh".to_string())
}

/// A login shell Tracewise cannot set up (anything but zsh, bash and fish): the PATH and the terminal hook are not added.
pub(crate) fn unsupported_shell() -> Option<String> {
    Some(user_shell()).filter(|s| !["zsh", "bash", "fish"].contains(&s.as_str()))
}

/// fish reads every file in `conf.d`, so Tracewise owns one file there instead of editing `config.fish`.
pub(crate) fn fish_file(env: &Env) -> PathBuf {
    env.home.join(".config/fish/conf.d/tracewise.fish")
}

/// fish is the login shell, or its config folder exists.
pub(crate) fn fish_installed(env: &Env) -> bool {
    user_shell() == "fish" || env.home.join(".config/fish").is_dir()
}

/// The whole `conf.d/tracewise.fish`: PATH for every fish, and the terminal hook for interactive ones.
pub(crate) fn fish_block(env: &Env, terminal: bool) -> String {
    let root = root_text(env);
    let export = if root == "$HOME/.tracewise" { String::new() } else { format!("set -gx TRACEWISE_HOME \"{root}\"\n") };
    // without the terminal layer: the programs are on PATH (agent hooks call `tracewise`), and no shims, no terminal hook
    let (dirs, hook) =
        if terminal { (format!("\"{root}/shims\" \"{root}/bin\""), "if status is-interactive\n    tracewise shell init fish | source\nend\n") } else { (format!("\"{root}/bin\""), "") };
    format!(
        "{BLOCK_START}\n# managed by `tracewise setup`; `tracewise uninstall` removes this file\n{export}for d in {dirs}\n    contains -- $d $PATH; or set -gx PATH $d $PATH\nend\n{hook}{BLOCK_END}\n"
    )
}

/// Writes `conf.d/tracewise.fish` unless a file of the same name that is not ours is there. True when the file is Tracewise's.
pub(crate) fn write_fish_file(env: &Env, terminal: bool) -> Result<bool, String> {
    let path = fish_file(env);
    let old = fs::read_to_string(&path).unwrap_or_default();
    if !old.is_empty() && !old.contains(BLOCK_START) {
        return Ok(false); // the person's own file: never overwrite it
    }
    let new = fish_block(env, terminal);
    if old != new {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        fs::write(&path, new).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(true)
}

pub(crate) fn root_text(env: &Env) -> String {
    if env.root == env.home.join(".tracewise") {
        "$HOME/.tracewise".to_string()
    } else {
        env.root.to_string_lossy().to_string()
    }
}

/// For `.zshenv`: where the programs and shims are, for every shell.
pub(crate) fn env_block(env: &Env, shims: bool) -> String {
    let root = root_text(env);
    let export = if root == "$HOME/.tracewise" { String::new() } else { format!("export TRACEWISE_HOME=\"{root}\"\n") };
    // the programs are always on PATH (agent hooks call `tracewise`); the shims only with the terminal layer
    let dirs = if shims { format!("{root}/bin:{root}/shims") } else { format!("{root}/bin") };
    format!("{BLOCK_START}\n# managed by `tracewise setup`; `tracewise uninstall` removes this block\n{export}export PATH=\"{dirs}:$PATH\"\n{BLOCK_END}\n")
}

/// For `.zshrc`: the terminal integration, interactive shells only.
pub(crate) fn rc_block(env: &Env) -> String {
    let root = root_text(env);
    format!(
        "{BLOCK_START}\n# managed by `tracewise setup`; `tracewise uninstall` removes this block\ntypeset -U path\npath=(\"{root}/bin\" \"{root}/shims\" $path)\neval \"$(tracewise shell init zsh)\"\n{BLOCK_END}\n"
    )
}

/// Lines an older, hand-made Tracewise setup left in `.zshrc`.
pub(crate) fn is_legacy_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("# Tracewise git wrapper and shell integration")
        || (t.starts_with("export PATH=") && t.contains(".tracewise/bin"))
        || t.contains("tracewise shell init zsh")
}

/// `text` without the marked block and without legacy lines.
pub(crate) fn strip_tracewise(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim() == BLOCK_START {
            inside = true;
            continue;
        }
        if inside {
            if line.trim() == BLOCK_END {
                inside = false;
            }
            continue;
        }
        if is_legacy_line(line) {
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    if out.is_empty() {
        String::new()
    } else {
        out.join("\n") + "\n"
    }
}

/// `.zshrc` with Tracewise's lines replaced by the one block at the end.
pub fn rewrite_zshrc(text: &str, block: &str) -> String {
    let rest = strip_tracewise(text);
    if rest.is_empty() {
        block.to_string()
    } else {
        format!("{rest}\n{block}")
    }
}

/// The other names `tracewise` answers to.
pub(crate) const SHORT_NAMES: [&str; 3] = ["tw", "trace", "tracwise"];

/// True when a program called `name` is on `path` outside `own_bin` (our own links do not count).
pub(crate) fn name_is_taken(name: &str, own_bin: &Path, path: Option<&std::ffi::OsStr>) -> bool {
    let Some(path) = path else { return false };
    std::env::split_paths(path).filter(|d| d != own_bin).any(|d| d.join(name).is_file())
}

fn install_programs(env: &Env) -> Result<Vec<String>, String> {
    let bin = env.bin();
    fs::create_dir_all(&bin).map_err(|e| format!("cannot create {}: {e}", bin.display()))?;
    let mut done = Vec::new();
    for name in PROGRAMS {
        let src = env.src_dir.join(name);
        let dest = bin.join(name);
        if !src.is_file() {
            if *name == "tracewise" {
                return Err(format!("{} not found", src.display()));
            }
            continue; // tracewise-sensor may not be built
        }
        let same = src.canonicalize().ok() == dest.canonicalize().ok();
        if !same {
            let tmp = bin.join(format!(".{name}.new"));
            fs::copy(&src, &tmp).map_err(|e| format!("cannot copy {name}: {e}"))?;
            let _ = tracewise_platform::shims::set_executable(&tmp);
            fs::rename(&tmp, &dest).map_err(|e| format!("cannot install {name}: {e}"))?;
        }
        done.push(name.to_string());
    }
    // the checksums of a release candidate travel with the programs, so `tracewise doctor` can compare them later
    let sums = env.src_dir.join(CHECKSUM_FILE);
    if sums.is_file() {
        fs::copy(&sums, bin.join(CHECKSUM_FILE)).map_err(|e| format!("cannot copy {CHECKSUM_FILE}: {e}"))?;
    }
    let link = bin.join("git");
    tracewise_platform::shims::create_shim_link(&bin.join("tracewise-git"), &link).map_err(|e| format!("cannot link git: {e}"))?;
    // `tw`, `trace` and `tracwise` (the usual typo) are other names for `tracewise`; an alias is left out when a different program
    // of that name is already on PATH, because it would hide that tool
    for alias in SHORT_NAMES {
        if alias != "tw" && name_is_taken(alias, &bin, env.path.as_deref()) {
            eprintln!("tracewise: not linking `{alias}`: another program of that name is on your PATH");
            continue;
        }
        tracewise_platform::shims::create_shim_link(&bin.join("tracewise"), &bin.join(alias))
            .map_err(|e| format!("cannot link {alias}: {e}"))?;
    }
    Ok(done)
}

fn relink_cargo_copies(env: &Env) -> Vec<String> {
    let dir = env.home.join(".cargo/bin");
    let mut linked = Vec::new();
    for name in PROGRAMS {
        let p = dir.join(name);
        let target = env.bin().join(name);
        if p == target || !target.is_file() {
            continue;
        }
        let is_old_copy = p.is_file() && !p.is_symlink();
        let is_other_link = p.is_symlink() && p.canonicalize().ok() != target.canonicalize().ok();
        if is_old_copy || is_other_link {
            let _ = fs::remove_file(&p);
            if tracewise_platform::shims::create_shim_link(&target, &p).is_ok() {
                linked.push(p.to_string_lossy().to_string());
            }
        } else if p.is_symlink() {
            linked.push(p.to_string_lossy().to_string());
        }
    }
    linked
}

pub fn setup_in(env: &Env, opts: &SetupOptions) -> Result<Manifest, String> {
    tracewise_platform::private::create_dir_all(&env.root).map_err(|e| format!("cannot create {}: {e}", env.root.display()))?;
    let previous = read_manifest(env);

    let programs = install_programs(env)?;
    let shims = crate::cmd_shims::install_shims(&env.shims(), &env.bin().join("tracewise-shim"))?.len();

    let cfg_path = env.root.join("config.toml");
    // the existing settings stay: only the profile and, when it was chosen, the capture mode are set
    let existing = fs::read_to_string(&cfg_path).unwrap_or_default();
    let mut cfg_text = crate::cmd_config::set_value(&existing, "profile.name", &opts.profile);
    if let Some(mode) = opts.capture {
        cfg_text = crate::cmd_config::set_value(&cfg_text, "capture.mode", mode.name());
    }
    let mode = crate::cmd_config::read_value(&cfg_text, "capture.mode").and_then(|v| tracewise_core::capture::CaptureMode::parse(&v));
    let agents_only = mode == Some(tracewise_core::capture::CaptureMode::Agents);
    if fs::read_to_string(&cfg_path).ok().as_deref() != Some(&cfg_text) {
        tracewise_platform::private::write(&cfg_path, &cfg_text).map_err(|e| format!("cannot write {}: {e}", cfg_path.display()))?;
    }

    let mut backup = previous.as_ref().and_then(|m| m.zshrc_backup.clone());
    if opts.shell_rc {
        // the terminal layer: the shims on PATH and the hook in `.zshrc`; without it only the programs are on PATH
        let mut files = vec![(env.zshenv(), env_block(env, opts.terminal)), (env.zshrc(), if opts.terminal { rc_block(env) } else { String::new() })];
        if user_shell() == "bash" {
            // bash has no terminal hook yet, only the PATH; a login shell reads .bash_profile, else .profile
            let login = [".bash_profile", ".profile"]
                .iter()
                .map(|n| env.home.join(n))
                .find(|p| p.is_file())
                .unwrap_or_else(|| env.home.join(".bash_profile"));
            files.push((env.home.join(".bashrc"), env_block(env, opts.terminal)));
            files.push((login, env_block(env, opts.terminal)));
        }
        for (path, block) in files {
            let old = fs::read_to_string(&path).unwrap_or_default();
            // an empty block takes Tracewise's lines out of the file and adds none
            let new = if block.is_empty() { strip_tracewise(&old) } else { rewrite_zshrc(&old, &block) };
            if new != old {
                if !old.is_empty() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("rc");
                    let b = env.home.join(format!("{name}.tracewise-backup-{}", tracewise_core::clock::now().format("%Y%m%d-%H%M%S")));
                    fs::write(&b, &old).map_err(|e| format!("cannot back up {name}: {e}"))?;
                    backup = Some(b.to_string_lossy().to_string());
                }
                fs::write(&path, new).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            }
        }
    }
    if opts.shell_rc && fish_installed(env) {
        write_fish_file(env, opts.terminal)?;
    }
    let zshrc_block = opts.shell_rc && opts.terminal;

    // agents mode runs nothing in the background, so there is nothing to start at login
    let login = match opts.login {
        _ if agents_only => false,
        Login::Ask => crate::ui::confirm("Start the Tracewise daemon at login?"),
        Login::Yes => true,
        Login::No => false,
    };
    let plist = env.agent_plist();
    let launch_agent = if login {
        login_item_write(env)?;
        if opts.live {
            stop_manual_daemon(env);
            std::thread::sleep(std::time::Duration::from_millis(500));
            login_item_load(env);
        }
        Some(plist.to_string_lossy().to_string())
    } else {
        if plist.is_file() {
            remove_launch_agent(env, opts.live);
        }
        None
    };

    let m = Manifest {
        version: env!("CARGO_PKG_VERSION").to_string(),
        installed_at: tracewise_core::clock::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        profile: opts.profile.clone(),
        programs,
        shims,
        zshrc_block,
        zshenv_block: opts.shell_rc,
        zshrc_backup: backup,
        launch_agent,
        cargo_links: relink_cargo_copies(env),
    };
    write_manifest(env, &m)?;

    if opts.live && opts.start && !agents_only && !crate::util::is_daemon_running() {
        let _ = Command::new(env.bin().join("tracewise")).args(["daemon", "start"]).output();
    }
    Ok(m)
}

/// Takes Tracewise's lines out of the shell startup files (and fish's file): what `setup` wrote there, nothing else. Returns what was
/// changed.
pub(crate) fn remove_shell_blocks(env: &Env) -> Vec<String> {
    let mut removed = Vec::new();
    let rc_files = [env.zshrc(), env.zshenv(), env.home.join(".bashrc"), env.home.join(".bash_profile"), env.home.join(".profile")];
    for path in rc_files {
        if let Ok(old) = fs::read_to_string(&path) {
            let new = strip_tracewise(&old);
            if new != old {
                // a file that held only Tracewise's block goes away instead of staying empty
                let _ = if new.trim().is_empty() { fs::remove_file(&path) } else { fs::write(&path, new) };
                removed.push(format!("Tracewise lines in {}", path.display()));
            }
        }
    }
    // the fish file is wholly Tracewise's when it carries the block marker
    if fs::read_to_string(fish_file(env)).is_ok_and(|t| t.contains(BLOCK_START)) && fs::remove_file(fish_file(env)).is_ok() {
        removed.push(format!("Tracewise file {}", fish_file(env).display()));
    }
    removed
}

pub(crate) fn remove_launch_agent(env: &Env, live: bool) {
    if live {
        login_item_unload(env);
    }
    let _ = fs::remove_file(env.agent_plist());
}

/// `purge` also deletes the Tracewise home except the project histories in `projects/`; `purge_histories` deletes those too.
pub fn uninstall_in(env: &Env, purge: bool, purge_histories: bool, live: bool) -> Vec<String> {
    let mut removed = Vec::new();
    if live {
        if let Some(pid) = fs::read_to_string(env.root.join("daemon.pid")).ok().and_then(|t| t.trim().parse::<i32>().ok()) {
            let _ = tracewise_platform::signal::terminate_daemon(pid);
        }
    }
    if env.agent_plist().is_file() {
        remove_launch_agent(env, live);
        removed.push(env.agent_plist().to_string_lossy().to_string());
    }
    removed.extend(remove_shell_blocks(env));
    if env.shims().is_dir() {
        let n = crate::cmd_shims::uninstall_shims(&env.shims(), None);
        removed.push(format!("{n} shims"));
        let _ = fs::remove_dir(env.shims());
    }
    for l in read_manifest(env).map(|m| m.cargo_links).unwrap_or_default() {
        if Path::new(&l).is_symlink() && fs::remove_file(&l).is_ok() {
            removed.push(l);
        }
    }
    let programs = read_manifest(env).map(|m| m.programs).unwrap_or_else(|| PROGRAMS.iter().map(|s| s.to_string()).collect());
    for name in programs.iter().map(String::as_str).chain(["git", CHECKSUM_FILE]).chain(SHORT_NAMES) {
        let p = env.bin().join(name);
        if p.is_symlink() || p.is_file() {
            let _ = fs::remove_file(&p);
            removed.push(p.to_string_lossy().to_string());
        }
    }
    let _ = fs::remove_dir(env.bin());
    for f in ["install.json", "config.toml", "daemon.pid", "daemon.log", "shim.log", "sensor.sock", "sensor.status.json", "sensor-events"] {
        let _ = fs::remove_file(env.root.join(f));
    }
    // hooks `agent install --scope user` (or `init`) wrote in the home folder: they would call a program that is gone
    if !env.home.as_os_str().is_empty() {
        for (name, cfg) in &crate::cmd_agents::AGENT_CONFIGS {
            let had = fs::read_to_string(env.home.join(cfg)).is_ok_and(|t| t.contains("tracewise hook"));
            if crate::cmd_agents::remove_hooks_and_skill(&env.home, name, cfg).is_ok() && had {
                removed.push(format!("Tracewise hooks in {}", env.home.join(cfg).display()));
            }
        }
    }
    if purge_histories {
        let _ = fs::remove_dir_all(&env.root);
        removed.push(env.root.to_string_lossy().to_string());
    } else if purge {
        // the histories of every project live in projects/, owned by this device's key and pushed under its name: only
        // --purge-histories deletes them
        for e in fs::read_dir(&env.root).into_iter().flatten().flatten() {
            if !["projects", "device.key", "device"].iter().any(|k| e.file_name() == *k) {
                let p = e.path();
                let _ = if p.is_dir() && !p.is_symlink() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
            }
        }
        let _ = fs::remove_dir(&env.root);
        removed.push(format!("{} (except projects/, device.key and device)", env.root.display()));
    } else {
        let _ = fs::remove_dir(&env.root);
    }
    removed
}
