#!/usr/bin/env python3
"""Create a stable gzip tar archive around a built bionic-reader executable."""

from __future__ import annotations

import argparse
import gzip
import os
import tarfile
import tempfile
from pathlib import Path


def add_file(archive: tarfile.TarFile, source: Path, name: str, epoch: int) -> None:
    info = archive.gettarinfo(str(source), arcname=name)
    info.mtime = epoch
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    info.pax_headers = {}
    with source.open("rb") as stream:
        archive.addfile(info, stream)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--epoch", required=True, type=int)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--root", required=True, type=Path, help="repository root")
    args = parser.parse_args()

    binary = args.binary.resolve(strict=True)
    root = args.root.resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error(f"{binary} is not an executable file")

    files = [
        (binary, "bin/bionic-reader"),
        (root / "README.md", "README.md"),
        (root / "LICENSE", "LICENSE"),
        (
            root / "packaging/config.example.toml",
            "share/bionic-reader/config.example.toml",
        ),
    ]
    for asset_directory in ("completions", "integrations"):
        asset_root = root / asset_directory
        if not asset_root.is_dir():
            continue
        files.extend(
            (
                source,
                f"share/bionic-reader/{asset_directory}/{source.relative_to(asset_root).as_posix()}",
            )
            for source in asset_root.rglob("*")
            if source.is_file() and not source.is_symlink()
        )
    for source, _ in files:
        if not source.is_file():
            parser.error(f"required archive input is missing: {source}")

    top = f"bionic-reader-{args.version}-{args.target}"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    output = args.output.resolve()
    with tempfile.NamedTemporaryFile(
        prefix=f".{output.name}.", suffix=".tmp", dir=output.parent, delete=False
    ) as temporary:
        temp_path = Path(temporary.name)
        try:
            with gzip.GzipFile(
                filename="",
                mode="wb",
                fileobj=temporary,
                mtime=args.epoch,
                compresslevel=9,
            ) as compressed:
                with tarfile.open(
                    fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT
                ) as archive:
                    directory = tarfile.TarInfo(top)
                    directory.type = tarfile.DIRTYPE
                    directory.mode = 0o755
                    directory.mtime = args.epoch
                    directory.uid = 0
                    directory.gid = 0
                    directory.uname = ""
                    directory.gname = ""
                    archive.addfile(directory)
                    for source, relative_name in sorted(
                        files, key=lambda item: item[1]
                    ):
                        add_file(archive, source, f"{top}/{relative_name}", args.epoch)
            os.replace(temp_path, output)
        except BaseException:
            temp_path.unlink(missing_ok=True)
            raise


if __name__ == "__main__":
    main()
