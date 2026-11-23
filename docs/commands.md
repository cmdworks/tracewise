# Commands

`tracewise` also answers to `tw`, `trace` and `tracwise` (the usual typo): `tw log` is `tracewise log`. Every command accepts `--json` (`-j`) for scripts and tools, and `-C <dir>` to run as if started in another folder. `tracewise <command> --help` shows all options with examples. Commands that other programs call (hooks, wrappers, the watcher, editors) are listed in [entry points](internals.md).

Change ids are the 26-character ids shown by `log` (for example `01M6ZGHB9C46J2SNR4HTTJV60E`). Times accept durations like `30m`, `2h`, `7d`, or a date and time.

## Setup

| Command | |
|---|---|
| `tracewise setup [--preset agents\|balanced\|full]` | install for this user (no `sudo`). In a terminal it asks how much to watch from a menu; `--preset` skips the question (see [capture presets](configuration.md#capture-presets)). `--capture agents\|full` still works, `--login yes\|no` decides whether the watcher starts at login |
| `tracewise uninstall [--purge] [--purge-histories]` | remove what `setup` installed and the agents' user-scope hooks; `--purge` also deletes `~/.tracewise` except the project histories in `~/.tracewise/projects`, `--purge-histories` deletes those too |
| `tracewise init` | start recording the project here (the nearest folder above with `.git` or `.tracewiseproject`, else this folder; a linked worktree uses its main checkout's project); installs Claude Code's hooks for your user and the other agents' hooks in the project; refuses the home folder and an unadopted copy |
| `tracewise doctor [--all]` | check this repository, or the whole installation |
| `tracewise status` | this repository at a glance |

## Agents

| Command | |
|---|---|
| `tracewise hook <event> --from <agent> [--neutral]` | what an agent's hook runs; `--neutral` answers in an agent-independent shape for a program that nests Tracewise inside its own hook |
| `tracewise companion [--json]` | the API version and features a nesting program checks before it uses Tracewise |
| `tracewise agent install <name> [--scope project\|user]` | install hooks for `claude`, `codex`, `cursor`, `gemini`, `antigravity` or an adapter file |
| `tracewise agent uninstall <name> [--scope project\|user]` | remove them |
| `tracewise agent status` | which agents are connected here |
| `tracewise agent list` | built-in agents and adapter files |
| `tracewise jobs [--stop <id>]` | work that agents started in the background; `--stop` ends the attribution of later writes to one |

## Looking at history

| Command | |
|---|---|
| `tracewise log [path]` | changes, newest first. Filters: `--actor`, `--since`, `--grep`, `--limit`; `--group` groups them by task. A run of minor edits older than a day (automatic reasons such as `whitespace`, `minor edit (3 characters)`) is shown as one `fold` line; `--unfold` (`-u`) shows every row, `--fold-after <duration>` changes the age (`0` folds all). Nothing is deleted, and `--json` always lists every row |
| `tracewise show <id>` | one change with its diff |
| `tracewise blame <path> [--at <id>]` | who last changed each line, and why |
| `tracewise why <path>:<line>` | the full story of one line |
| `tracewise diff [path] --since <time>` | what changed since then, as a diff |
| `tracewise status <path>` | who holds a file and who changed it lately (`--limit n`) |
| `tracewise commands` | recorded commands |
| `tracewise log --md [--session <id> \| --intent <id>]` | one task as Markdown, for a review or a hand-off |
| `tracewise lsp` | a language server on standard input and output, for editors without a Tracewise extension: hover says why a line changed, a save is recorded. See [editors](editors.md) |
| `tracewise record [--source <name>] <paths…>` | record the named files now, for an editor that saved them (short name `rec`); only those files are looked at. Every `--json` answer is described in [JSON output](json.md) |

## Reasons

| Command | |
|---|---|
| `tracewise note -m "<reason>" --before <paths…>` | declare why these files are about to change (`-m`, `--note` or `--why`); `tracewise note --list` shows the active ones |
| `tracewise run -m "<reason>" -- <command>` | run a command under a reason |
| `mv a b --note "<reason>"` | the same for `sed`, `mv`, `cp`, `rm` and `tee` once their wrappers are on your `PATH`: the reason goes anywhere after the command, as `--note "<reason>"` or `--note=<reason>` (`--why` works too; there is no short form, it would clash with the tool's own options), and is taken off before the real tool runs |
| `tracewise pending` | changes without a reason |
| `tracewise note <id> "<reason>"` | add a reason to one change |
| `tracewise note --last "<reason>"` | add it to all of your changes that lack one |

## Going back

| Command | |
|---|---|
| `tracewise undo <id>` | take back one change, line by line |
| `tracewise undo --actor <who> --since <time>` | take back everything someone did in a time window |
| `tracewise undo --intent <id>` / `--session <id>` / `--command <id>` | take back a task, a session, or one bulk command |
| `tracewise undo --redo <undo id>` | put back what an undo removed |
| `tracewise restore <path> --at <id or time>` | put a whole file back as it was |

`undo` accepts `--dry-run`; `undo --strict` refuses files edited since.

## Locks

Needs the `coordination` profile.

| Command | |
|---|---|
| `tracewise lock list` | current locks |
| `tracewise lock release <path>` / `--all` | release a lock (an agent can release only its own) |
| `tracewise lock wait <path>` | wait until a file is free |

## Sharing

| Command | |
|---|---|
| `tracewise share init [--remote URL] [--level summary\|full\|off] [--encrypt]` | turn sharing on for the project |
| `tracewise share trust` | approve the remote named in `.tracewiseconfig` |
| `tracewise share status` | how sharing is set up here |
| `tracewise share push` / `tracewise share fetch` | send / import history |
| `tracewise share key export\|import\|list\|rotate\|rekey` | manage the encryption key |
| `tracewise share keys [--forget <device>]` | the signing keys pinned for each machine |

## Maintenance

| Command | |
|---|---|
| `tracewise doctor --verify` | check that no record was altered or removed |
| `tracewise doctor --gc` | pack stored versions and clean up |
| `tracewise doctor --rebuild` | rebuild the database from the batches |
| `tracewise ignore <patterns…>` | stop recording paths (writes `.tracewiseignore`) |
| `tracewise ignore --prune [--dry-run]` | remove the history of ignored paths (`--path`, `--older-than`, `--yes`, `--backup`) |
| `tracewise config get [key]` / `set [--global] <key> <value>` | read or change settings |
| `tracewise completions zsh\|bash\|fish` | print Tab completion for your shell (`tracewise completions zsh > ~/.zfunc/_tracewise`) |
| `tracewise capture repos` | repositories the watcher follows, with sizes |
| `tracewise capture [list \| use <preset>]` | which layers record live, and named presets of them (`agents`, `balanced`, `full`; yours in `~/.tracewise/presets.toml`); see [configuration](configuration.md#capture-presets) |
| `tracewise capture daemon start\|stop\|status` | the background watcher |
| `tracewise capture sensor …` | the optional exact-attribution sensor |
| `tracewise doctor --adopt-copy` | give a copied repository folder its own identity |

## Short forms

*Why* is a question and *note* is an answer: `tracewise why` asks why a line changed, and a reason you write is a *note* (`tracewise note`, `--note`, `-m`). `--why` still works wherever `--note` does.

Every command has a short name (`tracewise l` is `tracewise log`), and the common options have a short letter that means the same thing in every command.

| Command | Short | | Command | Short | | Command | Short |
|---|---|---|---|---|---|---|---|
| `init` | `i` | | `blame` | `b` | | `note` | `n` |
| `status` | `st` | | `why` | `w` | | `run` | `r` |
| `doctor` | `dr` | | `diff` | `d` | | `pending` | `p` |
| `log` | `l` | | `undo` | `u` | | `config` | `cfg` |
| `show` | `sw` | | | | | `daemon` | `dm` |
| `restore` | `rs` | | | | | `scan` | `sc` |

| Option | Short | Commands |
|---|---|---|
| `--json` | `-j` | all |
| `--note` (`--why`) | `-m` | `run`, `note --before` |
| `--actor` | `-a` | `log`, `undo` |
| `--since` | `-s` | `log`, `diff`, `undo` |
| `--session` | `-S` | `log --md`, `undo` |
| `--intent` | `-i` | `log --md`, `undo` |
| `--command` | `-c` | `undo` |
| `--limit` | `-l` | `log`, `status <path>` |
| `--grep` | `-g` | `log` |
| `--group` | `-G` | `log` |
| `--at` | `-t` | `blame`, `restore` |
| `--dry-run` | `-n` | `undo`, `ignore --prune` |
| `--yes` | `-y` | `ignore --prune` |
| `--path` | `-p` | `ignore --prune` |
| `--older-than` | `-o` | `ignore --prune` |
| `--backup` | `-b` | `ignore --prune` |
| `--all` | `-a` | `doctor` |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | success |
| `1` | the command failed, or a check (`verify`, `doctor`) found a problem |
| `2` | wrong usage |
