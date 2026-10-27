"""The bundle inventory must retain every embedded engine's dependency graph."""

import json
import runpy
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
merge = runpy.run_path(str(ROOT / "scripts/merge-library-sboms.py"))["merge"]


class LibrarySbomTests(unittest.TestCase):
    def test_embedded_engines_keep_components_and_distinct_edges(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = []
            for name, dependency in (
                ("cli", "clap"),
                ("python", "pyo3"),
                ("wasm", "wasm-bindgen"),
            ):
                path = Path(directory) / f"{name}.json"
                path.write_text(
                    json.dumps(
                        {
                            "bomFormat": "CycloneDX",
                            "specVersion": "1.5",
                            "metadata": {
                                "component": {"name": name, "bom-ref": "root"}
                            },
                            "components": [{"name": dependency, "bom-ref": "dep"}],
                            "dependencies": [{"ref": "root", "dependsOn": ["dep"]}],
                        }
                    )
                )
                paths.append(path)
            result = merge(paths)
            components = result["components"]
            self.assertEqual(
                {c["name"] for c in components},
                {"cli", "python", "wasm", "clap", "pyo3", "wasm-bindgen"},
            )
            references = {c["bom-ref"] for c in components}
            self.assertEqual(len(references), 6)
            references.add(result["metadata"]["component"]["bom-ref"])
            for edge in result["dependencies"]:
                self.assertIn(edge["ref"], references)
                self.assertTrue(set(edge["dependsOn"]) <= references)


if __name__ == "__main__":
    unittest.main()
