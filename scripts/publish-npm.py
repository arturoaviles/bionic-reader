#!/usr/bin/env python3
"""Publish native packages before their launcher, safely resuming partial uploads."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import subprocess
import tarfile
from pathlib import Path


def check_existing(path: Path, integrity: str) -> None:
    """Skip an existing immutable version only if its full tarball digest matches."""
    digest = base64.b64encode(hashlib.sha512(path.read_bytes()).digest()).decode()
    if integrity != f"sha512-{digest}":
        raise ValueError(f"Published package differs from this release: {path.name}")


def publish(directory: Path, registry: str, provenance: bool) -> None:
    """Refuse overwrites and preserve actual registry errors instead of hiding them."""
    native = sorted(directory.glob("*-native-*.tgz"))
    base = list(directory.glob("*-base-*.tgz"))
    if len(native) != 6 or len(base) != 1:
        raise ValueError("Expected six native packages and one launcher")
    for path in [*native, *base]:
        with tarfile.open(path, "r:gz") as archive:
            source = archive.extractfile("package/package.json")
            if source is None:
                raise ValueError("Missing package identity")
            with source:
                identity = json.load(source)
        name = f"{identity['name']}@{identity['version']}"
        existing = subprocess.run(
            ["npm", "view", name, "dist.integrity", "--json", "--registry", registry],
            capture_output=True,
            text=True,
        )
        if existing.returncode == 0:
            check_existing(path, json.loads(existing.stdout))
            print(f"Already published with matching digest: {name}")
            continue
        if "E404" not in existing.stderr:
            raise RuntimeError(existing.stderr)
        command = [
            "npm",
            "publish",
            str(path),
            "--access",
            "public",
            "--ignore-scripts",
            "--registry",
            registry,
        ]
        if provenance:
            command.append("--provenance")
        subprocess.run(command, check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--registry", required=True)
    parser.add_argument("--provenance", action="store_true")
    args = parser.parse_args()
    publish(args.directory, args.registry, args.provenance)
