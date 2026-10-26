#!/usr/bin/env bash
# Invoked in an offline glibc 2.28 container, after host checksum verification.
set -euo pipefail
bundle=${1:?Usage: check-glibc-runtime.sh BUNDLE}
[[ $(getconf GNU_LIBC_VERSION) == 'glibc 2.28' ]]
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
archives=("$bundle"/*.tar.gz)
[[ ${#archives[@]} == 1 && -f ${archives[0]} ]]
tar -xzf "${archives[0]}" --strip-components=1 -C "$temporary"
binary="$temporary/bin/bionic-reader"
export XDG_CONFIG_HOME="$temporary/config" NO_COLOR=1
"$binary" --version | grep '^bionic-reader '
[[ $("$binary" --style never -t 'Reading text') == 'Reading text' ]]
[[ $(printf 'Reading stdin' | "$binary" --style never) == 'Reading stdin' ]]
for format in plain ansi markdown html; do
  "$binary" --style always --format "$format" -t 'Reading text' > "$temporary/$format"
  [[ -s $temporary/$format ]]
done
grep -q '<strong>' "$temporary/html"
grep -q '\*\*' "$temporary/markdown"
