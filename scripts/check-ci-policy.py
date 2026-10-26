#!/usr/bin/env python3
"""Check the invariants that make the aggregate CI gate meaningful."""

import re
from pathlib import Path


def check_gate_config(workflow: str) -> None:
    """Require the gate to depend on every PR job, with no missing job IDs."""
    jobs = workflow.split("\njobs:\n", 1)[1]
    names = set(re.findall(r"^  ([A-Za-z_][A-Za-z0-9_-]*):$", jobs, re.MULTILINE))
    gate = jobs.split("\n  ci-gate:\n", 1)[1]
    dependency_match = re.search(r"^    needs: \[([^]]+)\]$", gate, re.MULTILINE)
    if dependency_match is None:
        raise ValueError("CI gate must declare an explicit dependency list")
    dependencies = set(dependency_match[1].replace(" ", "").split(","))
    # Attestations intentionally never run for PRs and have a separate trust boundary.
    if dependencies != names - {"ci-gate", "attest-candidates"}:
        raise ValueError("CI gate must aggregate every PR job")
    if "    if: always()\n" not in gate:
        raise ValueError("CI gate must run after failures and cancellations")
    command = re.search(r"scripts/check-ci-gate.py (.+)", gate)
    if command is None or set(command[1].split()) != dependencies:
        raise ValueError("CI gate's expected jobs must match needs")


def check_toolchain(manifest: str, toolchain: str) -> None:
    """Keep the selected local compiler equal to the published MSRV."""
    minimum = re.search(r'^rust-version = "([^"]+)"$', manifest, re.MULTILINE)
    selected = re.search(r'^channel = "([^"]+)"$', toolchain, re.MULTILINE)
    if minimum is None or selected is None or minimum[1] != selected[1]:
        raise ValueError("rust-toolchain.toml must match Cargo.toml rust-version")


if __name__ == "__main__":
    check_toolchain(
        Path("Cargo.toml").read_text(), Path("rust-toolchain.toml").read_text()
    )
    check_gate_config(Path(".github/workflows/ci.yml").read_text())
    print("Toolchain selection and CI gate dependencies match.")
