#!/usr/bin/env bash
# Build an archive for this machine; portable release candidates use build-release.sh.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"
if [[ $# -gt 1 ]]; then
  printf 'Usage: %s [output-directory]\n' "$0" >&2
  exit 64
fi
for tool in cargo rustc python3 git; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'Local archive building requires %s on PATH\n' "$tool" >&2
    exit 69
  }
done
host=$(rustc -vV | sed -n 's/^host: //p')
case "$host" in
  x86_64-apple-darwin|aarch64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-unknown-linux-musl|aarch64-unknown-linux-musl) ;;
  *) printf 'Unsupported local host: %s\n' "$host" >&2; exit 69 ;;
esac
metadata=$(cargo metadata --locked --no-deps --format-version 1)
fields=$(python3 -c '
import json, sys
metadata = json.load(sys.stdin)
package = next(package for package in metadata["packages"] if package["name"] == "bionic-reader")
print(package["version"] + "\t" + metadata["target_directory"])
' <<< "$metadata")
IFS=$'\t' read -r version target_directory <<< "$fields"
output_directory=${1:-"$repo_root/dist/local"}
cargo build --locked --release --bin bionic-reader --target "$host"
python3 scripts/create_archive.py \
  --binary "$target_directory/$host/release/bionic-reader" \
  --version "$version" --target "$host" \
  --epoch "${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct HEAD)}" \
  --output "$output_directory/bionic-reader-v$version-$host.tar.gz" \
  --root "$repo_root"
printf 'Built host-local archive in %s (use on the build machine).\n' "$output_directory"
