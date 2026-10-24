#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
scripts/check-completions.sh target/debug/bionic-reader
scripts/check-distribution.sh

if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
else
  printf 'Dependency audit skipped locally: install cargo-audit 0.22.2 to run it.\n'
fi
