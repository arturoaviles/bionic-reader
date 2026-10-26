#!/usr/bin/env bash
set -Eeuo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"

if [[ $# -gt 1 ]]; then
  printf 'Usage: %s [output-directory]\n' "$0" >&2
  exit 64
fi

[[ $(uname -s) == Linux ]] || {
  printf 'Core24 Snap candidates must be built on Ubuntu 24.04\n' >&2
  exit 69
}
[[ -r /etc/os-release ]] || {
  printf 'Cannot identify the host distribution; Snap candidates require Ubuntu 24.04\n' >&2
  exit 69
}
# shellcheck disable=SC1091
source /etc/os-release
[[ ${ID:-} == ubuntu && ${VERSION_ID:-} == 24.04 ]] || {
  printf 'Core24 Snap candidates require Ubuntu 24.04; found %s %s\n' \
    "${ID:-unknown}" "${VERSION_ID:-unknown}" >&2
  exit 69
}

case $(uname -m) in
  x86_64|amd64)
    build_for=amd64
    ;;
  aarch64|arm64)
    build_for=arm64
    ;;
  *)
    printf 'Snap candidates require an amd64 or arm64 host; found %s\n' "$(uname -m)" >&2
    exit 69
    ;;
esac

for tool in cargo python3 tar; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'Snap candidate builds require %s on PATH\n' "$tool" >&2
    exit 69
  }
done

snapcraft_bin=$(command -v snapcraft || true)
[[ -n $snapcraft_bin ]] || {
  printf 'Snapcraft 8.14.5 is required to build a candidate; see CONTRIBUTING.md\n' >&2
  exit 69
}
if (( EUID != 0 )); then
  command -v sudo >/dev/null 2>&1 || {
    printf 'Snapcraft destructive builds require root privileges; install sudo or run as root\n' >&2
    exit 69
  }
fi

run_snapcraft() {
  if (( EUID == 0 )); then
    "$snapcraft_bin" "$@"
  else
    sudo -- env "PATH=$PATH" "$snapcraft_bin" "$@"
  fi
}

snapcraft_version=$(run_snapcraft --version)
[[ $snapcraft_version == 'snapcraft 8.14.5' ]] || {
  printf 'Snap candidate builds are pinned to Snapcraft 8.14.5; found %s\n' \
    "$snapcraft_version" >&2
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
output_dir=$(cd "$output_dir" && pwd -P)
[[ $output_dir != "$repo_root" ]] || {
  printf 'The candidate output directory cannot be the repository root\n' >&2
  exit 64
}

temporary=$(mktemp -d "${TMPDIR:-/tmp}/bionic-reader-snap.XXXXXX")
cleanup() {
  local status=$?
  trap - EXIT
  if [[ -n ${temporary:-} && -d $temporary ]]; then
    if (( EUID == 0 )); then
      rm -rf -- "$temporary"
    else
      sudo -- rm -rf -- "$temporary" || {
        printf 'Could not remove Snap build staging directory %s\n' "$temporary" >&2
        if (( status == 0 )); then
          status=1
        fi
      }
    fi
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

source_snapshot="$temporary/source"
project_stage="$temporary/project"
mkdir -p "$source_snapshot" "$project_stage"

git -C "$repo_root" ls-files -z |
  tar --null --no-recursion -C "$repo_root" -cf - -T - |
  tar -C "$source_snapshot" -xf -

python3 - "$repo_root/packaging/snap/snapcraft.yaml.in" "$project_stage/snapcraft.yaml" \
  "$version" "$source_snapshot" <<'PY'
from pathlib import Path
import json
import sys

template_path, output_path, version, source = sys.argv[1:]
contents = Path(template_path).read_text(encoding="utf-8")
contents = contents.replace("@VERSION@", version)
contents = contents.replace('"@SOURCE_PATH@"', json.dumps(str(Path(source).resolve())))
Path(output_path).write_text(contents, encoding="utf-8")
PY

(cd "$project_stage" && run_snapcraft --destructive-mode --build-for="$build_for")
snap_files=("$project_stage"/bionic-reader_*.snap)
if [[ ! -f ${snap_files[0]} || ${#snap_files[@]} -ne 1 ]]; then
  printf 'Snapcraft did not create exactly one local candidate in %s\n' "$project_stage" >&2
  exit 1
fi

candidate_path="$output_dir/$(basename "${snap_files[0]}")"
install -m 0644 -- "${snap_files[0]}" "$candidate_path"
printf 'Created local classic Snap candidate %s\n' "$candidate_path"
printf 'The candidate is local and unsigned; Snap Store classic-confinement approval remains separate.\n'
