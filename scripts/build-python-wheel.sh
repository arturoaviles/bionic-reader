#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

local_build=false
if [[ ${1:-} == --local ]]; then
  local_build=true
  shift
fi
if [[ $# -lt 1 || $# -gt 2 ]]; then
  printf 'Usage: %s [--local] <target-triple> [output-directory]\n' "$0" >&2
  exit 64
fi

target=$1
host=$(rustc -vV | sed -n 's/^host: //p')
if [[ $local_build == true && $target != "$host" ]]; then
  printf 'Local wheels must match the compiler host (%s), got %s\n' "$host" "$target" >&2
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
if [[ $local_build == true ]]; then
  output_dir=${2:-"$repo_root/dist/local/python"}
else
  output_dir=${2:-"$repo_root/dist/candidates/v$version/python"}
fi
mkdir -p "$output_dir"
output_dir=$(cd "$output_dir" && pwd)

stage_data="$repo_root/packaging/python/bionic_reader.data"
[[ ! -e $stage_data ]] || {
  printf 'Refusing to overwrite existing wheel data directory: %s\n' "$stage_data" >&2
  exit 73
}
temporary=
wheel_stage=
cleanup() {
  [[ -z $temporary ]] || rm -rf "$temporary"
  [[ -z $wheel_stage ]] || rm -rf "$wheel_stage"
  rm -rf "$stage_data"
}
trap cleanup EXIT
wheel_stage=$(mktemp -d "$output_dir/.wheel-stage.XXXXXX")
mkdir -p "$stage_data/data/share/bionic-reader"
mkdir -p "$stage_data/scripts"
if [[ $local_build == true ]]; then
  cargo build --release --locked --target "$target" --bin bionic-reader
fi
native_binary="${CARGO_TARGET_DIR:-$repo_root/target}/$target/release/bionic-reader"
[[ -x $native_binary ]] || {
  printf 'Build the native release archive for %s before its wheel\n' "$target" >&2
  exit 66
}
cp "$native_binary" "$stage_data/scripts/bionic-reader"
cp "$repo_root/packaging/config.example.toml" "$stage_data/data/share/bionic-reader/config.example.toml"
cp -R "$repo_root/integrations" "$stage_data/data/share/bionic-reader/integrations"
[[ -d $repo_root/completions ]] || {
  printf 'Generate shell completions before building wheels: bionic-reader completions bash|zsh|fish\n' >&2
  exit 66
}
cp -R "$repo_root/completions" "$stage_data/data/share/bionic-reader/completions"

command -v maturin >/dev/null 2>&1 || {
  printf 'maturin 1.15.0 is required; install it with python -m pip install maturin==1.15.0\n' >&2
  exit 69
}
[[ $(maturin --version) == 'maturin 1.15.0' ]] || {
  printf 'This candidate build is pinned to maturin 1.15.0; found %s\n' "$(maturin --version)" >&2
  exit 69
}

require_zig() {
  command -v zig >/dev/null 2>&1 || {
    printf 'Portable Linux wheels require Zig 0.15.1 for target-compatible runtime libraries\n' >&2
    exit 69
  }
  [[ $(zig version) == '0.15.1' ]] || {
    printf 'Portable Linux wheels are pinned to Zig 0.15.1; found %s\n' "$(zig version)" >&2
    exit 69
  }
}

build_args=(build --profile python-wheel --locked --target "$target" --out "$wheel_stage")
case "$target" in
  x86_64-apple-darwin|aarch64-apple-darwin)
    export MACOSX_DEPLOYMENT_TARGET=11.0
    ;;
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)
    if [[ $local_build == true ]]; then
      build_args+=(--compatibility linux)
    else
      require_zig
      build_args+=(--compatibility manylinux_2_28 --zig)
    fi
    ;;
  x86_64-unknown-linux-musl|aarch64-unknown-linux-musl)
    require_zig
    build_args+=(--compatibility musllinux_1_2 --zig)
    ;;
  *)
    printf 'Unsupported Python wheel target: %s\n' "$target" >&2
    exit 64
    ;;
esac

(cd "$repo_root/packaging/python" && maturin "${build_args[@]}")

staged_wheel=$(find "$wheel_stage" -maxdepth 1 -type f -name 'bionic_reader-*.whl' -print | LC_ALL=C sort | tail -n 1)
[[ -n $staged_wheel ]] || {
  printf 'maturin did not create a wheel for %s\n' "$target" >&2
  exit 1
}
python3 "$repo_root/scripts/check-python-wheel.py" "$staged_wheel"
wheel="$output_dir/$(basename "$staged_wheel")"
cp "$staged_wheel" "$wheel"

if [[ ${WHEEL_BUILD_ONLY:-0} != 1 ]]; then
  "$repo_root/scripts/check-python-wheel-install.sh" "$target" "$wheel"
fi
