#!/usr/bin/env python3
"""Stage and publish the exact Cargo candidate tested by release CI."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import runpy
import shutil
import subprocess
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path
from typing import Any

import tomllib


def command(*arguments: str, cwd: Path | None = None) -> str:
    """Read command output without shell interpolation."""
    return subprocess.check_output(arguments, cwd=cwd, text=True).strip()


def validate_crate(data: bytes, version: str, commit: str) -> str:
    """Bind an archive's package identity and clean VCS record to the release."""
    prefix = f"bionic-reader-{version}"
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        manifest = archive.extractfile(f"{prefix}/Cargo.toml")
        vcs = archive.extractfile(f"{prefix}/.cargo_vcs_info.json")
        if manifest is None or vcs is None:
            raise ValueError("Cargo candidate is missing identity records")
        with manifest, vcs:
            package = tomllib.loads(manifest.read().decode())["package"]
            identity = json.load(vcs)
    if package.get("name") != "bionic-reader" or package.get("version") != version:
        raise ValueError("Cargo candidate package/version differs from the release")
    if (
        identity.get("git", {}).get("sha1") != commit
        or identity.get("git", {}).get("dirty", False)
        or identity.get("path_in_vcs") != ""
    ):
        raise ValueError("Cargo candidate is not the clean release commit")
    return hashlib.sha256(data).hexdigest()


def stage_cargo(source: dict[str, Any], version: str, repo: str, output: Path) -> None:
    """Download the original successful main CI artifact and verify its digest."""
    match = re.fullmatch(
        rf"https://github\.com/{re.escape(repo)}/actions/runs/([0-9]+)",
        source.get("ci_run", ""),
    )
    if match is None:
        raise ValueError("Release source must identify this repository's CI run")
    run_id = match[1]
    run = json.loads(command("gh", "api", f"repos/{repo}/actions/runs/{run_id}"))
    runpy.run_path(str(Path(__file__).with_name("prepare-release.py")))["check_run"](
        run, source["commit"], repo
    )
    artifacts = json.loads(
        command(
            "gh", "api", f"repos/{repo}/actions/runs/{run_id}/artifacts?per_page=100"
        )
    )["artifacts"]
    candidates = [
        item for item in artifacts if item["name"] == "bionic-reader-cargo-crate"
    ]
    if len(candidates) != 1 or candidates[0]["expired"]:
        raise ValueError("Original CI Cargo candidate is missing or expired")
    artifact = candidates[0]
    with tempfile.TemporaryDirectory(
        prefix="bionic-reader-cargo-publication-"
    ) as directory:
        archive = Path(directory) / "candidate.zip"
        with archive.open("wb") as destination:
            subprocess.run(
                ["gh", "api", f"repos/{repo}/actions/artifacts/{artifact['id']}/zip"],
                stdout=destination,
                check=True,
            )
        digest = "sha256:" + hashlib.sha256(archive.read_bytes()).hexdigest()
        if artifact.get("digest") != digest:
            raise ValueError(
                "Cargo artifact digest differs from GitHub's recorded digest"
            )
        filename = f"bionic-reader-{version}.crate"
        with zipfile.ZipFile(archive) as bundle:
            if bundle.namelist() != [filename]:
                raise ValueError(
                    "Cargo artifact must contain exactly the release crate"
                )
            data = bundle.read(filename)
    checksum = validate_crate(data, version, source["commit"])
    output.mkdir()
    (output / filename).write_bytes(data)
    (output / "source.json").write_text(
        json.dumps(
            {
                "version": version,
                "commit": source["commit"],
                "sha256": checksum,
                "ci_run": source["ci_run"],
            }
        )
        + "\n"
    )
    shutil.copyfile(__file__, output / "publish-crates-io.py")


def load_candidate(publication: Path) -> dict[str, Any]:
    """Reject malformed metadata or modified staged crate contents."""
    source: dict[str, Any] = json.loads((publication / "source.json").read_text())
    if not re.fullmatch(
        r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", source["version"]
    ) or not re.fullmatch(r"[0-9a-f]{40}", source["commit"]):
        raise ValueError("Invalid Cargo release identity")
    data = (publication / f"bionic-reader-{source['version']}.crate").read_bytes()
    if validate_crate(data, source["version"], source["commit"]) != source["sha256"]:
        raise ValueError("Staged Cargo candidate checksum differs")
    return source


def already_published(source: dict[str, Any]) -> bool:
    """Skip a retry only when the registry has the exact tested crate bytes."""
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/bionic-reader/{source['version']}",
        headers={"User-Agent": "bionic-reader-release", "Cache-Control": "no-cache"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            version = json.load(response)["version"]
    except urllib.error.HTTPError as error:
        error.close()
        if error.code == 404:
            return False
        raise
    if version["checksum"] != source["sha256"]:
        raise ValueError(
            "Published crate has different bytes; versions cannot be replaced"
        )
    return True


def verify_source(source: dict[str, Any], checkout: Path) -> None:
    """Repackage the exact release checkout and compare it before authentication."""
    if command("git", "rev-parse", "HEAD", cwd=checkout) != source["commit"]:
        raise ValueError("Cargo checkout is not the verified release commit")
    subprocess.run(
        ["cargo", "package", "--locked", "--package", "bionic-reader"],
        cwd=checkout,
        check=True,
    )
    metadata = json.loads(
        command(
            "cargo",
            "metadata",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
            cwd=checkout,
        )
    )
    archive = (
        Path(metadata["target_directory"])
        / "package"
        / f"bionic-reader-{source['version']}.crate"
    )
    if hashlib.sha256(archive.read_bytes()).hexdigest() != source["sha256"]:
        raise ValueError("Repackaged release differs from the tested CI candidate")


def main() -> None:
    """Emit the checkout identity, verify without credentials, or publish using OIDC."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("identity", "verify", "publish"))
    parser.add_argument("--publication", type=Path, required=True)
    parser.add_argument("--source", type=Path, default=Path("source"))
    args = parser.parse_args()
    source = load_candidate(args.publication)
    if args.phase == "identity":
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write(f"commit={source['commit']}\n")
        return
    if args.phase == "verify":
        verify_source(source, args.source)
        published = already_published(source)
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write(f"published={str(published).lower()}\n")
        return
    if already_published(source):
        print(f"Verified existing bionic-reader {source['version']}")
        return
    if command("git", "rev-parse", "HEAD", cwd=args.source) != source["commit"]:
        raise ValueError("Cargo checkout changed after verification")
    subprocess.run(
        ["cargo", "publish", "--locked", "--package", "bionic-reader"],
        cwd=args.source,
        check=True,
    )
    for _ in range(15):
        if already_published(source):
            print(f"Published and verified bionic-reader {source['version']}")
            return
        time.sleep(4)
    raise RuntimeError(
        "Published version is not yet visible on crates.io; retry verification"
    )


if __name__ == "__main__":
    main()
