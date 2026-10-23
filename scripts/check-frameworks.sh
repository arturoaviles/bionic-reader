#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
while IFS=$'\t' read -r name url revision; do
  git init --quiet "$temporary/$name"
  git -C "$temporary/$name" fetch --quiet --depth=1 "$url" "$revision"
  git -C "$temporary/$name" checkout --quiet --detach FETCH_HEAD
done < integrations/adapters/frameworks.tsv
BIONIC_READER_FRAMEWORKS_DIR="$temporary" cargo test --locked --test frameworks -- --ignored --test-threads=1
