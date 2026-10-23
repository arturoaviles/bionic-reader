#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  printf 'Usage: %s <rpm-directory> <empty-repository-directory> <repository-url> <existing-gpg-key-id>\n' "$0" >&2
  exit 64
fi

packages_dir=$(cd "$1" && pwd)
repo_dir=$2
repository_url=$3
key_id=$4
repo_dir_abs=$(mkdir -p "$repo_dir" && cd "$repo_dir" && pwd)
[[ $repository_url =~ ^(https?://|file://)[^[:space:]]+$ ]] || {
  printf 'Repository URL must be an http(s) or file URL without whitespace\n' >&2
  exit 64
}

for tool in rpm rpmsign createrepo_c gpg; do
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
packages=("$packages_dir"/*.rpm)
((${#packages[@]} > 0)) || {
  printf 'No .rpm candidate packages found in %s\n' "$packages_dir" >&2
  exit 66
}

mkdir -p "$repo_dir_abs/Packages" "$repo_dir_abs/keys"
for package in "${packages[@]}"; do
  architecture=$(rpm -qp --queryformat '%{ARCH}' "$package")
  case "$architecture" in
    x86_64|aarch64) ;;
    *)
      printf 'Unsupported RPM architecture %s in %s\n' "$architecture" "$package" >&2
      exit 65
      ;;
  esac
  destination="$repo_dir_abs/Packages/$(basename "$package")"
  [[ ! -e $destination ]] || {
    printf 'Duplicate package filename: %s\n' "$destination" >&2
    exit 65
  }
  cp "$package" "$destination"
  rpmsign --addsign --define "_gpg_name $key_id" "$destination"
done

gpg --export "$key_id" > "$repo_dir_abs/keys/bionic-reader-archive-keyring.gpg"
createrepo_c --checksum sha256 "$repo_dir_abs"
gpg --batch --yes --local-user "$key_id" --armor --detach-sign \
  --output "$repo_dir_abs/repodata/repomd.xml.asc" "$repo_dir_abs/repodata/repomd.xml"
gpg --verify "$repo_dir_abs/repodata/repomd.xml.asc" "$repo_dir_abs/repodata/repomd.xml"

python3 - "$repo_dir_abs/bionic-reader.repo" \
  "$(dirname "${BASH_SOURCE[0]}")/../packaging/repositories/dnf/bionic-reader.repo.in" \
  "$repository_url" <<'PY'
from pathlib import Path
import sys

output, template, repository_url = sys.argv[1:]
contents = Path(template).read_text(encoding="utf-8")
contents = contents.replace("@REPOSITORY_URL@", repository_url)
Path(output).write_text(contents, encoding="utf-8")
PY

printf 'Created signed DNF candidate repository at %s\n' "$repo_dir_abs"
