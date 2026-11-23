# Contributing

Tracewise is open source under the [GNU GPL v3 or later](LICENSE): changes you distribute must stay open source under the same license.

## Commits

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`.

| Type | Use for | In the changelog |
|---|---|---|
| `feat` | a new feature | Added |
| `fix` | a bug fix | Fixed |
| `perf`, `refactor` | speed, internal changes | Changed |
| `docs` | documentation | Documentation |
| `test`, `ci`, `build`, `chore`, `style` | everything else | left out |

Scopes name the part of the repository: `cli`, `core`, `git`, `shim`, `platform`, `ui` (desktop app), `vscode` and `zed` (editor extensions), `lsp` (the language server), `site`, `release`. Add `!` after the type (`feat!: ...`) for a breaking change. CI checks every commit of a pull request.

## Before a pull request

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

## Trying a build

`scripts/twd` builds the workspace and links the dev programs (`tracewise`, `tracewise-watchd`, ...), the other names `tw`, `trace` and `tracwise`, plus `twd` and `twdev` into `~/.local/bin` (`TWDEV_BIN` changes the folder). From then on `tracewise` is the dev build in every folder and terminal, on your real data, and `twd` from any folder rebuilds and relinks. The links point into `target/debug`, so a plain `cargo build` updates them too. `twd` stops rather than overwrite a file there that is not its own link; `twd --uninstall` removes its links.

`twdev` runs the same build on separate data in `~/.tracewise-dev` (`TWDEV_HOME` changes it), to try things such as the v1 migration without touching your real history.

With [direnv](https://direnv.net) (`direnv allow` once), `.devbin/` is on `PATH` inside the checkout, so `twd` and `twdev` work there before the first install.

## The VS Code extension

It lives in `editors/vscode` (TypeScript, its own `pnpm` project) and is a thin client of the `tracewise` program: it runs a short list of commands with `--json` and draws the answers, so the JSON in [docs/json.md](docs/json.md) is its contract.

```bash
cd editors/vscode && pnpm install
pnpm check                 # type-check, tests, bundle
pnpm package               # a .vsix to install by hand: code --install-extension tracewise-*.vsix
TRACEWISE_BIN=../../target/debug/tracewise pnpm fixtures   # record what the CLI answers, after changing its JSON
```

To try it, press F5 in VS Code with `editors/vscode` open (an Extension Development Host starts), with `tracewise` on your `PATH` (`scripts/twd` links the dev build).

## The Zed extension

`editors/zed` is a small Rust crate built for WebAssembly (`rustup target add wasm32-wasip2`, then `cargo build --release --target wasm32-wasip2` in that folder). It only starts `tracewise lsp`; the language server itself is part of the CLI (`crates/tracewise-cli/src/lsp.rs`). To try it, run `zed: install dev extension` in Zed and pick the folder.

## Documentation site

The site at https://cmdworks.github.io/tracewise/ is built from `docs/` and a few root files (`UI.md`, `CHANGELOG.md`, `CONTRIBUTING.md`). Edit those; never edit `site/src`, which is generated. The home page and the look are in `site/`.

```bash
cd site && pnpm install && pnpm dev     # live preview
pnpm build                              # fails on a broken link
```

Changes to the docs on `main` are published by the `Docs site` workflow.

## Releasing (maintainers)

```bash
scripts/release.sh 0.3.0                      # bumps versions, writes the CHANGELOG from commits, commits and tags
git push origin main && git push origin v0.3.0
```

The tag runs the release workflow. It refuses a version that is not higher than the newest release or does not match `Cargo.toml`.
