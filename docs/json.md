# JSON output

Every command takes `--json` (`-j`) and prints one JSON document on standard output. Editors, the desktop app and scripts read it, so it is a **contract**: a key listed here is never removed and never changes its type. New keys may appear, so read the keys you need and ignore the rest. Tests in `crates/tracewise-cli/tests/json_contract.rs` pin the top-level keys and their types for each command; the nested keys listed below are the contract too, and are covered as far as the editor extensions' tests read them.

## Which `tracewise` is this? `companion`

```bash
tracewise companion --json
```

```json
{ "api": 1, "name": "tracewise", "version": "0.0.1-beta", "features": ["neutral-hook", "locks", "lock-list-json", "notices", "json-contract", "record"] }
```

A program that uses Tracewise should not compare version numbers. It checks `features` for the capability it needs (`record` for `tracewise record`, `json-contract` for this page) and tells the person to update when one is missing. `api` changes only for a break in a shape or a command.

## Errors

A command that cannot work exits with a non-zero code and prints the reason on standard error, not JSON. Exit code `2` means "not a Tracewise project here" (no `.git` or `.tracewiseproject` above the folder); run `tracewise -C <folder> ...` to pick the folder.

## `log` — an array of changes, newest first

| Key | Type | |
|---|---|---|
| `id` | string | the change id (26 characters) |
| `path` | string | relative to the project |
| `op` | string | `create`, `modify`, `delete`, `rename`, `chmod`, ... |
| `old_path` | string or null | for a `rename` |
| `actor` | object | `kind` is `human`, `agent`, `baseline`, `ambiguous` or `unknown`; `name` and `session` when known |
| `time` | object | `start` and `end`, ISO 8601 |
| `source` | string | what recorded it: `hook`, `scan`, `catch-up`, `editor`, `watcher`, ... |
| `confidence` | string | `exact`, `likely`, `ambiguous` or `unknown` |
| `candidates` | array | actors it could be when `actor.kind` is `ambiguous` |
| `hunks` | array | `old_start`, `old_lines`, `new_start`, `new_lines` |
| `hash_only` | bool | true when the content was too big or binary to keep |
| `before`, `after` | string or null | content hashes |
| `why` | string or null | the reason, when there is one |
| `why_auto` | boolean | the reason is an automatic one (a small change nobody was asked about, or a skipped question), not a person's words |

## `blame <path>` — an array with one row per line

`line` (number, from 1), `text`, `change` (id, or `baseline` for a line from before Tracewise), `actor`, `time` (string, empty for baseline lines), `why` (string or null), `source` (what recorded the change: `zed`, `vscode`, `jetbrains`, `lsp`, `scan`, `hook`, ...). Baseline lines also carry `git_author`.

## `why <path>:<line>`

`{ "change": { ...one `log` row... }, "history": [ ... ], "intent": ..., "line": 3 }`.

## `status`

`repo`, `data_dir`, `repo_id` (strings), `changes`, `locks` (numbers), `daemon` (`running` or `stopped`), `profile` (object), `share` (object or null), `capture` (`{ "mode": "full" | "agents", "preset": "agents" | "balanced" | "full" | ... | null }`).

## `doctor`

`ok` (bool), `problems`, `profile`, `attribution` (`changes`, `days`, `confidence`, `actor_kind`), `coverage`: `agents` (per agent: hooks installed), `shell`, `shims`, `git_wrapper`, `daemon` (bools), `capture` (`mode`, `live`, `caught_up`, and `last_catch_up_ms` / `prompt_catch_up_throttled` when a catch-up has run).

## `pending` — an array of changes with no reason yet

`id`, `path`, `actor`, `time`.

## `file <path>`

`path`, `changes` (an array of `log` rows), `lock` (null or who holds it).

## `scan` and `record <paths>`

`{ "changes": ["<id>", ...] }`: the ids of the changes just recorded (empty when nothing changed). `record` looks only at the files named; see [commands](commands.md). `record` also returns `"stats": [{ "id", "path", "op", "stats": { "added_lines", "removed_lines", "added_chars", "removed_chars", "whitespace_only" } | null }]`, one row per change: how big it is, so an editor can decide whether it is worth asking about (`stats` is `null` when the texts are not stored, for example a huge file). The keys of `changes` are unchanged.

## Writing a client

- Run `tracewise -C <folder of the open file> ...` so a file in a subfolder finds its project, and pass the file as an absolute path: every command that takes a path (`blame`, `why`, `log`, `file`, `diff`, `restore`, `record`) accepts one, in either spelling of the folder.
- Give every call a timeout, and show a quiet "not tracked" for exit code 2.
- Run only the commands you need; never build arguments from file contents.
