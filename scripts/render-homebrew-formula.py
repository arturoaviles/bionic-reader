#!/usr/bin/env python3
"""Render a Homebrew formula from the four macOS and GNU Linux archives."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--artifacts", required=True, type=Path)
    parser.add_argument("--template", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    template_path = args.template or root / "packaging/homebrew/bionic-reader.rb.in"
    template = template_path.read_text(encoding="utf-8")
    substitutions = {
        "@VERSION@": args.version,
        "@SHA256_MAC_X64@": digest(
            args.artifacts / f"bionic-reader-v{args.version}-x86_64-apple-darwin.tar.gz"
        ),
        "@SHA256_MAC_ARM64@": digest(
            args.artifacts
            / f"bionic-reader-v{args.version}-aarch64-apple-darwin.tar.gz"
        ),
        "@SHA256_LINUX_X64@": digest(
            args.artifacts
            / f"bionic-reader-v{args.version}-x86_64-unknown-linux-gnu.tar.gz"
        ),
        "@SHA256_LINUX_ARM64@": digest(
            args.artifacts
            / f"bionic-reader-v{args.version}-aarch64-unknown-linux-gnu.tar.gz"
        ),
    }
    for marker, value in substitutions.items():
        template = template.replace(marker, value)
    if "@" in template:
        parser.error("the Homebrew template contains an unresolved marker")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(template, encoding="utf-8")


if __name__ == "__main__":
    main()
