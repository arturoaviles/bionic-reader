#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"

if [[ $# -ne 0 ]]; then
  printf 'Usage: %s\n' "$0" >&2
  exit 64
fi

for tool in cargo git python3 tar; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'Cargo package check requires %s on PATH\n' "$tool" >&2
    exit 69
  }
done

metadata=$(cargo metadata --locked --no-deps --format-version 1)
metadata_fields=$(python3 -c '
import json
import sys

metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
package = matches[0]
print("\t".join((package["name"], package["version"], metadata["target_directory"])))
' <<< "$metadata")
IFS=$'\t' read -r package_name package_version target_dir <<< "$metadata_fields"

temporary_parent=${TMPDIR:-/tmp}
if ! temporary_parent=$(cd "$temporary_parent" 2>/dev/null && pwd -P); then
  temporary_parent=/tmp
fi
case "$temporary_parent/" in
  "$repo_root/"*) temporary_parent=/tmp ;;
esac
temporary=$(mktemp -d "$temporary_parent/bionic-reader-cargo-package.XXXXXX")
temporary=$(cd "$temporary" && pwd -P)
case "$temporary/" in
  "$repo_root/"*)
    printf 'Cargo package temporary directory must be outside the checkout: %s\n' "$temporary" >&2
    exit 1
    ;;
esac
trap 'rm -rf "$temporary"' EXIT
mkdir -p "$temporary/install-root" "$temporary/extracted"

printf 'Packaging and verifying the extracted Cargo candidate...\n'
cargo package --locked --package "$package_name"
crate_path="$target_dir/package/$package_name-$package_version.crate"
[[ -f $crate_path ]] || {
  printf 'Cargo did not create the expected crate archive: %s\n' "$crate_path" >&2
  exit 1
}
printf 'Auditing Cargo archive contents for %s %s...\n' "$package_name" "$package_version"
python3 - "$repo_root" "$crate_path" "$package_name" "$package_version" <<'PY'
from pathlib import PurePosixPath
import subprocess
import sys
import tarfile
from pathlib import Path

repo_root = Path(sys.argv[1])
crate_path = Path(sys.argv[2])
package_root = f"{sys.argv[3]}-{sys.argv[4]}"
source_roots = {"completions", "integrations", "src", "tests"}
support_files = {
    "Cargo.lock",
    "Cargo.toml",
    "LICENSE",
    "README.md",
    "packaging/config.example.toml",
    "packaging/npm/bin/bionic-reader.js",
    "scripts/create_archive.py",
}
generated_files = {".cargo_vcs_info.json", "Cargo.toml.orig"}

tracked = subprocess.run(
    ["git", "ls-files", "-z"], cwd=repo_root, check=True, stdout=subprocess.PIPE
).stdout.decode("utf-8").split("\0")
tracked = {name for name in tracked if name}
missing_support = support_files - tracked
if missing_support:
    raise SystemExit("required Cargo package support files are not tracked: " + ", ".join(sorted(missing_support)))
expected_files = {
    name for name in tracked
    if name in support_files or PurePosixPath(name).parts[0] in source_roots
}

archive_files = set()
with tarfile.open(crate_path, "r:gz") as archive:
    members = archive.getmembers()
    if not members:
        raise SystemExit("Cargo archive is empty")
    for member in members:
        raw_name = member.name
        if raw_name.startswith("/") or "\\" in raw_name or ".." in raw_name.split("/"):
            raise SystemExit(f"Cargo archive contains an unsafe path: {raw_name}")
        path = PurePosixPath(raw_name)
        if not path.parts or path.parts[0] != package_root:
            raise SystemExit(f"Cargo archive has an unexpected top-level path: {raw_name}")
        relative_parts = path.parts[1:]
        if not relative_parts:
            if not member.isdir():
                raise SystemExit("Cargo archive root is not a directory")
            continue
        relative = PurePosixPath(*relative_parts).as_posix()
        if member.isdir():
            if not any(name.startswith(relative + "/") for name in expected_files):
                raise SystemExit(f"Cargo archive contains an unexpected directory: {relative}")
            continue
        if not member.isfile():
            raise SystemExit(f"Cargo archive contains a non-regular file: {relative}")
        if relative in archive_files:
            raise SystemExit(f"Cargo archive contains a duplicate file: {relative}")
        archive_files.add(relative)
        if relative in generated_files:
            continue
        if any(part.startswith(".") for part in PurePosixPath(relative).parts):
            raise SystemExit(f"Cargo archive contains an unexpected hidden file: {relative}")
        if relative not in expected_files:
            raise SystemExit(f"Cargo archive contains an unexpected or private file: {relative}")

missing_files = expected_files - archive_files
if missing_files:
    raise SystemExit("Cargo archive is missing tracked source or support files: " + ", ".join(sorted(missing_files)))
print(f"Cargo archive content audit passed ({len(archive_files)} files)")
PY

tar -xzf "$crate_path" -C "$temporary/extracted"
extracted_package="$temporary/extracted/$package_name-$package_version"
[[ -f $extracted_package/Cargo.toml ]] || {
  printf 'Could not find extracted package manifest: %s/Cargo.toml\n' "$extracted_package" >&2
  exit 1
}

cargo install --path "$extracted_package" --locked --root "$temporary/install-root"
installed_binary="$temporary/install-root/bin/bionic-reader"
[[ -x $installed_binary ]] || {
  printf 'Cargo did not install the expected executable: %s\n' "$installed_binary" >&2
  exit 1
}
python3 scripts/check-installed-cli.py "$installed_binary"

cargo uninstall --locked --root "$temporary/install-root" --package "$package_name"
if [[ -e $installed_binary || -L $installed_binary ]]; then
  printf 'Cargo uninstall left the executable behind: %s\n' "$installed_binary" >&2
  exit 1
fi

printf 'Cargo package install and uninstall checks passed for %s %s\n' \
  "$package_name" "$package_version"
