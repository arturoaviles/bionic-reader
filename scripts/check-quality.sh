#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

make verify

if command -v cargo-audit >/dev/null 2>&1; then
  make audit
else
  printf 'Dependency audit skipped locally: install cargo-audit 0.22.2 to run it.\n'
fi
