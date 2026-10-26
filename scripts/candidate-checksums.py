#!/usr/bin/env python3
"""Write or verify a complete, flat candidate bundle without following symlinks."""

import argparse
import hashlib
import re
from pathlib import Path


def digest(path: Path) -> str:
    """Hash a candidate without loading the entire artifact into memory."""
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def candidate_files(directory: Path) -> list[Path]:
    """Return regular files with safe relative names in a flat bundle."""
    files = []
    for path in sorted(directory.iterdir()):
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Unexpected bundle entry: {path}")
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.+-]*", path.name):
            raise ValueError(f"Unsafe candidate name: {path.name}")
        if path.name != "SHA256SUMS":
            files.append(path)
    if not files:
        raise ValueError("Empty candidate bundle")
    return files


def checksums(directory: Path, verify: bool) -> None:
    """Require the manifest to cover exactly the bundle's files and digests."""
    expected = "".join(f"{digest(p)}  {p.name}\n" for p in candidate_files(directory))
    manifest = directory / "SHA256SUMS"
    if verify:
        if manifest.read_text(encoding="utf-8") != expected:
            raise ValueError(f"Checksum mismatch or incomplete manifest: {manifest}")
    else:
        manifest.write_text(expected, encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--verify", action="store_true")
    arguments = parser.parse_args()
    checksums(arguments.directory, arguments.verify)
