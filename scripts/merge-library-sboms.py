#!/usr/bin/env python3
"""Combine the three embedded Rust dependency graphs into a candidate-bundle SBOM."""

from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
from typing import Any


def scoped(value: Any, prefix: str) -> Any:
    """Keep each graph's references distinct, even when dependencies overlap."""
    if isinstance(value, list):
        return [scoped(item, prefix) for item in value]
    if not isinstance(value, dict):
        return value
    result = {}
    for key, item in value.items():
        if key in {"bom-ref", "ref"}:
            result[key] = prefix + item
        elif key == "dependsOn":
            result[key] = [prefix + reference for reference in item]
        else:
            result[key] = scoped(item, prefix)
    return result


def merge(paths: list[Path]) -> dict[str, Any]:
    """Preserve CLI, Python-extension, and wasm32 runtime components and edges."""
    documents: list[dict[str, Any]] = [json.loads(path.read_text()) for path in paths]
    if len(documents) != 3 or any(
        doc.get("bomFormat") != "CycloneDX" or doc.get("specVersion") != "1.5"
        for doc in documents
    ):
        raise ValueError("Expected three CycloneDX 1.5 dependency graphs")
    result = copy.deepcopy(documents[0])
    root = result["metadata"]["component"]
    root["name"] += " distribution bundle"
    root["bom-ref"] = "bionic-reader-distribution-bundle"
    result["components"] = []
    result["dependencies"] = []
    roots = []
    for scope, document in zip(("cli", "python", "wasm32"), documents):
        graph = scoped(document, scope + ":")
        component = graph["metadata"]["component"]
        roots.append(component["bom-ref"])
        result["components"].extend([component, *graph.get("components", [])])
        result["dependencies"].extend(graph.get("dependencies", []))
    result["dependencies"].append({"ref": root["bom-ref"], "dependsOn": roots})
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("graphs", nargs=3, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.write_text(json.dumps(merge(args.graphs), indent=2) + "\n")


if __name__ == "__main__":
    main()
