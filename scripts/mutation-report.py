#!/usr/bin/env python3
"""Run a bounded mutation campaign; findings are advisory, tool errors are fatal."""

import argparse
import json
import os
import signal
import subprocess
from pathlib import Path


def bounded_wait(process: subprocess.Popen[bytes], budget: int) -> tuple[int, bool]:
    """Terminate the whole process group when a baseline or campaign expires."""
    try:
        return process.wait(timeout=budget), False
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
        return process.returncode, True


def report(module: str, budget: int, output: Path) -> None:
    """Keep baseline failures fatal even when the overall campaign times out."""
    output.mkdir(parents=True, exist_ok=True)
    # Baseline is explicitly completed before the advisory budget can expire.
    if budget <= 0:
        raise ValueError("The mutation budget must be positive")
    with (output / "baseline.log").open("w", encoding="utf-8") as log:
        baseline_process = subprocess.Popen(
            ["cargo", "test", "--locked", "--lib"],
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        baseline_status, baseline_timeout = bounded_wait(baseline_process, 600)
    if baseline_timeout or baseline_status != 0:
        raise RuntimeError(
            f"Baseline failed (exit {baseline_status}, timeout {baseline_timeout})"
        )
    command = [
        "cargo",
        "mutants",
        "--file",
        f"src/{module}.rs",
        "--jobs",
        "2",
        "--output",
        str(output),
        "--timeout",
        "60",
        "--build-timeout",
        "180",
        "--",
        "--locked",
        "--lib",
    ]
    with (output / "campaign.log").open("w", encoding="utf-8") as log:
        process = subprocess.Popen(
            command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True
        )
        status, exhausted = bounded_wait(process, budget)
    # Do not turn a timeout during cargo-mutants' own baseline into a success.
    outcomes_path = output / "mutants.out" / "outcomes.json"
    outcomes = json.loads(outcomes_path.read_text(encoding="utf-8"))
    baseline = [o for o in outcomes["outcomes"] if o["scenario"] == "Baseline"]
    if len(baseline) != 1 or baseline[0]["summary"] != "Success":
        raise RuntimeError("Mutation baseline did not complete successfully")
    if not exhausted and status not in (0, 2, 3):
        raise RuntimeError(f"cargo-mutants failed with status {status}")
    summary = (
        f"{module}: exit {status}; {budget}-second budget exhausted: {exhausted}.\n"
        f"Tested: {outcomes['total_mutants']}; caught: {outcomes['caught']}; "
        f"survived: {outcomes['missed']}; timed out: {outcomes['timeout']}; "
        f"unviable: {outcomes['unviable']}.\n"
        "Surviving mutants and mutant timeouts are advisory. "
        "See mutants.out/missed.txt, timeout.txt, outcomes.json and campaign.log.\n"
    )
    (output / "summary.md").write_text(summary, encoding="utf-8")
    print(summary)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("module", choices=("settings", "segmentation", "transformer"))
    parser.add_argument("--budget", type=int, default=2700)
    parser.add_argument("--output", type=Path, default=Path("target/mutations"))
    args = parser.parse_args()
    report(args.module, args.budget, args.output / args.module)
