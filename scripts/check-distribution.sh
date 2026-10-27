#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

shellcheck_bin=${SHELLCHECK_BIN:-shellcheck}
zsh_bin=${ZSH_BIN:-zsh}
fish_bin=${FISH_BIN:-fish}
python_bin=${PYTHON_BIN:-python3}

for tool in "$shellcheck_bin" "$zsh_bin" "$fish_bin" node "$python_bin" ruff mypy ruby; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'Required distribution checker is missing: %s\n' "$tool" >&2
    exit 69
  }
done

shellcheck_targets=(scripts/*.sh integrations/bash.sh)
"$shellcheck_bin" --shell=bash scripts/*.sh
# SC1003 is excluded here because the integration intentionally uses printf's
# `\033\\` escape sequence to terminate a terminal control string.
"$shellcheck_bin" --shell=bash --exclude=SC1003 integrations/bash.sh
for source in "${shellcheck_targets[@]}"; do
  bash -n "$source"
done
"$zsh_bin" -n integrations/zsh.sh
"$fish_bin" --no-execute integrations/fish.fish

node_files=(packaging/npm/bin/*.js packaging/npm/lib/*.js packaging/npm/lib/*.cjs scripts/*.mjs)
for source in "${node_files[@]}"; do
  node --check "$source"
done

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
PYTHONPYCACHEPREFIX="$temporary/pycache" "$python_bin" -m compileall -q scripts
ruff check --config .github/ruff.toml scripts
ruff format --config .github/ruff.toml --check scripts
mypy --strict scripts/*.py
"$python_bin" scripts/check-packaging.py

version=0.0.0
touch \
  "$temporary/bionic-reader-v$version-x86_64-apple-darwin.tar.gz" \
  "$temporary/bionic-reader-v$version-aarch64-apple-darwin.tar.gz" \
  "$temporary/bionic-reader-v$version-x86_64-unknown-linux-gnu.tar.gz" \
  "$temporary/bionic-reader-v$version-aarch64-unknown-linux-gnu.tar.gz"
"$python_bin" scripts/render-homebrew-formula.py \
  --version "$version" \
  --artifacts "$temporary" \
  --output "$temporary/bionic-reader.rb"
ruby -c "$temporary/bionic-reader.rb"
ruby -c Formula/bionic-reader.rb
ruby -c scripts/check-homebrew-formula.rb
ruby -e 'require "yaml"; ARGV.each { |path| YAML.load_file(path) }' \
  packaging/linux/nfpm.yaml packaging/snap/snapcraft.yaml.in .github/workflows/*.yml

printf 'Distribution source checks passed. Native artifact and external registry checks remain release gates.\n'
