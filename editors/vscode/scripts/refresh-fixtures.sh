#!/bin/sh
# Records what the real `tracewise` answers, so the extension's tests read real shapes. Run it after the CLI's JSON changes:
#   TRACEWISE_BIN=../../target/debug/tracewise sh scripts/refresh-fixtures.sh
# `node scripts/shape-check.mjs` (run by CI) compares the shapes of the committed fixtures with a fresh run.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
tw=${TRACEWISE_BIN:-tracewise}
out=${1:-$here/src/test/fixtures}
mkdir -p "$out"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export TRACEWISE_HOME="$work/home" HOME="$work/home"
mkdir -p "$HOME" "$work/p"
cd "$work/p"
git init -q .
printf 'fn parse(input: &str) {\n    build(input)\n}\n' > parser.rs
env -u CLAUDECODE -u CLAUDE_CODE_SESSION_ID "$tw" init >/dev/null
printf 'fn parse(input: &str) {\n    let tokens = lex(input);\n    build(tokens)\n}\n' > parser.rs
TRACEWISE_ACTOR=agent:claude:s1 env -u CLAUDECODE "$tw" scan >/dev/null
TRACEWISE_ACTOR=agent:claude:s1 env -u CLAUDECODE "$tw" note --last "handle empty input without panicking" >/dev/null
printf 'fn parse(input: &str) {\n    let tokens = lex(input);\n    build(tokens)\n}\n// hand edit\n' > parser.rs
TRACEWISE_ACTOR=human:me env -u CLAUDECODE "$tw" scan >/dev/null
rec() { env -u CLAUDECODE -u CLAUDE_CODE_SESSION_ID "$tw" "$@" --json > "$out/$1.json"; }
rec companion
rec status
rec blame parser.rs
rec log
rec pending
rec doctor
env -u CLAUDECODE "$tw" why parser.rs:2 --json > "$out/why.json"
echo "wrote fixtures to $out"
