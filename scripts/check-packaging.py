#!/usr/bin/env python3
"""Validate package metadata invariants without contacting registries."""

from __future__ import annotations

import json
from pathlib import Path


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"packaging check failed: {message}")


def main() -> None:
    root = Path(__file__).resolve().parent.parent

    python_project = (root / "packaging/python/pyproject.toml").read_text(
        encoding="utf-8"
    )
    require(
        'bindings = "bin"' in python_project,
        "Python must wrap the Rust binary",
    )
    require("locked = true" in python_project, "Python builds must honor Cargo.lock")

    npm_source = (root / "packaging/npm/package.json.in").read_text(encoding="utf-8")
    npm_manifest = json.loads(npm_source.replace("@VERSION@", "0.0.0"))
    optional_dependencies = npm_manifest["optionalDependencies"]
    expected_packages = {
        "@arturoaviles/bionic-reader-darwin-x64",
        "@arturoaviles/bionic-reader-darwin-arm64",
        "@arturoaviles/bionic-reader-linux-x64-gnu",
        "@arturoaviles/bionic-reader-linux-x64-musl",
        "@arturoaviles/bionic-reader-linux-arm64-gnu",
        "@arturoaviles/bionic-reader-linux-arm64-musl",
    }
    require(
        set(optional_dependencies) == expected_packages,
        "npm must declare all six native targets",
    )
    lifecycle_scripts = npm_manifest.get("scripts", {})
    require(
        "install" not in lifecycle_scripts and "postinstall" not in lifecycle_scripts,
        "the npm launcher must not require lifecycle scripts",
    )
    require(
        npm_manifest["bin"]["bionic-reader"] == "bin/bionic-reader.js",
        "npm must install its thin launcher",
    )

    homebrew = (root / "packaging/homebrew/bionic-reader.rb.in").read_text(
        encoding="utf-8"
    )
    require(
        all(
            marker in homebrew
            for marker in (
                "@VERSION@",
                "@SHA256_MAC_X64@",
                "@SHA256_MAC_ARM64@",
                "@SHA256_LINUX_X64@",
                "@SHA256_LINUX_ARM64@",
            )
        ),
        "Homebrew formula must cover all four macOS and Linux native candidates",
    )
    require(
        'pkgshare.install Dir["share/bionic-reader/*"]' in homebrew,
        "Homebrew must install shared assets under pkgshare",
    )
    require(
        "completions/_bionic-reader" in homebrew,
        "Homebrew must install the generated Zsh completion",
    )

    for relative, markers in (
        (
            "packaging/repositories/apt/bionic-reader.sources.in",
            ("@REPOSITORY_URL@", "@SUITE@", "@ARCHITECTURES@"),
        ),
        ("packaging/repositories/dnf/bionic-reader.repo.in", ("@REPOSITORY_URL@",)),
        ("packaging/repositories/pacman/bionic-reader.conf.in", ("@REPOSITORY_URL@",)),
    ):
        template = (root / relative).read_text(encoding="utf-8")
        require(
            all(marker in template for marker in markers),
            f"repository template markers missing: {relative}",
        )

    snap = (root / "packaging/snap/snapcraft.yaml.in").read_text(encoding="utf-8")
    require(
        "confinement: classic" in snap, "Snap candidate must retain classic confinement"
    )
    require("grade: devel" in snap, "Snap candidate must remain a devel build")
    require(
        "@VERSION@" in snap and "@SOURCE_PATH@" in snap,
        "Snap render markers are incomplete",
    )

    linux_package = (root / "packaging/linux/nfpm.yaml").read_text(encoding="utf-8")
    require(
        "${INTEGRATIONS}" in linux_package,
        "Linux packages must carry integration assets",
    )
    require(
        "${CONFIG_EXAMPLE}" in linux_package,
        "Linux packages must carry the config example",
    )
    require(
        "${COMPLETIONS}" in linux_package, "Linux packages must carry shell completions"
    )
    for path in (
        "${COMPLETIONS}/bionic-reader.bash",
        "${COMPLETIONS}/_bionic-reader",
        "${COMPLETIONS}/bionic-reader.fish",
    ):
        require(path in linux_package, f"Linux completion source missing: {path}")
    for name in ("bionic-reader.bash", "_bionic-reader", "bionic-reader.fish"):
        require(
            (root / "completions" / name).is_file(),
            f"generated completion asset is missing: {name}",
        )
    for name in ("bash", "zsh", "fish"):
        require(
            f"bionic-reader completions {name}"
            in (root / "docs/SHELL.md").read_text(encoding="utf-8"),
            f"Shell guide must document completion generation for {name}",
        )

    print("Package metadata invariants passed.")


if __name__ == "__main__":
    main()
