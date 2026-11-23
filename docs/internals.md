# Entry points for other programs

These commands are called by programs, not typed by people, so `tracewise --help` does not list them. They keep their names because hooks, shell blocks, wrappers and editors that are already installed call them by name. Their output and arguments are part of the contract in [JSON output](json.md) where it says so.

| Command | Called by | Does |
|---|---|---|
| `tracewise hook <event> --from <agent>` | an agent's hook | records what the agent is about to do, and answers the agent (`--neutral` for a program that nests Tracewise) |
| `tracewise shell init\|preexec\|precmd\|begin\|end` | the shell block `setup` adds | records the commands you type |
| `tracewise git-wrapper …`, `tracewise git-hook <name>`, `tracewise git-hooks install` | the `git` program Tracewise puts first on `PATH`, and the hooks `init` installs in a git project | marks checkouts, merges, pulls, resets and the like as one git change |
| `tracewise daemon run\|start\|stop\|status\|scan-paths\|active\|sweep` | `tracewise-watchd`, `setup`, `capture` | the background watcher; use `tracewise capture daemon …` yourself |
| `tracewise strict …` | `setup --strict` | the optional sensor; use `tracewise capture sensor …` yourself |
| `tracewise shims install\|uninstall` | `setup` | the command shims |
| `tracewise proc …`, `tracewise commands` | the other Tracewise programs | process identity, the recorded command list |
| `tracewise companion` | a program that nests Tracewise | the API version and features |
| `tracewise lsp` | an editor | the language server (see [editors](editors.md)) |
| `tracewise record <paths…>` | an editor extension | records files that were just saved |
