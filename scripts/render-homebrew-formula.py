#!/usr/bin/env python3
"""Render a release formula or one for a local host-native archive."""

from __future__ import annotations

import argparse
import hashlib
import re
from pathlib import Path

LOCAL_TARGETS = (
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
)


def formula_class(name: str) -> str:
    """Convert a lower-case Homebrew formula name to its Ruby class name."""
    if not re.fullmatch(r"[a-z][a-z0-9+._-]*", name):
        raise ValueError(
            "formula name must start with a letter and use lower-case "
            "Homebrew characters"
        )
    parts = re.split(r"[+._-]+", name)
    return "".join(part[0].upper() + part[1:] for part in parts if part)


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
    parser.add_argument(
        "--local-target",
        choices=LOCAL_TARGETS,
        help="render a local-install formula for one host-native archive",
    )
    parser.add_argument(
        "--formula-name",
        default="bionic-reader",
        help="formula name for --local-target (default: bionic-reader)",
    )
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    if args.local_target:
        archive = (
            args.artifacts / f"bionic-reader-v{args.version}-{args.local_target}.tar.gz"
        )
        try:
            archive = archive.resolve(strict=True)
            if not archive.is_file():
                parser.error(f"local archive is not a file: {archive}")
            class_name = formula_class(args.formula_name)
        except (OSError, ValueError) as error:
            parser.error(str(error))
        template_path = args.template or (
            root / "packaging/homebrew" / "bionic-reader-local.rb.in"
        )
        template = template_path.read_text(encoding="utf-8")
        substitutions = {
            "@VERSION@": args.version,
            "@FORMULA_CLASS@": class_name,
            "@ARCHIVE_URL@": archive.as_uri(),
            "@SHA256@": digest(archive),
        }
    else:
        if args.formula_name != "bionic-reader":
            parser.error("--formula-name requires --local-target")
        template_path = args.template or (
            root / "packaging/homebrew" / "bionic-reader.rb.in"
        )
        template = template_path.read_text(encoding="utf-8")
        substitutions = {
            "@VERSION@": args.version,
            "@SHA256_MAC_X64@": digest(
                args.artifacts
                / f"bionic-reader-v{args.version}-x86_64-apple-darwin.tar.gz"
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
