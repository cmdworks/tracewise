# Getting started

This guide takes you from nothing to a project whose every change is recorded, explained and reversible. It takes about five minutes.

## 1. Install

Tracewise runs on macOS (Apple silicon and Intel).

**From a release:**

```bash
tar -xzf tracewise-0.0.1-beta-macos-arm64.tar.gz    # or -x86_64 on an Intel Mac
cd tracewise-0.0.1-beta-macos-arm64
./tracewise setup
```

**From source** (needs a current stable Rust from [rustup](https://rustup.rs)):

```bash
git clone https://github.com/cmdworks/tracewise
cd tracewise
cargo build --release
./target/release/tracewise setup
```

`setup` opens with a short menu: how should Tracewise watch your work? **agents** (agents and editors only; nothing runs in the background), **balanced** (also the commands you type and git; the recommended choice) or **full** (also a background watcher for changes by any program). Give the answer up front with `--preset agents|balanced|full`; without a terminal `setup` asks nothing and keeps your earlier choice. You can change it any time with `tracewise capture use <preset>` (see [capture presets](configuration.md#capture-presets)). If you pick `full` it also asks whether the watcher should start at login. When it is done it prints a checklist of what it did and what to do next. It:

- copies the programs to `~/.tracewise/bin`, with `tw`, `trace` and `tracwise` as other names for `tracewise` (every example below works with either);
- adds a `git` wrapper and a few command wrappers (`sed`, `mv`, `cp`, `rm`, `tee`) there, so writes made through them are attributed;
- adds one clearly marked block to `~/.zshenv` and `~/.zshrc` (your other lines are kept; a backup is written);
- starts the background watcher (not in `agents` mode).

It never asks for `sudo`. Open a new terminal and check that everything is in place:

```bash
tracewise doctor --all
```

<a id="unsigned-binaries"></a>
> **Unsigned binaries.** Release binaries are not yet signed by Apple. If macOS refuses to run them, remove the download flag once: `xattr -dr com.apple.quarantine tracewise-*`.

## 2. Start recording a project

Tracewise works in any folder, with or without git:

```bash
cd ~/code/my-project
tracewise init
```

`init` records the current state of every tracked file as the *baseline* and registers the project with the watcher. From now on every change is recorded, whoever makes it. The history lives in `~/.tracewise/projects/<id>/`; the only thing added to your folder is a small `.tracewiseproject` marker holding the project id. The first `init` prints a recovery secret once: keep it in a password manager.

## 3. Connect your coding agents

Agents with hooks tell Tracewise exactly which file they are about to edit, in which session, and why. `init` already set them up for the agents it found in your home folder: Claude Code gets hooks for your user (`~/.claude/settings.json`, so it is covered in every folder and subfolder); Codex, Cursor, Gemini and Antigravity get hooks in the project, as before. To add an agent later, install its hooks in the project:

```bash
tracewise agent install claude       # Claude Code
tracewise agent install codex        # also: cursor, gemini, antigravity
```

Use `--scope user` to install them for your user instead (every project); for agents other than Claude Code, check that the agent loads its user-level hooks file. Agents without hooks are still recorded by the watcher, with less certainty about who made the change. Other agents can be added with a small adapter file — see [Configuration](configuration.md#adding-an-agent).

## 4. See what happened

```bash
tracewise log                        # newest changes first
tracewise log --actor claude --since 2h
tracewise log --group                # changes grouped by task
tracewise show <change id>           # one change with its diff
tracewise blame src/app.rs           # who last changed each line, and why
tracewise why src/app.rs:42          # the full story of one line
tracewise diff src/app.rs --since 1h # what a file looked like an hour ago, as a diff
```

Every command accepts `--json` for scripts and tools.

## 5. Say why

Agents declare a reason before editing; Tracewise can require it (see [reasons](configuration.md#reasons)). Agents and people can also do it by hand:

```bash
tracewise note -m "fix the empty-input crash" --before src/parser.rs
tracewise run -m "update dependencies" -- cargo update
mv parser.rs lexer.rs --note "the lexer is its own module now"   # sed, mv, cp, rm and tee take --note too
tracewise pending                    # changes that have no reason yet
tracewise note --last "rename the config keys"
```

For people this is never required. When you commit, the commit message becomes the reason of your changes that have none.

## 6. Go back

Undo works on lines, not whole files: only the lines a change wrote are taken back, and later edits by others stay.

```bash
tracewise undo <change id>                     # one change
tracewise undo --actor codex --since 30m       # everything an agent did in the last half hour
tracewise undo --intent <intent id>            # one whole task
tracewise undo <change id> --dry-run           # show what would happen
tracewise undo --redo <undo id>                     # changed your mind
tracewise restore src/app.rs --at <change id>  # put a whole file back as it was
```

An undo is itself recorded, so nothing is ever lost.

## 7. Keep it healthy

```bash
tracewise status          # this repository: changes, locks, watcher
tracewise doctor          # checks this repository; --all checks the installation
tracewise doctor --verify # checks that no record was altered or removed
tracewise doctor --gc     # packs stored versions; run now and then
tracewise ignore build/   # stop recording a path (writes .tracewiseignore)
```

## Removing Tracewise

```bash
tracewise uninstall
```

This removes exactly what `setup` installed. The history of each project stays in its `~/.tracewise/projects/<id>/` folder; delete that folder to remove it too.

## Next

- [How it works](how-it-works.md) — what happens behind each of these commands.
- [Commands](commands.md) — the full list.
- [Sharing](sharing.md) — the same history on every machine of your team.
