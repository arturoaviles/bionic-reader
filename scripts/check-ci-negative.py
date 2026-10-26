#!/usr/bin/env python3
"""Exercise real policy tools with deliberately invalid disposable inputs."""

import os
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def reject(command: list[str], diagnostic: str, directory: Path) -> None:
    """Require the intended finding, not merely a missing tool or setup failure."""
    result = subprocess.run(
        command,
        cwd=directory,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
        timeout=120,
    )
    if result.returncode == 0 or diagnostic not in result.stdout:
        raise RuntimeError(f"Expected rejection ({diagnostic}): {result.stdout}")
    print(f"Rejected as expected: {diagnostic}")


def crate(
    directory: Path,
    name: str,
    license_expression: str,
    dependencies: str = "",
    version: str = "0.1.0",
    offline: bool = True,
) -> None:
    """Create a small standalone Cargo fixture without touching the checkout."""
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "src").mkdir()
    (directory / "src/lib.rs").write_text("pub fn fixture() {}\n", encoding="utf-8")
    (directory / "Cargo.toml").write_text(
        f'[package]\nname = "{name}"\nversion = "{version}"\nedition = "2021"\n'
        f'license = "{license_expression}"\n[dependencies]\n{dependencies}\n',
        encoding="utf-8",
    )
    subprocess.run(
        ["cargo", "generate-lockfile"] + (["--offline"] if offline else []),
        cwd=directory,
        check=True,
        timeout=120,
    )


def main() -> None:
    """Run negative cases using the same configs and invocations as CI."""
    with tempfile.TemporaryDirectory(prefix="bionic-ci-negative-") as temporary:
        root = Path(temporary)
        markdown = root / "broken.md"
        markdown.write_text(
            "# Present\n\n[broken](#missing-anchor)\n", encoding="utf-8"
        )
        reject(
            [
                "lychee",
                "--config",
                str(ROOT / ".lychee.toml"),
                "--offline",
                "--no-progress",
                str(markdown),
            ],
            "missing-anchor",
            root,
        )
        yaml = root / "duplicate.yml"
        yaml.write_text("---\nkey: one\nkey: two\n", encoding="utf-8")
        reject(
            ["yamllint", "-c", str(ROOT / ".yamllint.yml"), str(yaml)],
            "duplication of key",
            root,
        )
        workflow = root / "unsafe.yml"
        workflow.write_text(
            "name: Unsafe\non: pull_request\npermissions: {}\njobs:\n"
            "  check:\n    runs-on: ubuntu-latest\n    steps:\n"
            '      - run: echo "${{ github.event.pull_request.title }}"\n',
            encoding="utf-8",
        )
        reject(
            ["zizmor", "--offline", "--format", "github", str(workflow)],
            "template-injection",
            root,
        )
        deny = ["cargo", "deny", "--locked", "--config", str(ROOT / "deny.toml")]
        forbidden = root / "forbidden"
        crate(forbidden, "license-fixture", "GPL-3.0-only")
        reject(deny + ["check", "licenses"], "rejected", forbidden)
        for version in ("0.4.10", "0.4.11", "0.4.14"):
            other_fuzzer = root / f"fuzzer-{version}"
            crate(other_fuzzer, "libfuzzer-sys", "NCSA", version=version)
            reject(deny + ["check", "licenses"], "rejected", other_fuzzer)
        other_ncsa = root / "other-ncsa"
        crate(other_ncsa, "license-fixture", "NCSA", version="0.4.13")
        reject(deny + ["check", "licenses"], "rejected", other_ncsa)
        wildcard = root / "wildcard"
        # Use the already fetched application dependency without network access.
        crate(wildcard, "wildcard-fixture", "MIT", 'libc = "*"')
        reject(deny + ["check", "bans"], "wildcard", wildcard)
        source = root / "git-source"
        crate(source, "git-fixture", "MIT")
        for command in (
            ["git", "init", "-q"],
            ["git", "add", "."],
            [
                "git",
                "-c",
                "user.name=CI fixture",
                "-c",
                "user.email=ci@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        ):
            subprocess.run(command, cwd=source, check=True, timeout=30)
        consumer = root / "consumer"
        crate(
            consumer,
            "consumer",
            "MIT",
            f'git-fixture = {{ git = "{source.as_uri()}" }}',
            offline=False,
        )
        reject(deny + ["check", "sources"], "source-not-allowed", consumer)
    print("All deliberately invalid CI inputs were rejected.")


if __name__ == "__main__":
    # Retain caller toolchain/cache configuration for standalone Cargo fixtures.
    os.chdir(ROOT)
    main()
