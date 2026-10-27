#!/usr/bin/env bash
# The universal payload is built once on Linux x86-64 and reused by every target.
set -euo pipefail
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || exit 69
destination=${1:?Usage: install-wasm-bindgen.sh DESTINATION}
mkdir -p "$destination"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
archive=wasm-bindgen-0.2.129-x86_64-unknown-linux-musl
curl --fail --silent --show-error --location --retry 3 \
  "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/0.2.129/$archive.tar.gz" \
  --output "$temporary/tool.tar.gz"
printf '%s  %s\n' \
  82d12bb940e2d4e72e0d5605387fc1b8ca179044e012b620f0ce4e7440e8320e \
  "$temporary/tool.tar.gz" | sha256sum --check --strict
tar -xzf "$temporary/tool.tar.gz" -C "$temporary" "$archive/wasm-bindgen"
install -m 755 "$temporary/$archive/wasm-bindgen" "$destination/wasm-bindgen"
