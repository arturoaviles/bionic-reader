#!/usr/bin/env python3
"""Build a platform-independent npm API candidate from the verified library payload."""

from __future__ import annotations

import argparse
import json
import runpy
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    node = runpy.run_path(str(root / "scripts/package-release-node.py"))
    registry = runpy.run_path(str(root / "scripts/package-registries.py"))
    manifest = json.loads(
        (root / "packaging/npm/package.json.in")
        .read_text()
        .replace("@VERSION@", args.version)
    )
    # This internal API-only candidate also runs on Windows without native CLI packages.
    # Full release packages retain the six optional native dependencies.
    manifest.pop("optionalDependencies", None)
    files = {
        "package.json": (json.dumps(manifest, indent=2).encode() + b"\n", 0o644),
        "bin/bionic-reader.js": (
            (root / "packaging/npm/bin/bionic-reader.js").read_bytes(),
            0o755,
        ),
        "LICENSE": ((root / "LICENSE").read_bytes(), 0o644),
    }
    for name in node["LIBRARY_FILES"]:
        files[name] = ((root / "dist/library" / name).read_bytes(), 0o644)
    node["validate_library"](files, args.version)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    registry["write_package"](args.output, files)
    print(args.output)


if __name__ == "__main__":
    main()
