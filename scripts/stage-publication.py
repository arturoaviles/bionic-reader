#!/usr/bin/env python3
"""Verify a published release and stage independent package publishing inputs."""

from __future__ import annotations

import argparse
import json
import os
import re
import runpy
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
checksums = runpy.run_path(str(ROOT / "scripts/candidate-checksums.py"))["checksums"]
registry = runpy.run_path(str(ROOT / "scripts/package-registries.py"))


def validate_source(
    tag: str, sha: str, release: dict[str, object], source: dict[str, object]
) -> str:
    """Reject drafts, prereleases, moved tags and mismatched source records."""
    if not re.fullmatch(
        r"v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", tag
    ):
        raise ValueError("Expected a normal version tag")
    if (
        release.get("isDraft") is not False
        or release.get("isPrerelease") is not False
        or release.get("tagName") != tag
    ):
        raise ValueError("Only a published normal release can publish packages")
    if source.get("tag") != tag or source.get("commit") != sha:
        raise ValueError("Release source record does not match the current tag")
    return tag[1:]


def main() -> None:
    """Reverify candidate provenance before credentials enter publishing jobs."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", args.tag):
        parser.error("Expected vMAJOR.MINOR.PATCH")
    repo = os.environ["GITHUB_REPOSITORY"]

    def gh(*arguments: str) -> str:
        return subprocess.check_output(["gh", *arguments], text=True).strip()

    release = json.loads(
        gh(
            "release",
            "view",
            args.tag,
            "--repo",
            repo,
            "--json",
            "isDraft,isPrerelease,tagName",
        )
    )
    sha = gh("api", f"repos/{repo}/commits/{args.tag}", "--jq", ".sha")
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", sha, "origin/main"], check=True
    )
    args.output.mkdir(parents=True, exist_ok=True)
    if any(args.output.iterdir()):
        raise ValueError("Publication output must be empty")
    with tempfile.TemporaryDirectory(prefix="bionic-reader-publish-") as directory:
        bundle = Path(directory) / "release"
        subprocess.run(
            [
                "gh",
                "release",
                "download",
                args.tag,
                "--repo",
                repo,
                "--dir",
                str(bundle),
            ],
            check=True,
        )
        checksums(bundle, True)
        source = json.loads((bundle / "RELEASE-SOURCE.json").read_text())
        version = validate_source(args.tag, sha, release, source)
        for target in registry["TARGETS"]:
            for name in (
                f"bionic-reader-v{version}-{target}.tar.gz",
                f"bionic-reader-v{version}-{target}.npm.tgz",
            ):
                subprocess.run(
                    [
                        "gh",
                        "attestation",
                        "verify",
                        str(bundle / name),
                        "--repo",
                        repo,
                        "--signer-workflow",
                        f"{repo}/.github/workflows/ci.yml",
                        "--source-digest",
                        sha,
                        "--deny-self-hosted-runners",
                    ],
                    check=True,
                )
        wheels = sorted(bundle.glob("*.whl"))
        if len(wheels) != 6:
            raise ValueError("Expected all six platform wheels")
        for wheel in wheels:
            subprocess.run(
                [
                    "gh",
                    "attestation",
                    "verify",
                    str(wheel),
                    "--repo",
                    repo,
                    "--signer-workflow",
                    f"{repo}/.github/workflows/ci.yml",
                    "--source-digest",
                    sha,
                    "--deny-self-hosted-runners",
                ],
                check=True,
            )
        npm = args.output / "npm"
        registry["registry_packages"](bundle, npm, version)
        # The public tarballs must be identical to the rederived verified inputs.
        for package in npm.iterdir():
            if package.read_bytes() != (bundle / package.name).read_bytes():
                raise ValueError(
                    f"Registry payload differs from verified candidates: {package.name}"
                )
        pypi, homebrew = args.output / "pypi", args.output / "homebrew"
        pypi.mkdir()
        homebrew.mkdir()
        for wheel in wheels:
            shutil.copyfile(wheel, pypi / wheel.name)
        subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts/render-homebrew-formula.py"),
                "--version",
                version,
                "--artifacts",
                str(bundle),
                "--output",
                str(homebrew / "bionic-reader.rb"),
            ],
            check=True,
        )
        shutil.copyfile(ROOT / "scripts/publish-npm.py", npm / "publish-npm.py")
        for channel in (npm, pypi, homebrew):
            checksums(channel, False)
    print(f"Verified publication inputs for {args.tag} at {sha}")


if __name__ == "__main__":
    main()
