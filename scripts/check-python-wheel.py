#!/usr/bin/env python3
"""Check a wheel candidate contains the binary and shared integration assets."""

from __future__ import annotations

import argparse
import zipfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("wheel", type=Path)
    args = parser.parse_args()

    with zipfile.ZipFile(args.wheel) as wheel:
        files = set(wheel.namelist())
    required_suffixes = {
        ".data/scripts/bionic-reader",
        ".data/data/share/bionic-reader/config.example.toml",
        ".data/data/share/bionic-reader/integrations/bash.sh",
        ".data/data/share/bionic-reader/integrations/zsh.sh",
        ".data/data/share/bionic-reader/integrations/fish.fish",
        ".data/data/share/bionic-reader/completions/bionic-reader.bash",
        ".data/data/share/bionic-reader/completions/_bionic-reader",
        ".data/data/share/bionic-reader/completions/bionic-reader.fish",
    }
    missing = [
        suffix
        for suffix in required_suffixes
        if not any(name.endswith(suffix) for name in files)
    ]
    if missing:
        raise SystemExit(
            f"wheel candidate is missing required files: {', '.join(sorted(missing))}"
        )


if __name__ == "__main__":
    main()
