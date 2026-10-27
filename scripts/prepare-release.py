#!/usr/bin/env python3
"""Stage a release from attested, successful main-push CI candidates."""

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
from typing import Any

TARGETS = (
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
)
ROOT = Path(__file__).resolve().parents[1]
checksums = runpy.run_path(str(ROOT / "scripts/candidate-checksums.py"))["checksums"]


def command(*args: str) -> str:
    """Run a command without shell interpolation and retain errors."""
    return subprocess.check_output(args, text=True).strip()


def release_version(tag: str, manifest: str) -> str:
    """Require a normal SemVer tag that exactly matches the package version."""
    number = r"(?:0|[1-9][0-9]*)"
    if not re.fullmatch(rf"v{number}\.{number}\.{number}", tag):
        raise ValueError("Expected a normal SemVer tag such as v0.1.0")
    version = re.search(r'^version = "([^"]+)"$', manifest, re.MULTILINE)
    if version is None or tag != f"v{version[1]}":
        raise ValueError("Release tag must match Cargo.toml version")
    return tag[1:]


def check_run(run: dict[str, Any], sha: str, repo: str) -> None:
    """Reject runs from other commits, repositories, events or workflows."""
    expected = {
        "head_sha": sha,
        "head_branch": "main",
        "event": "push",
        "path": ".github/workflows/ci.yml",
        "status": "completed",
        "conclusion": "success",
    }
    if any(run.get(key) != value for key, value in expected.items()):
        raise ValueError("Candidates must come from successful main-push CI at the tag")
    if run.get("head_repository", {}).get("full_name") != repo:
        raise ValueError("Candidate repository does not match")


def stage_bundle(bundle: Path, output: Path, version: str, target: str) -> None:
    """Verify and copy a complete target, giving its SBOM a unique asset name."""
    checksums(bundle, True)
    names = {path.name for path in bundle.iterdir()}
    archive = f"bionic-reader-v{version}-{target}.tar.gz"
    wheels = [name for name in names if name.endswith(".whl")]
    packages = {
        name for name in names if name.endswith((".deb", ".rpm", ".pkg.tar.zst"))
    }
    node = f"bionic-reader-v{version}-{target}.npm.tgz"
    expected = {
        archive,
        node,
        "SHA256SUMS",
        "sbom.cdx.json",
        "library-performance.json",
        *wheels,
        *packages,
    }
    if names != expected or len(wheels) != 1:
        raise ValueError(f"Incomplete or unexpected target bundle: {target}")
    gnu = target.endswith("-linux-gnu")
    if len(packages) != (3 if gnu else 0) or (
        gnu
        and not all(
            sum(name.endswith(suffix) for name in packages) == 1
            for suffix in (".deb", ".rpm", ".pkg.tar.zst")
        )
    ):
        raise ValueError(f"Missing or unexpected system packages: {target}")
    registry = runpy.run_path(str(ROOT / "scripts/package-registries.py"))
    library = registry["read_package"](bundle / node)
    registry["node_package"]["validate_library"](library, version)
    report = json.loads((bundle / "library-performance.json").read_text())
    identity = json.loads(library["lib/package.json"][0])
    if (
        report.get("package_version") != version
        or report.get("artifact", {}).get("sha256") != identity["wasm_sha256"]
    ):
        raise ValueError("Performance report does not describe the bundled library")
    if (
        not report.get("cold")
        or not report.get("warm")
        or not report.get("environment")
    ):
        raise ValueError("Performance report is missing measurements or environment")
    for source in sorted(bundle.iterdir()):
        if source.name == "SHA256SUMS":
            continue
        name = (
            f"bionic-reader-v{version}-library-performance.json"
            if source.name == "library-performance.json"
            else f"bionic-reader-v{version}-{target}.sbom.cdx.json"
            if source.name == "sbom.cdx.json"
            else source.name
        )
        destination = output / name
        if destination.exists():
            if (
                source.name == "library-performance.json"
                and destination.read_bytes() == source.read_bytes()
            ):
                continue
            raise ValueError(f"Duplicate release asset: {name}")
        shutil.copyfile(source, destination)


