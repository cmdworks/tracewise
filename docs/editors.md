# Editors

Tracewise works from the command line and the desktop app; these add its history to your editor. Each one is a thin window onto the `tracewise` program: it holds no history, and it needs Tracewise [installed](getting-started.md) and `tracewise init` run in the project.

## VS Code (and Cursor, Windsurf, VSCodium, Antigravity)

The Tracewise extension shows, at the end of each line Tracewise has seen change, **who** changed it, **when** and **why**; hover for the full reason. It also:

- tells Tracewise when you save a file, so your own edits are recorded as yours even without a background watcher (`tracewise record`);
- shows in the status bar how many changes still have no reason, and lists them;
- undoes the change behind a line, after a confirmation. Only the lines that change wrote are taken back.

It works from any folder inside a project, not only the one you opened. Settings: `tracewise.path` (the program, if it is not on your `PATH`), `tracewise.recordSaves`, `tracewise.blame.enabled`. Source: `editors/vscode` in the repository. Until it is published to the Marketplace, build the `.vsix` (`pnpm package` there, or download it from the extension's CI run) and install it with `code --install-extension`. In a folder you have not trusted the extension does nothing: it runs a program, and through it git.

## Zed

Zed lets an extension add languages, themes and language servers, but not draw in the gutter or add a status bar item. So the Zed extension starts `tracewise lsp`, a language server: **hover** a line to see who changed it and why, and saving a file records it. Optionally turn on one hint at the end of each run of changed lines:

```json
{
  "lsp": { "tracewise": { "initialization_options": { "inlayBlame": true, "askWhy": true } } },
  "inlay_hints": { "enabled": true }
}
```

Source: `editors/zed`.


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

A change saved in an editor says which one: the hover shows `as via Zed`, `tracewise log` shows `you (likely) via Zed`, and the desktop app shows a small "via Zed" next to the name. The editor is taken from the name the editor gives the language server (Zed, VS Code, JetBrains), and the change's `source` in the JSON is `zed`, `vscode`, `jetbrains` or `lsp`.

### Saying why in Zed: two ways, your choice

- **Buttons after each save (default).** A popup asks "why did a.txt change?" with Same as last (once you have answered), Bug fix, Refactor, New feature, Type my own… and Don't ask again. Turn it off with `"askWhy": false` in `lsp.tracewise.initialization_options`.
- **Your own words in an editor tab.** The popup's "Type my own…" opens a small file (`why.md` in the project's Tracewise data folder, so nothing is added to your project). Write the reason, save it, and Tracewise stores it and empties the file. Needs a Zed that supports opening a file from a language server; if it does not, a Terminal window opens on macOS. Not yet confirmed in a real Zed: Zed may not send saves of a file outside the project to the server.
- **Your own words in Zed's terminal, when you choose.** Zed cannot show a text box or run a task on save, so use a task you start yourself. Run `tracewise editors zed --apply`: it adds this task to `~/.config/zed/tasks.json` and binds `cmd-alt-w` (change it with `--key`) in `keymap.json`. Without `--apply` it only prints them. A file with comments is left alone and the text to add by hand is shown. Then press the key (or `task: spawn`) and type the reason in Zed's terminal. It explains your latest change:

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

## Any editor with language-server support

`tracewise lsp` talks the Language Server Protocol on standard input and output. Point your editor's language-server setting at the command `tracewise lsp` for the file types you want. It offers hover (always), inlay hints (when the editor sends `{"inlayBlame": true}` as initialization options) and records a file when the editor reports it saved. With `{"askWhy": true}` it also asks the editor "why did this file change?" after a save that changed something, with the buttons Same as last, Bug fix, Refactor, New feature, Type my own… and Skip and Don't ask again; see `askWhen`; the chosen button becomes the reason of that change. The Zed extension turns this on by default; set `"askWhy": false` in `lsp.tracewise.initialization_options` to turn it off. It starts and stops with the editor, so it runs in the background only while the editor is open.

## Which one records what

| | Records your saves | Shows who and why |
|---|---|---|
| VS Code extension | on save | end of line, hover, status bar |
| `tracewise lsp` (Zed and others) | on save | hover, optional hints |
| Neither | the catch-up scan finds edits when a command reads the history ([capture modes](configuration.md#capture)) | `tracewise blame`, `why` |

A save made by an AI extension inside the editor looks like a save by you, and so do edits made by an agent that is not hooked into Tracewise (Zed's own agent panel, for example): both are recorded as yours, `likely`. If an agent that Tracewise hooks into is working at the same moment, the change is recorded as unclear (`ambiguous`) rather than guessed.

## When an editor does not behave

`tracewise lsp` writes a line per message (method, id, file; never contents) to the file in `TRACEWISE_LSP_LOG`, or to `~/.tracewise/lsp-debug.log` when that file exists (`touch` it to turn logging on, delete it to turn it off). Restart the editor after turning it on.
