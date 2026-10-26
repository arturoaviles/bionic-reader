#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
target=${1:?Usage: generate-sbom.sh TARGET [OUTPUT-DIRECTORY]}
output=${2:-dist/sbom}
mkdir -p "$output"
# Binary mode excludes dev dependencies and follows the native build's default
# features. Fetch locked metadata first; cyclonedx itself has no --locked flag.
cargo metadata --locked --format-version 1 --filter-platform "$target" >/dev/null
lock_before=$(git hash-object Cargo.lock)
cargo cyclonedx --format json --spec-version 1.5 --describe binaries \
  --target "$target" --target-in-filename
[[ $(git hash-object Cargo.lock) == "$lock_before" ]]
mv "bionic-reader_bin_${target}.cdx.json" "$output/sbom.cdx.json"