def main() -> None:
    """Validate the tag/run, verify provenance, and assemble a release manifest."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("--run-id", default="")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    repo = os.environ["GITHUB_REPOSITORY"]
    # Validate before using the tag as a Git revision or an API argument.
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", args.tag):
        parser.error("Expected vMAJOR.MINOR.PATCH")
    sha = command("git", "rev-parse", f"refs/tags/{args.tag}^{{commit}}")
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", sha, "origin/main"], check=True
    )
    version = release_version(args.tag, command("git", "show", f"{sha}:Cargo.toml"))
    run_id = args.run_id
    if not run_id:
        runs = json.loads(
            command(
                "gh",
                "api",
                f"repos/{repo}/actions/workflows/ci.yml/runs"
                f"?head_sha={sha}&branch=main&event=push&status=success&per_page=1",
            )
        )["workflow_runs"]
        if not runs:
            raise ValueError(
                "No successful main CI run at the tag; wait for CI or rerun it"
            )
        run_id = str(runs[0]["id"])
    if not re.fullmatch(r"[0-9]+", run_id):
        raise ValueError("CI run ID must be numeric")
    run = json.loads(command("gh", "api", f"repos/{repo}/actions/runs/{run_id}"))
    check_run(run, sha, repo)
    # Require the main security workflows at this exact source commit too.
    for workflow in ("security.yml", "gitleaks.yml", "codeql.yml"):
        runs = json.loads(
            command(
                "gh",
                "api",
                f"repos/{repo}/actions/workflows/{workflow}/runs"
                f"?head_sha={sha}&branch=main&event=push&per_page=1",
            )
        )["workflow_runs"]
        if not runs or runs[0]["conclusion"] != "success":
            raise ValueError(f"Successful main {workflow} required at {sha}")
    args.output.mkdir(parents=True, exist_ok=True)
    if any(args.output.iterdir()):
        raise ValueError("Release output directory must be empty")
    with tempfile.TemporaryDirectory(prefix="bionic-reader-release-") as directory:
        for target in TARGETS:
            bundle = Path(directory) / target
            subprocess.run(
                [
                    "gh",
                    "run",
                    "download",
                    run_id,
                    "--repo",
                    repo,
                    "--name",
                    f"bionic-reader-{target}",
                    "--dir",
                    str(bundle),
                ],
                check=True,
            )
            checksums(bundle, True)
            for asset in sorted(bundle.iterdir()):
                if asset.name == "SHA256SUMS":
                    continue
                subprocess.run(
                    [
                        "gh",
                        "attestation",
                        "verify",
                        str(asset),
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
            stage_bundle(bundle, args.output, version, target)
    subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts/render-homebrew-formula.py"),
            "--version",
            version,
            "--artifacts",
            str(args.output),
            "--output",
            str(args.output / "bionic-reader.rb"),
        ],
        check=True,
    )
    registry_directory = Path(tempfile.mkdtemp(prefix="bionic-reader-registry-"))
    try:
        registry = runpy.run_path(str(ROOT / "scripts/package-registries.py"))
        registry["registry_packages"](args.output, registry_directory, version)
        for package in registry_directory.iterdir():
            shutil.copyfile(package, args.output / package.name)
    finally:
        shutil.rmtree(registry_directory)
    for target in ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"):
        arch = target.split("-", 1)[0]
        for suffix, label in (
            ("deb", "amd64" if arch == "x86_64" else "arm64"),
            ("rpm", arch),
            ("pkg.tar.zst", arch),
        ):
            candidates = list(args.output.glob(f"*{label}.{suffix}"))
            if len(candidates) != 1:
                raise ValueError(f"Expected one {suffix} for {target}")
            shutil.copyfile(
                candidates[0], args.output / f"bionic-reader-linux-{label}.{suffix}"
            )
    (args.output / "RELEASE-SOURCE.json").write_text(
        json.dumps(
            {"tag": args.tag, "commit": sha, "ci_run": run["html_url"]}, indent=2
        )
        + "\n",
        encoding="utf-8",
    )
    checksums(args.output, False)
    checksums(args.output, True)
    if "GITHUB_OUTPUT" in os.environ:
        with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
            output.write(f"tag={args.tag}\nsha={sha}\n")
    print(f"Verified release assets for {args.tag} from {sha}")


if __name__ == "__main__":
    main()
