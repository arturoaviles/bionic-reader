#!/usr/bin/env python3
"""Prepare merge enforcement only after the new workflows pass on PR and main."""

import argparse
import base64
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "arturoaviles/bionic-reader"
RULESET = 25737761


def api(endpoint: str) -> Any:
    """Read repository settings without exposing authentication material."""
    return json.loads(subprocess.check_output(["gh", "api", endpoint], text=True))


def successful_checks(sha: str, required: set[str]) -> None:
    """Require exact, completed GitHub Actions contexts on one commit."""
    pages = json.loads(
        subprocess.check_output(
            [
                "gh",
                "api",
                "--paginate",
                "--slurp",
                f"repos/{REPOSITORY}/commits/{sha}/check-runs?filter=latest&per_page=100",
            ],
            text=True,
        )
    )
    checks = {
        check["name"]: check
        for page in pages
        for check in page["check_runs"]
        if check["app"]["id"] == 15368
    }
    bad = [
        name
        for name in sorted(required)
        if name not in checks
        or checks[name]["status"] != "completed"
        or checks[name]["conclusion"] != "success"
    ]
    if bad:
        raise RuntimeError(f"Required checks have not passed on {sha}: {bad}")


def main(tested_sha: str, apply: bool) -> None:
    """Snapshot current settings and apply only after every rollout prerequisite."""
    proposal_path = ROOT / "docs/ci/main-ruleset-proposed.json"
    proposal = json.loads(proposal_path.read_text(encoding="utf-8"))
    status_rule = next(
        rule for rule in proposal["rules"] if rule["type"] == "required_status_checks"
    )
    required = {
        item["context"] for item in status_rule["parameters"]["required_status_checks"]
    }
    successful_checks(tested_sha, required)
    main_sha = api(f"repos/{REPOSITORY}/commits/main")["sha"]
    successful_checks(main_sha, required - {"Dependency Review"})
    for workflow in ("ci.yml", "security.yml", "codeql.yml", "gitleaks.yml"):
        remote = api(
            f"repos/{REPOSITORY}/contents/.github/workflows/{workflow}?ref=main"
        )
        if (
            base64.b64decode(remote["content"])
            != (ROOT / ".github/workflows" / workflow).read_bytes()
        ):
            raise RuntimeError(f"The reviewed {workflow} has not reached main")
    runs = api(
        f"repos/{REPOSITORY}/actions/workflows/ci.yml/runs?branch=main&event=push&per_page=10"
    )
    if not any(
        run["head_sha"] == main_sha and run["conclusion"] == "success"
        for run in runs["workflow_runs"]
    ):
        raise RuntimeError(
            "Main CI, including candidate attestations, must succeed first"
        )
    current = api(f"repos/{REPOSITORY}/rulesets/{RULESET}")
    if any(rule not in proposal["rules"] for rule in current["rules"]):
        raise RuntimeError(
            "Rules changed since the proposal; reconcile before applying"
        )
    if current.get("bypass_actors", []) != proposal["bypass_actors"]:
        raise RuntimeError("Bypass actors changed since the proposal")
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    snapshot = ROOT / "target/ci-rollout" / f"main-ruleset-{timestamp}.json"
    snapshot.parent.mkdir(parents=True, exist_ok=True)
    snapshot.write_text(json.dumps(current, indent=2) + "\n", encoding="utf-8")
    if apply:
        subprocess.run(
            [
                "gh",
                "api",
                "--method",
                "PUT",
                f"repos/{REPOSITORY}/rulesets/{RULESET}",
                "--input",
                str(proposal_path),
            ],
            check=True,
        )
    print(f"Rollout prerequisites passed; snapshot: {snapshot}; applied: {apply}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "tested_sha", help="Exact PR commit with all required checks passed"
    )
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    main(args.tested_sha, args.apply)
