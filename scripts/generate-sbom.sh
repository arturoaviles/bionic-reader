#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
target=${1:?Usage: generate-sbom.sh TARGET [OUTPUT-DIRECTORY]}
output=${2:-dist/sbom}
mkdir -p "$output"
temporary=$(mktemp -d)
cleanup() {
  rm -rf "$temporary"
  rm -f "bionic-reader_bin_${target}.cdx.json" \
    "bindings/python/_native_cdylib_${target}.cdx.json" \
    "bindings/wasm/bionic_reader_wasm_cdylib_${target}.cdx.json" \
    bionic-reader_bin_wasm32-unknown-unknown.cdx.json \
    bindings/python/_native_cdylib_wasm32-unknown-unknown.cdx.json \
    bindings/wasm/bionic_reader_wasm_cdylib_wasm32-unknown-unknown.cdx.json
}
trap cleanup EXIT
# Generate runtime graphs with the features/target used for each embedded engine.
# CycloneDX excludes dev dependencies but emits every workspace binary/cdylib.
# Fetch locked metadata first: cyclonedx itself has no --locked flag.
cargo metadata --locked --format-version 1 --filter-platform "$target" >/dev/null
cargo metadata --locked --format-version 1 --filter-platform wasm32-unknown-unknown >/dev/null
lock_before=$(git hash-object Cargo.lock)
cargo cyclonedx --format json --spec-version 1.5 --describe binaries \
  --target "$target" --target-in-filename
mv "bionic-reader_bin_${target}.cdx.json" "$temporary/cli.json"
cargo cyclonedx --format json --spec-version 1.5 --describe binaries \
  --no-default-features --target "$target" --target-in-filename
mv "bindings/python/_native_cdylib_${target}.cdx.json" "$temporary/python.json"
cargo cyclonedx --format json --spec-version 1.5 --describe binaries \
  --no-default-features --target wasm32-unknown-unknown --target-in-filename
mv bindings/wasm/bionic_reader_wasm_cdylib_wasm32-unknown-unknown.cdx.json "$temporary/wasm.json"
[[ $(git hash-object Cargo.lock) == "$lock_before" ]]
python3 scripts/merge-library-sboms.py "$temporary/cli.json" "$temporary/python.json" \
  "$temporary/wasm.json" --output "$output/sbom.cdx.json"
