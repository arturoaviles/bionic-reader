#!/usr/bin/env bash
# Prebuilt Linux x86-64 tools not supported by install-action's manifests.
set -euo pipefail
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || exit 69
destination=${1:?Usage: install-workflow-tools.sh DESTINATION}
mkdir -p "$destination"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
install_tool() {
  local binary=$1 url=$2 checksum=$3 member=${4:-$1}
  curl --fail --silent --show-error --location --retry 3 "$url" --output "$temporary/$binary.tar.gz"
  printf '%s  %s\n' "$checksum" "$temporary/$binary.tar.gz" | sha256sum --check --strict
  tar -xzf "$temporary/$binary.tar.gz" -C "$temporary" "$member"
  install -m 755 "$temporary/$member" "$destination/$binary"
}
install_tool actionlint \
  https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_linux_amd64.tar.gz \
  8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8
install_tool lychee \
  https://github.com/lycheeverse/lychee/releases/download/lychee-v0.24.2/lychee-x86_64-unknown-linux-musl.tar.gz \
  73657a111819a30c47c08352896796f23d64e4eb2b3ed39b6d32149241566fc5 \
  lychee-x86_64-unknown-linux-musl/lychee
