#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ $# -lt 1 || $# -gt 2 ]]; then
  printf 'Usage: %s <target-triple> [output-directory]\n' "$0" >&2
  exit 64
fi

target=$1
version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
print(matches[0]["version"])
')
output_dir=${2:-"$repo_root/dist/candidates/v$version/python"}
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

build_args=(build --release --locked --target "$target" --out "$wheel_stage")
case "$target" in
  x86_64-apple-darwin|aarch64-apple-darwin)
    export MACOSX_DEPLOYMENT_TARGET=11.0
    ;;
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)
    command -v zig >/dev/null 2>&1 || {
      printf 'GNU Linux wheels require Zig to target the manylinux_2_28 baseline\n' >&2
      exit 69
    }
    [[ $(zig version) == '0.15.1' ]] || {
      printf 'GNU Linux wheels are pinned to Zig 0.15.1; found %s\n' "$(zig version)" >&2
      exit 69
    }
    build_args+=(--compatibility manylinux_2_28 --zig)
    ;;
  x86_64-unknown-linux-musl|aarch64-unknown-linux-musl)
    build_args+=(--compatibility musllinux_1_2)
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

host=$(rustc -vV | sed -n 's/^host: //p')
if [[ $host != "$target" ]]; then
  printf 'Built cross-target wheel %s; installation is checked only on an exact matching host (%s != %s)\n' \
    "$wheel" "$host" "$target"
  exit 0
fi

command -v uv >/dev/null 2>&1 || {
  printf 'uv is required for exact-host wheel installation checks\n' >&2
  exit 69
}

temporary=$(mktemp -d)
export PIP_CACHE_DIR="$temporary/pip-cache"
export UV_CACHE_DIR="$temporary/uv-cache"
verify_installed_binary() {
  local executable=$1
  [[ $("$executable" --version) == "bionic-reader $version" ]]
  python3 "$repo_root/scripts/check-installed-cli.py" "$executable"
  printf 'Installed executable checks passed: %s\n' "$executable"
}
python3 -m venv "$temporary/pip-env"
"$temporary/pip-env/bin/python" -m pip install --no-index --no-deps --force-reinstall "$wheel"
verify_installed_binary "$temporary/pip-env/bin/bionic-reader"

export UV_TOOL_DIR="$temporary/uv-tools"
export UV_TOOL_BIN_DIR="$temporary/uv-bin"
uv venv "$temporary/uv-env"
uv pip install --no-index --no-deps --find-links "$output_dir" \
  --python "$temporary/uv-env/bin/python" "bionic-reader==$version"
verify_installed_binary "$temporary/uv-env/bin/bionic-reader"
uv tool install --no-index --find-links "$output_dir" \
  --python "$(command -v python3)" "bionic-reader==$version"
verify_installed_binary "$UV_TOOL_BIN_DIR/bionic-reader"
printf 'Created wheel candidate %s\n' "$wheel"
