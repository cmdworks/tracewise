# Configuration

Tracewise works without any configuration. This page covers what you can change.

## Where settings live

Settings are read from, strongest first:

1. environment variables (yours only — agents cannot use them to loosen anything);
2. the project file `.tracewiseconfig` in the repository root (commit it to share settings with your team);
3. the global file `~/.tracewise/config.toml`;
4. built-in defaults.

Both files use the same keys. The easiest way to read and change them:

```bash
tracewise config get                         # every setting and where its value comes from
tracewise config get reasons.mode
tracewise config set reasons.mode enforce    # in .tracewiseconfig
tracewise config set --global profile.name full
```

A file that does not parse is ignored, and `tracewise doctor` says so. Agents cannot edit either file.

## Profiles

A profile switches groups of features on. `core` is the default.

| Profile | Adds |
|---|---|
| `core` | the change log, `log` / `blame` / `why` / `show` / `diff`, reasons, `undo` / `redo` / `restore`, `verify`, agent hooks, watcher, shell hook, git wrapper |
| `coordination` | [locks](how-it-works.md#7-locks), the grace period after your saves, notices to agents about others' changes |
| `policy` | protected files, the commit check (no commit while agent changes lack a reason), and refusals when an agent tries to pose as a person |
| `strict` | the optional sensor that names the exact process behind every write (needs `sudo` and Full Disk Access) |
| `share` | [sharing](sharing.md) through a git remote |

```toml
[profile]
name = "core"          # core, or full for everything

# or switch single features on:
coordination = true
share = true
```

## Reasons

```toml
[reasons]
mode = "warn"          # off | warn | enforce
```

| Mode | An agent editing without a reason… |
|---|---|
| `off` | is not asked |
| `warn` *(default)* | may edit; the change is listed by `tracewise pending` |
| `enforce` | is denied, told how to give a reason, and cannot end its turn while changes lack one |

A reason must be at least 12 characters and not a placeholder. People are never forced.

## Capture

```toml
[capture]
mode = "full"          # full | agents
```

| Mode | What runs | What is recorded |
|---|---|---|
| `full` *(default)* | the background watcher | every change, from anything, with its time |
| `agents` | nothing in the background | agents (through their hooks), commands you type in a hooked shell, the command wrappers and git, as they happen. Edits you or other programs make in between are picked up by a quick scan the next time a command reads the history or an agent turn starts, and are stamped with the time they were found. |

What the catch-up finds is recorded as yours (`likely`, `source = catch-up`): nothing else saw the write, and agents that have hooks were recorded live. So a write by something no hook saw, such as an agent without hooks or a dev server an agent left running, is also recorded as yours; only the watcher can tell better. An agent that runs `tracewise log` does not trigger a catch-up; the next prompt does. If a scan is slow (over half a second), an agent's prompt looks at most once a minute and `tracewise doctor` says so. A folder that is an unadopted copy of another project never catches up (it would write the copy's files into the original's history); run `tracewise doctor --adopt-copy` first.

Set it with `tracewise config set capture.mode agents` (use `--global` for every project), or `tracewise setup --capture agents`. After switching to `agents`, run `tracewise capture daemon stop`. History is kept when you switch back and forth. `tracewise doctor` says what the current mode does not see live. A watcher that is set up but not running is caught up the same way. The catch-up looks at each file's size and modification time: over 20,000 files it takes about 75 ms in a release build (process start included), and about the same with a few files edited.

## Capture presets

A preset is a named setup of the two things Tracewise can switch on its own: the **terminal** layer (the command shims on your `PATH` and the terminal hook in your shell startup files, which make wrapped commands, the git wrapper and typed-command recording work) and the **daemon** layer (the background watcher, which also starts at login). Agent hooks are installed per agent (`tracewise agent install`) and an editor extension runs inside the editor, so both work with every preset.

| Preset | terminal | daemon | For |
|---|---|---|---|
| `agents` | off | off | agents and editors only; nothing in the background, your shell is left alone |
| `balanced` | on | off | agents and your terminal; no background process |
| `full` | on | on | everything, including changes made by any program |

```bash
tracewise capture                    # the current preset and each layer, and anything that disagrees
tracewise capture list               # the presets
tracewise capture use balanced       # apply one (asks first; --dry-run says what would change, --yes skips the question)
```

- **The current preset is never stored.** It is worked out from what is really installed, so `tracewise setup`, `tracewise capture use` and `tracewise config set capture.mode` can never disagree about it; a setup that matches no preset is shown as `custom`.
- **`capture.mode` follows the daemon layer.** Daemon on means `full`, off means `agents`; a hand-set mode that disagrees with the watcher is named by `tracewise capture`, and applying a preset fixes it.
- **Applying goes through `setup`.** `capture use` runs the same code as `tracewise setup`. When the terminal layer goes off, the shims and the terminal hook are taken out, but the programs stay on your `PATH` (agent hooks call `tracewise` by name), and so does your history.
- **The presets are data, not code.** The built-in ones are in `crates/tracewise-cli/presets.toml`; put a file in the same format at `~/.tracewise/presets.toml` to replace one by name or add your own. A layer a preset does not mention is left as it is, and a problem in your file (unknown layer, bad name, not valid TOML) is shown as a warning and never stops anything.

```toml
# ~/.tracewise/presets.toml
[preset.quiet]
description = "Terminal on, no watcher: what I use on battery."
layers = { terminal = true, daemon = false }
```

## Storage

```toml
[storage]
max_file = "5mb"       # larger files are recorded by hash only
secrets  = "hash"      # a text file holding a known credential is recorded by hash only; "store" keeps it

[durability]
level = "full"         # full: every record is on disk before it is reported; normal: slightly faster, a power cut may lose the newest records

[retention]
commands = "30d"       # how long recorded command lines are kept ("off" keeps them all); changes are never pruned
```

## Ignoring files

Tracewise never records a path that git ignores (every `.gitignore`, `.git/info/exclude`, your global git ignore) or that is OS clutter such as `.DS_Store`. To ignore more without touching `.gitignore`, add patterns to `.tracewiseignore` (same syntax):

```bash
tracewise ignore 'dist/' '*.log'
```

To remove history that was recorded before a path was ignored:

```bash
tracewise ignore --prune --dry-run
tracewise ignore --prune
```

`prune` keeps the history verifiable: `tracewise doctor --verify` still passes afterwards.

## Adding an agent

Claude Code, Codex, Cursor, Gemini and Antigravity are built in. Any other agent with a hook system can be added without rebuilding Tracewise: put a small adapter file `<name>.toml` in `.tracewise/adapters/` of the project or in `~/.tracewise/adapters/`. It says where the event name, session, tool and file path sit in the agent's hook payload, and how to answer. Adapter files are data and are never executed.

```bash
tracewise agent list                                 # built-in agents and adapter files found
tracewise agent check myagent.toml payload.json      # what a sample payload would become
tracewise agent install myagent
```

## Environment variables

| Variable | Meaning |
|---|---|
| `TRACEWISE_HOME` | the global folder (default `~/.tracewise`) |
| `TRACEWISE_PROFILE` | `core` or `full`, over the files |
| `TRACEWISE_REASONS` | reasons mode for this shell |
| `TRACEWISE_CAPTURE` | capture mode (`full` or `agents`) for this shell |
| `TRACEWISE_WHY` | the reason for the next command |
| `TRACEWISE_ACTOR` | who is acting, e.g. `script:nightly-build` |
| `TRACEWISE_LOCK_TTL_SECS` | lock lifetime (default 120) |
| `TRACEWISE_EDIT_GRACE_SECS` | how long agents wait after your save (default 20) |

The variables keep the `TRACEWISE_` prefix of Tracewise's earlier name.
