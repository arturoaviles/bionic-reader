#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
output=${1:-target/performance}
mkdir -p "$output"
output=$(cd "$output" && pwd)
cargo bench --locked --bench transformer -- --color never 2>&1 | tee "$output/divan.txt"
cargo build --locked --release --bin bionic-reader
target_directory=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
binary="$target_directory/release/bionic-reader"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
export XDG_CONFIG_HOME="$temporary/config"
hyperfine --shell=none --warmup 5 --runs 30 --export-json "$output/startup.json" \
  --export-markdown "$output/startup.md" "\"$binary\" --version"
wc -c < "$binary" > "$output/binary-bytes.txt"
{
  rustc -vV
  uname -a
  printf 'commit: %s\n' "$(git rev-parse HEAD)"
  printf 'binary bytes: %s\n' "$(cat "$output/binary-bytes.txt")"
  printf 'Timing is advisory; Divan includes allocation profiling.\n'
} > "$output/environment.txt"
