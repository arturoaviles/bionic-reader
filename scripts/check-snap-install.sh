#!/usr/bin/env bash
set -Eeuo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"

if [[ $# -ne 1 ]]; then
  printf 'Usage: %s <local-bionic-reader-snap-file>\n' "$0" >&2
  exit 64
fi

candidate=$1
[[ -f $candidate ]] || {
  printf 'Local Snap candidate does not exist: %s\n' "$candidate" >&2
  exit 66
}
candidate_dir=$(cd "$(dirname "$candidate")" && pwd -P)
candidate="$candidate_dir/$(basename "$candidate")"

for tool in cargo python3 snap; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'Snap installation smoke requires %s on PATH\n' "$tool" >&2
    exit 69
  }
done
if (( EUID != 0 )); then
  command -v sudo >/dev/null 2>&1 || {
    printf 'Snap installation and removal require sudo or root privileges\n' >&2
    exit 69
  }
fi

snap_bin=$(command -v snap)
run_snap() {
  if (( EUID == 0 )); then
    "$snap_bin" "$@"
  else
    sudo -- "$snap_bin" "$@"
  fi
}

snapd_ready=0
for attempt in {1..30}; do
  if run_snap list >/dev/null 2>&1; then
    snapd_ready=1
    break
  fi
  sleep 2
done
if (( snapd_ready == 0 )); then
  printf 'snapd did not become ready after %s checks; run this on Ubuntu with snapd active\n' \
    "$attempt" >&2
  exit 69
fi
if run_snap list bionic-reader >/dev/null 2>&1; then
  printf 'Refusing to replace an existing bionic-reader Snap installation\n' >&2
  exit 73
fi

expected_version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
print(matches[0]["version"])
')
case "$(basename "$candidate")" in
  "bionic-reader_${expected_version}_amd64.snap"|"bionic-reader_${expected_version}_arm64.snap")
    ;;
  *)
    printf 'Expected a local Snap candidate for bionic-reader %s; found %s\n' \
      "$expected_version" "$(basename "$candidate")" >&2
    exit 65
    ;;
esac

installed=0
cleanup() {
  local status=$?
  trap - EXIT
  if (( installed )); then
    if ! run_snap remove bionic-reader; then
      printf 'Could not remove the bionic-reader Snap installed by this check\n' >&2
      if (( status == 0 )); then
        status=1
      fi
    fi
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

run_snap install --dangerous --classic "$candidate"
installed=1

installed_cli=/snap/bin/bionic-reader
[[ -x $installed_cli ]] || {
  printf 'Snap installation did not expose %s\n' "$installed_cli" >&2
  exit 1
}
actual_version=$("$installed_cli" --version)
[[ $actual_version == "bionic-reader $expected_version" ]] || {
  printf 'Installed Snap version did not match: %s\n' "$actual_version" >&2
  exit 1
}
python3 "$repo_root/scripts/check-installed-cli.py" "$installed_cli"

run_snap remove bionic-reader
installed=0
if run_snap list bionic-reader >/dev/null 2>&1; then
  printf 'Snap removal left bionic-reader installed\n' >&2
  exit 1
fi
[[ ! -e $installed_cli ]] || {
  printf 'Snap removal left the command at %s\n' "$installed_cli" >&2
  exit 1
}

printf 'Local Snap installation smoke passed for bionic-reader %s\n' "$expected_version"
