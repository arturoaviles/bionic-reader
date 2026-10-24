#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 4 || $# -gt 5 ]]; then
  printf 'Usage: %s <deb-directory> <empty-repository-directory> <repository-url> <existing-gpg-key-id> [suite]\n' "$0" >&2
  exit 64
fi

packages_dir=$(cd "$1" && pwd)
repo_dir=$2
repository_url=$3
key_id=$4
suite=${5:-stable}
repo_dir_abs=$(mkdir -p "$repo_dir" && cd "$repo_dir" && pwd)

[[ $suite =~ ^[A-Za-z0-9.-]+$ ]] || {
  printf 'Suite names may contain only letters, numbers, dots, and hyphens\n' >&2
  exit 64
}
[[ $repository_url =~ ^(https?://|file://)[^[:space:]]+$ ]] || {
  printf 'Repository URL must be an http(s) or file URL without whitespace\n' >&2
  exit 64
}

for tool in dpkg-deb dpkg-scanpackages apt-ftparchive gpg gzip; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'Required repository tool is missing: %s\n' "$tool" >&2
    exit 69
  }
done
gpg --batch --list-secret-keys "$key_id" >/dev/null 2>&1 || {
  printf 'The existing signing key is unavailable in the current GPG keyring: %s\n' "$key_id" >&2
  exit 69
}
if [[ -n $(find "$repo_dir_abs" -mindepth 1 -maxdepth 1 -print -quit) ]]; then
  printf 'Repository output directory must be empty: %s\n' "$repo_dir_abs" >&2
  exit 73
fi

shopt -s nullglob
packages=("$packages_dir"/*.deb)
((${#packages[@]} > 0)) || {
  printf 'No .deb candidate packages found in %s\n' "$packages_dir" >&2
  exit 66
}

pool="$repo_dir_abs/pool/main/b/bionic-reader"
mkdir -p "$pool" "$repo_dir_abs/keys"
has_amd64=0
has_arm64=0
for package in "${packages[@]}"; do
  architecture=$(dpkg-deb -f "$package" Architecture)
  case "$architecture" in
    amd64) has_amd64=1 ;;
    arm64) has_arm64=1 ;;
    *)
      printf 'Unsupported Debian package architecture %s in %s\n' "$architecture" "$package" >&2
      exit 65
      ;;
  esac
  destination="$pool/$(basename "$package")"
  [[ ! -e $destination ]] || {
    printf 'Duplicate package filename: %s\n' "$destination" >&2
    exit 65
  }
  cp "$package" "$destination"
done

architectures=()
for architecture in amd64 arm64; do
  has_arch=0
  if [[ $architecture == amd64 ]]; then has_arch=$has_amd64; else has_arch=$has_arm64; fi
  [[ $has_arch -eq 1 ]] || continue
  architectures+=("$architecture")
  index_dir="$repo_dir_abs/dists/$suite/main/binary-$architecture"
  mkdir -p "$index_dir"
  (
    cd "$repo_dir_abs"
    dpkg-scanpackages --arch "$architecture" pool/main/b/bionic-reader /dev/null > "$index_dir/Packages"
  )
  gzip -n -9 < "$index_dir/Packages" > "$index_dir/Packages.gz"
done

architecture_list=$(printf '%s\n' "${architectures[@]}" | LC_ALL=C sort | tr '\n' ' ' | sed 's/[[:space:]]*$//')
distribution_dir="$repo_dir_abs/dists/$suite"
release_config=$(mktemp)
trap 'rm -f "$release_config"' EXIT
cat > "$release_config" <<EOF
APT::FTPArchive::Release {
  Origin "Bionic Reader";
  Label "Bionic Reader";
  Suite "$suite";
  Codename "$suite";
  Architectures "$architecture_list";
  Components "main";
  Description "Bionic Reader project package repository";
};
EOF
apt-ftparchive -c "$release_config" release "$distribution_dir" > "$distribution_dir/Release"
gpg --export "$key_id" > "$repo_dir_abs/keys/bionic-reader-archive-keyring.gpg"
gpg --batch --yes --local-user "$key_id" --clearsign \
  --output "$distribution_dir/InRelease" "$distribution_dir/Release"
gpg --batch --yes --local-user "$key_id" --armor --detach-sign \
  --output "$distribution_dir/Release.gpg" "$distribution_dir/Release"
gpg --verify "$distribution_dir/Release.gpg" "$distribution_dir/Release"
gpg --verify "$distribution_dir/InRelease"

python3 - "$repo_dir_abs/bionic-reader.sources" \
  "$(dirname "${BASH_SOURCE[0]}")/../packaging/repositories/apt/bionic-reader.sources.in" \
  "$repository_url" "$suite" "$architecture_list" <<'PY'
from pathlib import Path
import sys

output, template, repository_url, suite, architectures = sys.argv[1:]
contents = Path(template).read_text(encoding="utf-8")
contents = contents.replace("@REPOSITORY_URL@", repository_url)
contents = contents.replace("@SUITE@", suite)
contents = contents.replace("@ARCHITECTURES@", architectures)
Path(output).write_text(contents, encoding="utf-8")
PY

printf 'Created signed APT candidate repository at %s\n' "$repo_dir_abs"
