---
title: Download
---

# Download

Tracewise is a command line, with an optional desktop app and editor extensions. macOS is supported now; Linux and Windows are planned.

## The program and the app

| OS | Command line (binary) | Desktop app (UI, with the menu bar) |
|---|---|---|
<!-- downloads:start -->
| **macOS, Apple silicon** | [tracewise-0.0.1-beta-macos-arm64.tar.gz](https://github.com/cmdworks/tracewise/releases/download/v0.0.1-beta/tracewise-0.0.1-beta-macos-arm64.tar.gz) | [Tracewise 0.0.1-beta (arm64).dmg](https://github.com/cmdworks/tracewise/releases/download/v0.0.1-beta/tracewise-desktop-0.0.1-beta-macos-arm64.dmg) (preview) |
| **macOS, Intel** | [tracewise-0.0.1-beta-macos-x86_64.tar.gz](https://github.com/cmdworks/tracewise/releases/download/v0.0.1-beta/tracewise-0.0.1-beta-macos-x86_64.tar.gz) | [Tracewise 0.0.1-beta (x86_64).dmg](https://github.com/cmdworks/tracewise/releases/download/v0.0.1-beta/tracewise-desktop-0.0.1-beta-macos-x86_64.dmg) (preview) |
| **Linux** | planned (build from source: `cargo build --release`) | planned |
| **Windows** | planned | planned |
<!-- downloads:end -->

The desktop app needs the command line and offers to install it on first start. Everything is on the [releases page](https://github.com/cmdworks/tracewise/releases).

## One line

Downloads the newest release, checks it, then runs `tracewise setup`, which asks how much to watch (agents, balanced or full). No `sudo`.

```bash
curl -fsSL https://raw.githubusercontent.com/cmdworks/tracewise/main/scripts/install.sh | sh
```

Then, in a project: `tracewise init`. See [getting started](./docs/getting-started).

## Editor extensions

They call the command line, so install it first.

| Editor | What you get | Get it |
|---|---|---|
| **VS Code**, Cursor, Windsurf, Antigravity | blame at the end of each line, a hover with the reason, a status bar item, your saves recorded, an optional "why?" box after a save | build the `.vsix` from [`editors/vscode`](https://github.com/cmdworks/tracewise/tree/main/editors/vscode) (marketplace publishing is pending) |
| **Zed** | hover with who and why, your saves recorded, a popup with one-click reasons after a save | `zed: install dev extension`, then pick [`editors/zed`](https://github.com/cmdworks/tracewise/tree/main/editors/zed) |
| **Any editor with language-server support** | the same hover and save recording | point it at `tracewise lsp` |

A change saved in an editor says which one (`you via Zed`). More in [editors](./docs/editors).

## From source

```bash
git clone https://github.com/cmdworks/tracewise && cd tracewise
cargo build --release
./target/release/tracewise setup
```

Release builds are not notarized yet. If macOS blocks one, clear the download flag first: `xattr -dr com.apple.quarantine .`.
