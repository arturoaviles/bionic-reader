#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ $# -lt 1 || $# -gt 2 ]]; then
  printf 'Usage: %s <linux-gnu-target> [output-directory]\n' "$0" >&2
  printf 'Example: %s x86_64-unknown-linux-gnu\n' "$0" >&2
  exit 64
fi

target=$1
output_dir=${2:-"$repo_root/dist/candidates"}
case "$target" in
  x86_64-unknown-linux-gnu) go_arch=amd64; deb_arch=amd64; rpm_arch=x86_64; archlinux_arch=x86_64 ;;
  aarch64-unknown-linux-gnu) go_arch=arm64; deb_arch=arm64; rpm_arch=aarch64; archlinux_arch=aarch64 ;;
  *)
    printf 'System packages are built from GNU Linux targets; unsupported target: %s\n' "$target" >&2
    exit 64
    ;;
esac

command -v nfpm >/dev/null 2>&1 || {
  printf 'nfpm is required to package Linux candidates; see CONTRIBUTING.md\n' >&2
  exit 69
}
[[ $(nfpm --version) == *'2.47.0'* ]] || {
  printf 'Linux package candidates are pinned to nfpm 2.47.0; found %s\n' "$(nfpm --version)" >&2
  exit 69
}

metadata_fields=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
print(matches[0]["version"] + "\t" + metadata["target_directory"])
')
IFS=$'\t' read -r version target_directory <<< "$metadata_fields"
binary="$target_directory/$target/release/bionic-reader"
[[ -x $binary ]] || {
  printf 'Build the candidate binary first: %s\n' "$binary" >&2
  exit 66
}

scripts/check-linux-glibc.sh "$binary" 2.28
mkdir -p "$output_dir"
output_dir=$(cd "$output_dir" && pwd)
export VERSION=$version
export BINARY=$binary
export LICENSE="$repo_root/LICENSE"
export README="$repo_root/README.md"
export INTEGRATIONS="$repo_root/integrations"
export CONFIG_EXAMPLE="$repo_root/packaging/config.example.toml"
export COMPLETIONS="$repo_root/completions"
export NFPM_ARCH=$go_arch
export NFPM_DEB_ARCH=$deb_arch
export NFPM_RPM_ARCH=$rpm_arch
export NFPM_ARCHLINUX_ARCH=$archlinux_arch

for packager in deb rpm archlinux; do
  nfpm package \
    --config "$repo_root/packaging/linux/nfpm.yaml" \
    --packager "$packager" \
    --target "$output_dir"
done
