#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ $# -gt 1 ]]; then
  printf 'Usage: %s [bionic-reader-binary]\n' "$0" >&2
  exit 64
fi

binary=${1:-"$repo_root/target/debug/bionic-reader"}
[[ -x $binary ]] || {
  printf 'Built bionic-reader executable not found: %s\n' "$binary" >&2
  exit 66
}

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT

for shell in bash zsh fish; do
  case "$shell" in
    bash) file=bionic-reader.bash ;;
    zsh) file=_bionic-reader ;;
    fish) file=bionic-reader.fish ;;
  esac

  "$binary" completions "$shell" >"$temporary/$file"
  if ! cmp -s "$temporary/$file" "$repo_root/completions/$file"; then
    printf 'Generated %s completion differs from completions/%s; regenerate it with the built CLI.\n' \
      "$shell" "$file" >&2
    diff -u "$repo_root/completions/$file" "$temporary/$file" || true
    exit 1
  fi
done

printf 'Checked-in shell completions match the built CLI.\n'
