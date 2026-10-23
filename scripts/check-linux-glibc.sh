#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  printf 'Usage: %s <binary> <maximum-glibc-version>\n' "$0" >&2
  exit 64
fi

binary=$1
maximum=$2
if ! command -v readelf >/dev/null 2>&1; then
  printf 'readelf is required to inspect the Linux binary\n' >&2
  exit 69
fi

if [[ ! -f $binary ]]; then
  printf 'Binary does not exist: %s\n' "$binary" >&2
  exit 66
fi

highest=$(LANG=C readelf -W --version-info --dyn-syms "$binary" \
  | sed -nE 's/.*Name: GLIBC_([0-9]+\.[0-9]+).*/\1/p' \
  | sort -V \
  | tail -n 1)
if [[ -z $highest ]]; then
  printf 'Could not find versioned GLIBC symbols in %s\n' "$binary" >&2
  exit 1
fi

if [[ $(printf '%s\n%s\n' "$maximum" "$highest" | sort -V | tail -n 1) != "$maximum" ]]; then
  printf 'Binary requires GLIBC_%s; the declared maximum is %s\n' "$highest" "$maximum" >&2
  exit 1
fi

printf 'Highest GLIBC symbol version: %s (maximum %s)\n' "$highest" "$maximum"
