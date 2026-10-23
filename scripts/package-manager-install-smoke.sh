#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

for tool in docker cargo rustc python3 nfpm cargo-zigbuild zig; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'Package-manager install smoke requires %s on PATH\n' "$tool" >&2
    exit 69
  }
done
[[ $(docker info --format '{{.OSType}}') == linux ]] || {
  printf 'Package-manager install smoke requires a running Docker Linux engine\n' >&2
  exit 69
}

case $(uname -m) in
  x86_64|amd64)
    target=x86_64-unknown-linux-gnu
    docker_platform=linux/amd64
    ;;
  aarch64|arm64)
    target=aarch64-unknown-linux-gnu
    docker_platform=linux/arm64
    ;;
  *)
    printf 'Package-manager install smoke needs an x86_64 or arm64 host; found %s\n' "$(uname -m)" >&2
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
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
temporary=$(cd "$temporary" && pwd -P)
# APT downloads as the unprivileged _apt user inside its container.
chmod 755 "$temporary"

CANDIDATE_OUTPUT_DIR="$temporary/candidates" scripts/build-release.sh "$target"
scripts/package-linux.sh "$target" "$temporary/packages"

run_smoke_image() {
  local manager=$1
  local image=$2
  docker run --rm --platform "$docker_platform" \
    --mount "type=bind,src=$repo_root,dst=/workspace,readonly" \
    --mount "type=bind,src=$temporary,dst=/smoke" \
    --mount "type=bind,src=$temporary/packages,dst=/packages,readonly" \
    "$image" bash /workspace/scripts/package-manager-install-smoke-container.sh \
    "$manager" "$version"
}

run_smoke_image debian debian:bookworm-slim
run_smoke_image fedora fedora:44
run_smoke_image archlinux archlinux/archlinux:latest

printf 'Local signed-repository installation smokes passed for GNU target %s\n' "$target"
