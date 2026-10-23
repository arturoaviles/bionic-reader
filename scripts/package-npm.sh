#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ $# -gt 2 ]]; then
  printf 'Usage: %s [artifact-directory] [output-directory]\n' "$0" >&2
  exit 64
fi

version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
print(matches[0]["version"])
')
artifact_dir=${1:-"$repo_root/dist/candidates/v$version"}
output_dir=${2:-"$repo_root/dist/candidates/v$version/npm"}
artifact_dir=$(cd "$artifact_dir" && pwd)
mkdir -p "$output_dir"
output_dir=$(cd "$output_dir" && pwd)

command -v npm >/dev/null 2>&1 || {
  printf 'npm is required to assemble Node package candidates\n' >&2
  exit 69
}
command -v node >/dev/null 2>&1 || {
  printf 'Node.js is required to assemble Node package candidates\n' >&2
  exit 69
}

stage=$(mktemp -d "$output_dir/.stage.XXXXXX")
temporary_dirs=()
cleanup() {
  rm -rf "$stage" "${temporary_dirs[@]}"
}
trap cleanup EXIT
mkdir -p "$stage/base/bin" "$stage/base/share/bionic-reader" "$stage/tarballs"
cp "$repo_root/packaging/npm/bin/bionic-reader.js" "$stage/base/bin/bionic-reader.js"
cp "$repo_root/LICENSE" "$stage/base/LICENSE"
cp "$repo_root/packaging/config.example.toml" "$stage/base/share/bionic-reader/config.example.toml"
cp -R "$repo_root/integrations" "$stage/base/share/bionic-reader/integrations"
cp -R "$repo_root/completions" "$stage/base/share/bionic-reader/completions"
chmod 755 "$stage/base/bin/bionic-reader.js"

python3 - "$repo_root/packaging/npm/package.json.in" "$stage/base/package.json" "$version" <<'PY'
from pathlib import Path
import sys

template = Path(sys.argv[1]).read_text(encoding="utf-8")
Path(sys.argv[2]).write_text(template.replace("@VERSION@", sys.argv[3]), encoding="utf-8")
PY

targets=(
  x86_64-apple-darwin
  aarch64-apple-darwin
  x86_64-unknown-linux-gnu
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-gnu
  aarch64-unknown-linux-musl
)

for target in "${targets[@]}"; do
  archive="$artifact_dir/bionic-reader-v$version-$target.tar.gz"
  [[ -f $archive ]] || {
    printf 'Required native archive is missing: %s\n' "$archive" >&2
    exit 66
  }

  case "$target" in
    x86_64-apple-darwin)
      suffix=darwin-x64; os_name=darwin; cpu=x64; libc=null ;;
    aarch64-apple-darwin)
      suffix=darwin-arm64; os_name=darwin; cpu=arm64; libc=null ;;
    x86_64-unknown-linux-gnu)
      suffix=linux-x64-gnu; os_name=linux; cpu=x64; libc=glibc ;;
    x86_64-unknown-linux-musl)
      suffix=linux-x64-musl; os_name=linux; cpu=x64; libc=musl ;;
    aarch64-unknown-linux-gnu)
      suffix=linux-arm64-gnu; os_name=linux; cpu=arm64; libc=glibc ;;
    aarch64-unknown-linux-musl)
      suffix=linux-arm64-musl; os_name=linux; cpu=arm64; libc=musl ;;
  esac

  package_name="@arturoaviles/bionic-reader-$suffix"
  package_dir="$stage/$suffix"
  mkdir -p "$package_dir/bin"
  temporary=$(mktemp -d)
  temporary_dirs+=("$temporary")
  tar -xzf "$archive" -C "$temporary"
  cp "$temporary/bionic-reader-$version-$target/bin/bionic-reader" "$package_dir/bin/bionic-reader"
  rm -rf "$temporary"
  chmod 755 "$package_dir/bin/bionic-reader"
  cp "$repo_root/LICENSE" "$package_dir/LICENSE"

  python3 - "$package_dir/package.json" "$package_name" "$version" "$os_name" "$cpu" "$libc" <<'PY'
from pathlib import Path
import json
import sys

path, name, version, os_name, cpu, libc = sys.argv[1:]
manifest = {
    "name": name,
    "version": version,
    "description": "Native bionic-reader executable for this platform",
    "license": "MIT",
    "files": ["bin/bionic-reader", "LICENSE"],
    "os": [os_name],
    "cpu": [cpu],
    "preferUnplugged": True,
}
if libc != "null":
    manifest["libc"] = [libc]
Path(path).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY

  npm pack "$package_dir" --ignore-scripts --pack-destination "$stage/tarballs" >/dev/null
done

npm pack "$stage/base" --ignore-scripts --pack-destination "$stage/tarballs" >/dev/null
cp "$stage/tarballs"/*.tgz "$output_dir/"
printf 'Created seven npm package tarballs in %s\n' "$output_dir"
