#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  printf 'Usage: %s [artifact-directory] [output-directory]\n' "$0"
  printf '       %s --local [archive-or-directory] [output-directory]\n' "$0"
  printf '\n'
  printf 'The release form packages all six platform archives. The local form packages\n'
  printf 'one built archive into base and native tarballs for project-local installation.\n'
}

if [[ ${1:-} == -h || ${1:-} == --help ]]; then
  usage
  exit 0
fi

version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
print(matches[0]["version"])
')

local_mode=false
local_archive=
if [[ ${1:-} == --local ]]; then
  local_mode=true
  shift
  if [[ $# -gt 2 ]]; then
    usage >&2
    exit 64
  fi

  local_source=${1:-"$repo_root/dist/local"}
  if [[ -d $local_source ]]; then
    artifact_dir=$(cd "$local_source" && pwd)
    shopt -s nullglob
    local_archives=("$artifact_dir"/bionic-reader-v"$version"-*.tar.gz)
    shopt -u nullglob
    if ((${#local_archives[@]} != 1)); then
      printf 'Local npm packaging requires exactly one built archive for version %s in %s; found %s\n' \
        "$version" "$artifact_dir" "${#local_archives[@]}" >&2
      exit 66
    fi
    local_archive=${local_archives[0]}
  elif [[ -f $local_source ]]; then
    artifact_dir=$(cd "$(dirname "$local_source")" && pwd)
    local_archive="$artifact_dir/$(basename "$local_source")"
  else
    printf 'Local npm input must be a built archive or a directory containing exactly one built archive: %s\n' \
      "$local_source" >&2
    exit 66
  fi
  output_dir=${2:-"$artifact_dir/npm"}
else
  if [[ $# -gt 2 ]]; then
    usage >&2
    exit 64
  fi
  artifact_dir=${1:-"$repo_root/dist/candidates/v$version"}
  output_dir=${2:-"$repo_root/dist/candidates/v$version/npm"}
  artifact_dir=$(cd "$artifact_dir" && pwd)
fi

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

if [[ $local_mode == true ]]; then
  archive_name=$(basename "$local_archive")
  prefix="bionic-reader-v$version-"
  [[ $archive_name == "$prefix"*.tar.gz ]] || {
    printf 'Local archive must be named %s<TARGET>.tar.gz: %s\n' "$prefix" "$archive_name" >&2
    exit 64
  }
  local_target=${archive_name#"$prefix"}
  local_target=${local_target%.tar.gz}
  targets=("$local_target")
  case "$local_target" in
    x86_64-apple-darwin) suffix=darwin-x64 ;;
    aarch64-apple-darwin) suffix=darwin-arm64 ;;
    x86_64-unknown-linux-gnu) suffix=linux-x64-gnu ;;
    x86_64-unknown-linux-musl) suffix=linux-x64-musl ;;
    aarch64-unknown-linux-gnu) suffix=linux-arm64-gnu ;;
    aarch64-unknown-linux-musl) suffix=linux-arm64-musl ;;
    *)
      printf 'Unsupported npm native archive target: %s\n' "$local_target" >&2
      exit 64 ;;
  esac
  local_native_name="@arturoaviles/bionic-reader-$suffix"
  local_native_tarball="$output_dir/bionic-reader-$suffix-v$version.tgz"
else
  targets=(
    x86_64-apple-darwin
    aarch64-apple-darwin
    x86_64-unknown-linux-gnu
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-gnu
    aarch64-unknown-linux-musl
  )
fi

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
[[ -f $repo_root/dist/library/lib/wasm/engine_bg.wasm ]] || {
  printf 'Build the WebAssembly library payload before npm packaging\n' >&2
  exit 66
}
cp -R "$repo_root/dist/library/lib" "$stage/base/lib"
python3 - "$stage/base" "$version" <<'PY'
import runpy
import sys
from pathlib import Path

directory = Path(sys.argv[1])
files = {
    path.relative_to(directory).as_posix(): (path.read_bytes(), 0o644)
    for path in (directory / "lib").rglob("*") if path.is_file()
}
runpy.run_path("scripts/package-release-node.py")["validate_library"](files, sys.argv[2])
PY
chmod 755 "$stage/base/bin/bionic-reader.js"

python3 - "$repo_root/packaging/npm/package.json.in" "$stage/base/package.json" \
  "$version" "$local_mode" "${local_native_name:-}" "${local_native_tarball:-}" <<'PY'
from pathlib import Path
import json
import sys

template, output, version, local_mode, native_name, native_tarball = sys.argv[1:]
manifest = json.loads(Path(template).read_text(encoding="utf-8").replace("@VERSION@", version))
if local_mode == "true":
    # Point at the native archive beside this package's generated output. This
    # keeps local installs registry-free while retaining the normal launcher
    # dependency resolution used by release packages.
    manifest["optionalDependencies"] = {
        native_name: Path(native_tarball).resolve().as_uri()
    }
Path(output).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY

for target in "${targets[@]}"; do
  if [[ $local_mode == true ]]; then
    archive=$local_archive
  else
    archive="$artifact_dir/bionic-reader-v$version-$target.tar.gz"
  fi
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
    *)
      printf 'Unsupported npm native archive target: %s\n' "$target" >&2
      exit 64 ;;
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
if [[ $local_mode == true ]]; then
  cp "$stage/tarballs/arturoaviles-bionic-reader-$version.tgz" \
    "$output_dir/bionic-reader-base-v$version.tgz"
  cp "$stage/tarballs/arturoaviles-bionic-reader-$suffix-$version.tgz" \
    "$output_dir/bionic-reader-$suffix-v$version.tgz"
  printf 'Created local base and %s native npm tarballs in %s\n' "$suffix" "$output_dir"
else
  cp "$stage/tarballs"/*.tgz "$output_dir/"
  printf 'Created seven npm package tarballs in %s\n' "$output_dir"
fi
