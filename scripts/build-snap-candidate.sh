#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ $# -gt 1 ]]; then
  printf 'Usage: %s [output-directory]\n' "$0" >&2
  exit 64
fi

[[ $(uname -s) == Linux ]] || {
  printf 'Snap candidates must be built on Linux\n' >&2
  exit 69
}
command -v snapcraft >/dev/null 2>&1 || {
  printf 'Snapcraft 8.14.4 is required to build a candidate; see CONTRIBUTING.md\n' >&2
  exit 69
}
[[ $(snapcraft --version) == 'snapcraft 8.14.4' ]] || {
  printf 'Snap candidate builds are pinned to Snapcraft 8.14.4; found %s\n' "$(snapcraft --version)" >&2
  exit 69
}

version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
print(matches[0]["version"])
')
output_dir=${1:-"$repo_root/dist/candidates/v$version"}
mkdir -p "$output_dir"
output_dir=$(cd "$output_dir" && pwd)
stage="$output_dir/snap-stage"
mkdir -p "$stage"

python3 - "$repo_root/packaging/snap/snapcraft.yaml.in" "$stage/snapcraft.yaml" \
  "$version" "$repo_root" <<'PY'
from pathlib import Path
import json
import sys

template_path, output_path, version, source = sys.argv[1:]
contents = Path(template_path).read_text(encoding="utf-8")
contents = contents.replace("@VERSION@", version)
contents = contents.replace("@SOURCE_PATH@", json.dumps(str(Path(source).resolve())))
Path(output_path).write_text(contents, encoding="utf-8")
PY

(cd "$stage" && snapcraft --destructive-mode)
snap_files=("$stage"/bionic-reader_*.snap)
[[ -f ${snap_files[0]} ]] || {
  printf 'Snapcraft did not create a .snap candidate in %s\n' "$stage" >&2
  exit 1
}
cp "${snap_files[0]}" "$output_dir/"
printf 'Created local classic Snap candidate %s\n' "$output_dir/$(basename "${snap_files[0]}")"
printf 'The recipe remains grade devel pending Snap Store classic-confinement approval.\n'
