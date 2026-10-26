#!/usr/bin/env python3
"""Reject missing, skipped, cancelled, or failed CI dependencies."""

import json
import os
import sys


def check_needs(needs: dict[str, dict[str, str]], expected: set[str]) -> None:
    """Require every expected job and only successful results."""
    if not expected or set(needs) != expected:
        raise ValueError(
            f"CI dependency mismatch: expected {expected}, got {set(needs)}"
        )
    failed = {
        name: value.get("result")
        for name, value in needs.items()
        if value.get("result") != "success"
    }
    if failed:
        raise ValueError(f"CI jobs did not succeed: {failed}")


if __name__ == "__main__":
    check_needs(json.loads(os.environ["CI_NEEDS"]), set(sys.argv[1:]))
    print("Every required CI job succeeded.")
