# Tracewise for VS Code

Shows who changed each line, and why: the history [Tracewise](https://cmdworks.github.io/tracewise/) records for you, your tools and your coding agents.

- **Blame at the end of each line**: who (you, `claude`, `codex`, ...), when, and the reason, for every line Tracewise has seen change. Hover for the full reason.
- **Your saves are recorded**: when you save a file, the extension tells Tracewise, so your own edits are recorded as yours even without a background watcher.
- **Status bar**: how many changes still have no reason; click to see them.
- **Undo a change** from the line it wrote, with a confirmation first. Your later edits to other lines stay.

It works from any folder inside a project, not only the one you opened.

## You need Tracewise

The extension is a window onto the `tracewise` program; it holds no history itself. [Install Tracewise](https://github.com/cmdworks/tracewise#download), run `tracewise init` in your project, and open the folder. The extension asks the program what it can do and says when it is too old (it needs the `json-contract` and `record` features).

## Settings

| Setting | |
|---|---|
| `tracewise.path` | the program, if it is not on your `PATH`. A machine setting: a project's own `.vscode/settings.json` cannot change it. |
| `tracewise.recordSaves` | tell Tracewise about every save (default on) |
| `tracewise.blame.enabled` | show the end-of-line labels (default on) |

## What it runs, and what it never does

In a folder you have not trusted it does nothing: it runs a program, and through it git, which reads a folder's own settings.

It runs `tracewise -C <the file's folder> <command> --json` for a short list of commands (`blame`, `why`, `log`, `status`, `pending`, `record`, `undo`, `show`, `companion`) and nothing else. It never builds a command from file contents, sends nothing over the network and has no telemetry.

GPL-3.0-or-later.
