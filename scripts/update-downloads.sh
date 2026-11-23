#!/bin/bash
# Rewrites the download tables in README.md and site/download.md for a release tag (default: the newest release).
# The rows sit between <!-- downloads:start --> and <!-- downloads:end -->. Usage: scripts/update-downloads.sh [vX.Y.Z[-pre]]
set -euo pipefail
cd "$(dirname "$0")/.."
tag=${1:-$(gh release list --limit 1 --json tagName --jq '.[0].tagName')}
[ -n "$tag" ] || { echo "no release found" >&2; exit 1; }
repo=${GITHUB_REPOSITORY:-cmdworks/tracewise}
TAG=$tag REPO=$repo python3 - <<'PY'
import os, re
tag, repo = os.environ["TAG"], os.environ["REPO"]
v = tag.lstrip("v")
base = f"https://github.com/{repo}/releases/download/{tag}"
def rows(note):
    out = []
    for name, arch in (("Apple silicon", "arm64"), ("Intel", "x86_64")):
        cli = f"tracewise-{v}-macos-{arch}.tar.gz"
        dmg = f"tracewise-desktop-{v}-macos-{arch}.dmg"
        out.append(f"| **macOS, {name}** | [{cli}]({base}/{cli}) | [Tracewise {v} ({arch}).dmg]({base}/{dmg}){note} |")
    out += ["| **Linux** | planned (build from source: `cargo build --release`) | planned |", "| **Windows** | planned | planned |"]
    return "\n".join(out)
for path, note in (("README.md", " · preview"), ("site/download.md", " (preview)")):
    s = open(path).read()
    new = re.sub(r"(<!-- downloads:start -->\n).*?(\n<!-- downloads:end -->)", lambda m: m.group(1) + rows(note) + m.group(2), s, flags=re.S)
    if new != s:
        open(path, "w").write(new)
        print("updated", path, "->", tag)
PY
