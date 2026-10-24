#!/usr/bin/env bash
set -Eeuo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"

artifact_dir=${1:-"$repo_root/dist/local"}
artifact_dir=$(cd "$artifact_dir" 2>/dev/null && pwd -P) || {
  printf 'Homebrew install check requires an existing artifact directory: %s\n' "${1:-$repo_root/dist/local}" >&2
  exit 66
}

brew_bin=${BREW_BIN:-$(command -v brew || true)}
[[ -n $brew_bin && -x $brew_bin ]] || {
  printf 'Homebrew install check requires brew on PATH or BREW_BIN\n' >&2
  exit 69
}
for tool in cargo rustc git python3; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'Homebrew install check requires %s on PATH\n' "$tool" >&2
    exit 69
  }
done

rust_host=$(rustc -vV | sed -n 's/^host: //p')
case "$rust_host" in
  x86_64-apple-darwin|aarch64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)
    target=$rust_host
    ;;
  *)
    printf 'Homebrew install check needs a supported macOS or GNU Linux host; found %s\n' "$rust_host" >&2
    exit 69
    ;;
esac

version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
print(matches[0]["version"])
')
archive="$artifact_dir/bionic-reader-v$version-$target.tar.gz"
[[ -f $archive ]] || {
  printf 'Host-native archive is missing: %s\n' "$archive" >&2
  printf 'Build it first with scripts/build-local-archive.sh [output-directory].\n' >&2
  exit 66
}

formula_name=bionic-reader-homebrew-ci
tap_repo="bionic-reader-homebrew-ci-$$"
tap_spec="arturoaviles/$tap_repo"
if "$brew_bin" list --formula "$formula_name" >/dev/null 2>&1; then
  printf 'Refusing to replace an existing Homebrew formula: %s\n' "$formula_name" >&2
  exit 73
fi
if "$brew_bin" tap | grep -Fqx -- "$tap_spec"; then
  printf 'Refusing to replace an existing Homebrew tap: %s\n' "$tap_spec" >&2
  exit 73
fi

temporary=
temporary_parent=${TMPDIR:-/tmp}
temporary_parent=$(cd "$temporary_parent" && pwd -P)
temporary=$(mktemp -d "${temporary_parent%/}/bionic-reader-homebrew.XXXXXX")
temporary=$(cd "$temporary" && pwd -P)
case "$temporary" in
  "$temporary_parent"/bionic-reader-homebrew.*) ;;
  *)
    printf 'Temporary directory escaped its expected parent: %s\n' "$temporary" >&2
    exit 73
    ;;
esac

tap_attempted=0
install_attempted=0
cleanup() {
  local status=$?
  trap - EXIT
  if ((install_attempted)) && "$brew_bin" list --formula "$formula_name" >/dev/null 2>&1; then
    "$brew_bin" uninstall --formula "$formula_name" || status=1
  fi
  if ((tap_attempted)) && "$brew_bin" tap | grep -Fqx -- "$tap_spec"; then
    "$brew_bin" untap "$tap_spec" || status=1
  fi
  if [[ -n $temporary ]]; then
    case "$temporary" in
      "$temporary_parent"/bionic-reader-homebrew.*) rm -rf -- "$temporary" || status=1 ;;
      *)
        printf 'Leaving unexpected temporary path untouched: %s\n' "$temporary" >&2
        status=1
        ;;
    esac
  fi
  exit "$status"
}
trap cleanup EXIT

tap_repo_dir="$temporary/homebrew-tap"
mkdir -p "$tap_repo_dir/Formula"
python3 scripts/render-homebrew-formula.py \
  --version "$version" \
  --artifacts "$artifact_dir" \
  --local-target "$target" \
  --formula-name "$formula_name" \
  --output "$tap_repo_dir/Formula/$formula_name.rb"
git -C "$tap_repo_dir" init -q
git -C "$tap_repo_dir" add Formula
git -C "$tap_repo_dir" -c user.name='Bionic Reader CI' -c user.email='ci@localhost' \
  commit -q -m 'Add local Homebrew candidate'

export HOMEBREW_NO_AUTO_UPDATE=1
tap_attempted=1
"$brew_bin" tap "$tap_spec" "$tap_repo_dir"
install_attempted=1
"$brew_bin" install --build-from-source "$tap_spec/$formula_name"
HOMEBREW_DEVELOPER=1 "$brew_bin" test "$tap_spec/$formula_name"
installed_prefix=$("$brew_bin" --prefix "$tap_spec/$formula_name")
python3 scripts/check-installed-cli.py "$installed_prefix/bin/bionic-reader"
"$brew_bin" uninstall --formula "$formula_name"
install_attempted=0

if "$brew_bin" list --formula "$formula_name" >/dev/null 2>&1; then
  printf 'Homebrew uninstall left the test formula installed: %s\n' "$formula_name" >&2
  exit 1
fi
"$brew_bin" untap "$tap_spec"
tap_attempted=0
printf 'Homebrew local archive install, use, test, and uninstall passed for %s.\n' "$target"
