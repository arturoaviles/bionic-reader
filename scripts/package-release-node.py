#!/usr/bin/env python3
"""Create a standalone, registry-free Node package from one native archive."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import re
import tarfile
from pathlib import Path, PurePosixPath

TARGETS = {
    "x86_64-apple-darwin": ("darwin", "x64", None),
    "aarch64-apple-darwin": ("darwin", "arm64", None),
    "x86_64-unknown-linux-gnu": ("linux", "x64", "glibc"),
    "aarch64-unknown-linux-gnu": ("linux", "arm64", "glibc"),
    "x86_64-unknown-linux-musl": ("linux", "x64", "musl"),
    "aarch64-unknown-linux-musl": ("linux", "arm64", "musl"),
}

LIBRARY_FILES = {
    "lib/package.json",
    "lib/version.js",
    "lib/index.d.ts",
    "lib/runtime.js",
    "lib/node.js",
    "lib/node.cjs",
    "lib/browser.js",
    "lib/react.js",
    "lib/react.cjs",
    "lib/react.d.ts",
    "lib/wasm/engine.js",
    "lib/wasm/engine.d.ts",
    "lib/wasm/engine_bg.wasm",
    "lib/wasm/engine_bg.wasm.d.ts",
}


def validate_library(files: dict[str, tuple[bytes, int]], version: str) -> None:
    """Reject incomplete, stale, or corrupted compiled library payloads."""
    missing = LIBRARY_FILES - files.keys()
    if missing:
        raise ValueError("Missing library payload: " + ", ".join(sorted(missing)))
    identity = json.loads(files["lib/package.json"][0])
    wasm = files["lib/wasm/engine_bg.wasm"][0]
    if identity.get("version") != version:
        raise ValueError("Library payload version does not match the package")
    if (
        not wasm.startswith(b"\0asm\x01\0\0\0")
        or identity.get("wasm_sha256") != hashlib.sha256(wasm).hexdigest()
    ):
        raise ValueError("Library WebAssembly payload is invalid or has changed")
    expected = "export const VERSION = " + json.dumps(version) + ";\n"
    if files["lib/version.js"][0] != expected.encode():
        raise ValueError("Library JavaScript version does not match the package")


def package(archive: Path, output: Path, library: Path | None = None) -> Path:
    """Bundle the native executable, launcher, and assets without dependencies."""
    match = re.fullmatch(
        r"bionic-reader-v([0-9]+\.[0-9]+\.[0-9]+)-(.+)\.tar\.gz", archive.name
    )
    if match is None or match[2] not in TARGETS:
        raise ValueError("Expected a supported versioned native archive")
    version, target = match.groups()
    system, cpu, libc = TARGETS[target]
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads(
        (root / "packaging/npm/package.json.in")
        .read_text()
        .replace("@VERSION@", version)
    )
    manifest.pop("optionalDependencies", None)
    manifest.update({"os": [system], "cpu": [cpu], "preferUnplugged": True})
    if libc is not None:
        # Static musl executables also run on glibc hosts. Registry-native
        # dependencies retain their stricter libc selector in package-registries.py.
        manifest["libc"] = ["musl", "glibc"] if libc == "musl" else [libc]
    launcher = (
        Path(__file__).resolve().parents[1] / "packaging/npm/bin/bionic-reader.js"
    )
    files = {
        "package.json": (json.dumps(manifest, indent=2).encode() + b"\n", 0o644),
        "bin/bionic-reader.js": (launcher.read_bytes(), 0o755),
    }
    library = library or root / "dist/library/lib"
    if not (library / "wasm/engine_bg.wasm").is_file():
        raise ValueError("Build the WebAssembly library payload before Node packaging")
    for path in sorted(library.rglob("*")):
        if path.is_file():
            name = "lib/" + path.relative_to(library).as_posix()
            if name in LIBRARY_FILES:
                files[name] = (path.read_bytes(), 0o644)
    validate_library(files, version)
    top = f"bionic-reader-{version}-{target}/"
    with tarfile.open(archive, "r:gz") as source:
        for member in source.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not member.name.startswith(top):
                raise ValueError(f"Unexpected archive member: {member.name}")
            relative = member.name[len(top) :]
            if ".." in PurePosixPath(relative).parts or relative.startswith("/"):
                raise ValueError(f"Unsafe archive member: {member.name}")
            if relative == "bin/bionic-reader":
                name, mode = "bin/native/bionic-reader", 0o755
            elif relative in ("LICENSE", "README.md") or relative.startswith(
                "share/bionic-reader/"
            ):
                name, mode = relative, 0o644
            else:
                continue
            stream = source.extractfile(member)
            if stream is None:
                raise ValueError(f"Missing member content: {member.name}")
            with stream:
                files[name] = (stream.read(), mode)
    if "bin/native/bionic-reader" not in files:
        raise ValueError("Native archive has no executable")
    output.mkdir(parents=True, exist_ok=True)
    destination = output / f"bionic-reader-v{version}-{target}.npm.tgz"
    with destination.open("wb") as raw:
        with gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed:
            with tarfile.open(
                fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT
            ) as result:
                for name, (data, mode) in sorted(files.items()):
                    info = tarfile.TarInfo(f"package/{name}")
                    info.size, info.mode = len(data), mode
                    result.addfile(info, io.BytesIO(data))
    return destination


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--library",
        type=Path,
        help="compiled library directory from the same candidate",
    )
    args = parser.parse_args()
    print(package(args.archive, args.output, args.library))
