#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if (($#)); then
  targets=("$@")
else
  targets=(
    x86_64-apple-darwin
    aarch64-apple-darwin
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-musl
  )
fi

metadata_fields=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
matches = [package for package in metadata["packages"] if package["name"] == "bionic-reader"]
if len(matches) != 1:
    raise SystemExit("expected exactly one bionic-reader Cargo package")
print(matches[0]["version"] + "\t" + metadata["target_directory"])
')
IFS=$'\t' read -r version target_directory <<< "$metadata_fields"
source_epoch=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct HEAD)}
[[ $source_epoch =~ ^[0-9]+$ ]] || {
  printf 'SOURCE_DATE_EPOCH must be a non-negative integer\n' >&2
  exit 64
}
export SOURCE_DATE_EPOCH=$source_epoch
export CARGO_INCREMENTAL=0
export MACOSX_DEPLOYMENT_TARGET=11.0

rust_host=$(rustc -vV | sed -n 's/^host: //p')
output_dir="$repo_root/dist/candidates/v$version"
if [[ -n ${CANDIDATE_OUTPUT_DIR:-} ]]; then
  output_dir=$CANDIDATE_OUTPUT_DIR
fi
mkdir -p "$output_dir"

for target in "${targets[@]}"; do
  target_base=$target
  case "$target" in
    *.2.28) target_base=${target%.2.28} ;;
  esac

  if command -v rustup >/dev/null 2>&1; then
    rustup target add "$target_base"
  fi

  case "$target_base" in
    *-apple-darwin)
      if [[ $(uname -s) != Darwin || $rust_host != "$target_base" ]]; then
        printf 'Build %s on a matching macOS runner; host is %s\n' "$target_base" "$rust_host" >&2
        exit 69
      fi
      cargo build --locked --release --bin bionic-reader --target "$target_base"
      ;;
    *-unknown-linux-gnu)
      command -v cargo-zigbuild >/dev/null 2>&1 || {
        printf 'GNU Linux builds require cargo-zigbuild and Zig; see CONTRIBUTING.md\n' >&2
        exit 69
      }
      [[ $(cargo-zigbuild --version) == 'cargo-zigbuild 0.23.4' ]] || {
        printf 'GNU Linux builds are pinned to cargo-zigbuild 0.23.4; found %s\n' "$(cargo-zigbuild --version)" >&2
        exit 69
      }
      command -v zig >/dev/null 2>&1 || {
        printf 'GNU Linux builds require Zig on PATH; see CONTRIBUTING.md\n' >&2
        exit 69
      }
      [[ $(zig version) == '0.15.1' ]] || {
        printf 'GNU Linux builds are pinned to Zig 0.15.1; found %s\n' "$(zig version)" >&2
        exit 69
      }
      cargo zigbuild --locked --release --bin bionic-reader --target "$target_base.2.28"
      scripts/check-linux-glibc.sh "$target_directory/$target_base/release/bionic-reader" 2.28
      ;;
    *-unknown-linux-musl)
      build=(cargo)
      target_arch=${target_base%%-*}
      host_arch=${rust_host%%-*}
      if [[ $(uname -s) == Linux && $target_arch == "$host_arch" ]]; then
        linker_var="CARGO_TARGET_$(printf '%s' "$target_base" | tr '[:lower:]-' '[:upper:]_')_LINKER"
        if [[ -z ${!linker_var:-} ]]; then
          export "$linker_var=musl-gcc"
        fi
        command -v "${!linker_var}" >/dev/null 2>&1 || {
          printf 'musl target %s requires %s\n' "$target_base" "${!linker_var}" >&2
          exit 69
        }
      else
        printf 'Build %s on a matching Linux runner with musl-gcc installed\n' "$target_base" >&2
        exit 69
      fi
      "${build[@]}" build --locked --release --bin bionic-reader --target "$target_base"
      ;;
    *)
      printf 'Unsupported release target: %s\n' "$target" >&2
      exit 64
      ;;
  esac

  binary="$target_directory/$target_base/release/bionic-reader"
  [[ -x $binary ]] || {
    printf 'Expected build output was not created: %s\n' "$binary" >&2
    exit 1
  }

  if [[ $target_base == "$rust_host" ]]; then
    "$binary" --version
  fi

  archive="$output_dir/bionic-reader-v$version-$target_base.tar.gz"
  python3 scripts/create_archive.py \
    --binary "$binary" \
    --version "$version" \
    --target "$target_base" \
    --epoch "$source_epoch" \
    --output "$archive" \
    --root "$repo_root"
  printf 'Created %s\n' "$archive"
done

(
  cd "$output_dir"
  export LC_ALL=C
  for archive in bionic-reader-*.tar.gz; do
    [[ -f $archive ]] || continue
    if command -v sha256sum >/dev/null 2>&1; then
      sha256sum "$archive"
    else
      shasum -a 256 "$archive"
    fi
  done
) > "$output_dir/SHA256SUMS"
