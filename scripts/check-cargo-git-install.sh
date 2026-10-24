#!/usr/bin/env bash
# Exercise Cargo's Git-source installation against the exact checkout commit.
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"
if [[ $# -ne 0 ]]; then
  printf 'Usage: %s\n' "$0" >&2
  exit 64
fi
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
source_url=$(python3 -c 'from pathlib import Path; print(Path.cwd().as_uri())')
revision=$(git rev-parse HEAD)
cargo install --git "$source_url" --rev "$revision" --locked --root "$temporary/install"
python3 scripts/check-installed-cli.py "$temporary/install/bin/bionic-reader"
cargo uninstall --root "$temporary/install" bionic-reader
[[ ! -e $temporary/install/bin/bionic-reader ]]
printf 'Cargo Git install, use, and uninstall checks passed for %s\n' "$revision"
