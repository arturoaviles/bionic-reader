#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
target=${1:?Usage: check-python-wheel-install.sh TARGET WHEEL}
wheel=${2:?Usage: check-python-wheel-install.sh TARGET WHEEL}
output_dir=$(cd "$(dirname "$wheel")" && pwd)
wheel="$output_dir/$(basename "$wheel")"
host=$(rustc -vV | sed -n 's/^host: //p')
version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "bionic-reader"))
')
temporary=
trap '[[ -z $temporary ]] || rm -rf "$temporary"' EXIT
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
"$temporary/pip-env/bin/python" -m pip uninstall --yes bionic-reader
[[ ! -e $temporary/pip-env/bin/bionic-reader ]]

export UV_TOOL_DIR="$temporary/uv-tools"
export UV_TOOL_BIN_DIR="$temporary/uv-bin"
uv venv "$temporary/uv-env"
uv pip install --no-index --no-deps --find-links "$output_dir" \
  --python "$temporary/uv-env/bin/python" "bionic-reader==$version"
verify_installed_binary "$temporary/uv-env/bin/bionic-reader"
uv pip uninstall --python "$temporary/uv-env/bin/python" bionic-reader
[[ ! -e $temporary/uv-env/bin/bionic-reader ]]
uv tool install --no-index --find-links "$output_dir" \
  --python "$(command -v python3)" "bionic-reader==$version"
verify_installed_binary "$UV_TOOL_BIN_DIR/bionic-reader"
uv tool uninstall bionic-reader
[[ ! -e $UV_TOOL_BIN_DIR/bionic-reader ]]
printf 'Created wheel candidate %s\n' "$wheel"
