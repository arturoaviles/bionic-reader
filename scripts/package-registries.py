#!/usr/bin/env python3
"""Repack verified standalone target packages for native npm registry installs."""

from __future__ import annotations

import argparse
import gzip
import io
import json
import runpy
import tarfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
TARGETS = runpy.run_path(str(ROOT / "scripts/package-release-node.py"))["TARGETS"]


def read_package(path: Path) -> dict[str, tuple[bytes, int]]:
    """Read regular package members without extracting links or unsafe paths."""
    files = {}
    with tarfile.open(path, "r:gz") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            name = PurePosixPath(member.name)
            if (
                not member.isfile()
                or name.is_absolute()
                or ".." in name.parts
                or name.parts[0] != "package"
            ):
                raise ValueError(f"Unsafe package member: {member.name}")
            relative = str(name.relative_to("package"))
            if relative in files:
                raise ValueError(f"Duplicate package member: {member.name}")
            stream = archive.extractfile(member)
            if stream is None:
                raise ValueError(f"Missing package member: {member.name}")
            with stream:
                files[relative] = (stream.read(), member.mode)
    return files


def write_package(path: Path, files: dict[str, tuple[bytes, int]]) -> None:
    """Write deterministic registry tarballs without running npm or package code."""
    with path.open("wb") as raw:
        with gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed:
            with tarfile.open(
                fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT
            ) as archive:
                for name, (data, mode) in sorted(files.items()):
                    member = tarfile.TarInfo(f"package/{name}")
                    member.size, member.mode = len(data), mode
                    archive.addfile(member, io.BytesIO(data))


def registry_packages(source: Path, output: Path, version: str) -> None:
    """Build a launcher and six optional native packages from verified CI bytes."""
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError("Registry package output must be empty")
    optional = {}
    base_files = None
    repository = {
        "type": "git",
        "url": "git+https://github.com/arturoaviles/bionic-reader.git",
    }
    for target, (system, cpu, libc) in TARGETS.items():
        files = read_package(source / f"bionic-reader-v{version}-{target}.npm.tgz")
        manifest = json.loads(files.pop("package.json")[0])
        if (
            manifest.get("version") != version
            or manifest.get("os") != [system]
            or manifest.get("cpu") != [cpu]
        ):
            raise ValueError(f"Native package identity mismatch: {target}")
        binary = files.pop("bin/native/bionic-reader")
        if not binary[1] & 0o111:
            raise ValueError(f"Nonexecutable native package: {target}")
        if base_files is None:
            base_files = files
        elif base_files != files:
            raise ValueError("Standalone packages disagree on shared launcher/assets")
        suffix = f"{system}-{cpu}"
        if libc:
            suffix += "-gnu" if libc == "glibc" else "-musl"
        name = f"@arturoaviles/bionic-reader-{suffix}"
        optional[name] = version
        native = {
            "name": name,
            "version": version,
            "license": "MIT",
            "repository": repository,
            "os": [system],
            "cpu": [cpu],
            "preferUnplugged": True,
        }
        if libc:
            native["libc"] = [libc]
        write_package(
            output / f"bionic-reader-npm-native-{suffix}-v{version}.tgz",
            {
                "package.json": (json.dumps(native, indent=2).encode() + b"\n", 0o644),
                "bin/bionic-reader": binary,
                "LICENSE": files["LICENSE"],
            },
        )
    if base_files is None:
        raise ValueError("No native packages")
    base = {
        "name": "@arturoaviles/bionic-reader",
        "version": version,
        "license": "MIT",
        "repository": repository,
        "engines": {"node": ">=18"},
        "bin": {"bionic-reader": "bin/bionic-reader.js"},
        "optionalDependencies": optional,
    }
    base_files["package.json"] = (json.dumps(base, indent=2).encode() + b"\n", 0o644)
    write_package(output / f"bionic-reader-npm-base-v{version}.tgz", base_files)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    registry_packages(args.directory, args.output, args.version)
