# Tracewise for Zed

Hover a line to see who changed it, when, and why, from the history [Tracewise](https://cmdworks.github.io/tracewise/) records for you, your tools and your coding agents. When you save a file, Tracewise is told, so your own edits are recorded as yours without a background watcher.


### What Zed lets an extension do (from Zed's docs, 2026-11-23)

- **Can:** add languages, themes, snippets, MCP servers and debuggers, and start a language server (our extension does this). A language server gets hover, code actions, inlay hints (a hint can carry a command) and diagnostics (shown underlined, on hover and in `diagnostics: deploy`). Settings under `lsp.tracewise` reach the server (`initialization_options` once at start, `settings` on request).
- **Cannot:** draw in the gutter, add a status-bar item or a panel, open Zed's terminal or a text box, or run a task when a file is saved (the only automatic task trigger is `create_worktree`). Tasks never prompt for input.
- **Not stated in Zed's docs, checked by hand instead:** whether a popup with buttons (`showMessageRequest`) is shown, whether code actions that run a command work, and code lens.
- Tasks come from `~/.config/zed/tasks.json`, a project's `.zed/tasks.json`, or a language that an extension ships. Our extension ships no language, so `tracewise editors zed --apply` writes the task into your own file.

### When the question appears: `askWhen`

Set it in `lsp.tracewise.initialization_options`. One name or a list (whichever happens first). Default: `pause`.

| `askWhen` | The popup appears |
|---|---|
| `"pause"` | once you have not saved for `askAfterSeconds` (default 0, so right after the save; set 10 and a burst of `cmd-s` becomes one question, asked when you stop) |
| `"every-save"` | on every save that changed something |
| `"interval"` | at once, then at most once per file every `askEverySeconds` (default 60); saves in between join the next question |
| `"leave"` | when you close the file |
| `"manual"` | never by itself: use the task key or `tracewise note` |

Example: `{ "askWhen": ["pause", "leave"], "askAfterSeconds": 5 }`. Buttons: Same as last, Bug fix, Refactor, New feature, Type my own…, **Skip** (no reason for these changes), Don't ask again (until the editor closes). Every save is recorded whatever you choose; only the question waits. Planned, not built yet: asking at `git commit`, one question for several files, only for large changes, and only when you edit lines an agent wrote.

`tracewise editors zed --settings` asks what you want (when the popup shows, how long to wait, the minimum change size, files to ignore, hints), prints the block, and offers to add it to `settings.json` (`--write`: comments stay, a backup `settings.json.tracewise-bak` is kept, and a file that already has `lsp.tracewise` is left alone) or to open that file in Zed (`--open`). Give the options as flags (`--when pause,leave --after 5 --min-chars 20 --ignore '*.md' --hints`) to skip the questions.

### Only ask about changes that matter

Every change is recorded, even one space. These options (same place, all optional) decide which ones get a question; the rest get an **automatic reason** instead, so nothing is left as a silent gap, and a reason you write later replaces it.

| Option | Meaning | Default |
|---|---|---|
| `askMinChars` | no question when fewer characters changed (added + removed) | 0 (ask) |
| `askMinLines` | no question when fewer lines changed | 0 (ask) |
| `askIgnoreWhitespace` | no question for whitespace-only changes | `true` |
| `askIgnore` | file patterns never asked about (`*.md`, `dist/**`), added to a built-in list: lock files, `target/`, `node_modules/`, `dist/`, `.git/`, minified files | none |

Automatic reasons: `minor edit (3 characters)`, `whitespace`, `not asked (ignored file)`, `skipped` (the Skip button) and `no reason given` (the popup was closed). A created, deleted or renamed file is never treated as minor. `tracewise editors zed --settings --min-chars 20 --ignore '*.md'` prints the block. Not built yet: counting small saves together (`askMinTotal*`), `askOnlyLanguages`, `askMaxPerHour`, and folding runs of minor edits into one line in `log` (planned in `.plan/2026-11-23-minor-changes-plan.md`).

### Saying why in Zed: two ways, your choice

- **Buttons after each save (default).** A popup asks "why did a.txt change?" with Same as last (once you have answered), Bug fix, Refactor, New feature, Type my own… and Don't ask again. Turn it off with `"askWhy": false` in `lsp.tracewise.initialization_options`.
- **Your own words in an editor tab.** The popup's "Type my own…" opens a small file (`why.md` in the project's Tracewise data folder, so nothing is added to your project). Write the reason, save it, and Tracewise stores it and empties the file. Needs a Zed that supports opening a file from a language server; if it does not, a Terminal window opens on macOS. Not yet confirmed in a real Zed: Zed may not send saves of a file outside the project to the server.
- **Your own words in Zed's terminal, when you choose.** Zed cannot show a text box or run a task on save, so use a task you start yourself. Add this to `~/.config/zed/tasks.json`, run it with `task: spawn`, and type the reason in the terminal. It explains your latest change:

```json
[
  {
    "label": "Tracewise: why",
    "command": "sh",
    "args": ["-c", "printf 'why? '; read r; tracewise note \"$(tracewise log -l 1 | cut -d' ' -f1)\" \"$r\""],
    "use_new_terminal": true,
    "reveal": "always"
  }
]
```

Bind a key if you like, in `keymap.json`: `{ "bindings": { "cmd-alt-w": ["task::Spawn", { "task_name": "Tracewise: why" }] } }`. Use both, or either.

## You need Tracewise

The extension starts `tracewise lsp`, a language server that is part of the `tracewise` program, found on your `PATH`. [Install Tracewise](https://github.com/cmdworks/tracewise#download) and run `tracewise init` in your project.

## What Zed lets an extension do

Zed extensions add languages, themes and language servers; they cannot draw in the gutter or add a status bar item. So this is a language server: hover, and optionally one hint at the end of each run of changed lines. Turn the hints on in your Zed settings:

```json
{
  "lsp": { "tracewise": { "initialization_options": { "inlayBlame": true, "askWhy": true } } },
  "inlay_hints": { "enabled": true }
}
```

It starts for the languages listed in `extension.toml`; to add one, add its name there (or list `"tracewise"` in that language's `language_servers` in your settings).

## Develop

`zed: install dev extension` and pick this folder (it needs the `wasm32-wasip2` target: `rustup target add wasm32-wasip2`). Logs: `zed: open log`, or run `zed --foreground`.

GPL-3.0-or-later.
