use std::path::{Path, PathBuf};
use std::process;

#[cfg(test)]
use tracewise_shim::find_real_tool_inner;
use tracewise_shim::{exec_if_non_utf8, find_real_tool, has_why_flag, is_read_only, shim_guard_applies};

use tracewise_platform::locate::find_tracewise_bin;
#[cfg(test)]
use tracewise_platform::locate::find_tracewise_bin_inner;

fn exec_tool(target: &Path, args: &[String], arg0: Option<&str>, set_depth: bool) -> ! {
    let env_set = if set_depth { Some(("TRACEWISE_SHIM_DEPTH", "1")) } else { None };
    let env_remove = if !set_depth { Some("TRACEWISE_SHIM_DEPTH") } else { None };
    tracewise_platform::process::fast_exec(target, args, arg0, env_set, env_remove);
}

fn main() {
    let os_args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let args: Vec<String> = os_args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let prog = if !args.is_empty() { &args[0] } else { "tool" };
    let tool = Path::new(prog).file_name().and_then(|n| n.to_str()).unwrap_or("");

    if tool == "tracewise-shim" || tool.is_empty() {
        eprintln!("tracewise-shim: fast tiny dispatcher for command shims");
        process::exit(0);
    }

    exec_if_non_utf8(tool, os_args.get(1..).unwrap_or(&[]), Path::new(prog).parent());
    let tool_args = if args.len() > 1 { &args[1..] } else { &[] };

    // 1. Loop guard: if TRACEWISE_SHIM_DEPTH is set, exec system tool directly
    let parent = tracewise_platform::process::exe(tracewise_platform::process::parent_pid());
    if shim_guard_applies(std::env::var_os("TRACEWISE_SHIM_DEPTH").is_some(), parent.as_deref(), std::env::current_exe().ok().as_deref()) {
        let fallback = PathBuf::from(format!("/usr/bin/{}", tool));
        let target = if fallback.is_file() { fallback } else { PathBuf::from(format!("/bin/{}", tool)) };
        exec_tool(&target, tool_args, Some(tool), true);
    }

    let shim_dir = Path::new(prog).parent();
    let real_tool = find_real_tool(tool, shim_dir);

    // 2. Read-only fast path (< 1 ms): exec real tool directly
    // a command with a reason is handed to tracewise, which takes the reason off before the real tool sees the arguments
    if !has_why_flag(tool_args) && is_read_only(tool, tool_args) {
        exec_tool(&real_tool, tool_args, Some(tool), true);
    }

    // 3. Mutating path: exec tracewise binary with argv[0] = prog
    if let Some(tracewise_bin) = find_tracewise_bin() {
        exec_tool(&tracewise_bin, tool_args, Some(prog), false);
    }

    // Fallback: if tracewise not found, fail open
    exec_tool(&real_tool, tool_args, Some(tool), true);
}

#[cfg(test)]
#[path = "test_main.rs"]
mod tests;
