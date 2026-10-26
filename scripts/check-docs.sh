#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
mode=${1:---offline}
case "$mode" in
  --offline) flags=(--offline) ;;
  --external) flags=() ;;
  *) printf 'Usage: %s [--offline|--external]\n' "$0" >&2; exit 64 ;;
esac
files=()
while IFS= read -r -d '' file; do
  files+=("$file")
done < <(git ls-files -z -- '*.md')
[[ ${#files[@]} -gt 0 ]]
lychee --config .lychee.toml --no-progress "${flags[@]}" "${files[@]}"
