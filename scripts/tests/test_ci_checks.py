"""Failure-path regressions for the CI gate and candidate integrity boundary."""

import runpy
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
gate = runpy.run_path(str(ROOT / "scripts/check-ci-gate.py"))["check_needs"]
checksums = runpy.run_path(str(ROOT / "scripts/candidate-checksums.py"))["checksums"]
policy = runpy.run_path(str(ROOT / "scripts/check-ci-policy.py"))


class GateTests(unittest.TestCase):
    def test_failure_cancellation_and_skips_fail_closed(self):
        for result in ("failure", "cancelled", "skipped", None):
            with self.subTest(result=result), self.assertRaises(ValueError):
                gate({"matrix": {"result": result}}, {"matrix"})

    def test_missing_or_empty_jobs_fail_closed(self):
        for needs, expected in (({}, {"matrix"}), ({}, set())):
            with self.assertRaises(ValueError):
                gate(needs, expected)

    def test_success(self):
        gate({"matrix": {"result": "success"}}, {"matrix"})

    def test_omitted_matrix_dependency_fails(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        policy["check_gate_config"](workflow)
        broken = workflow.replace("portable-core, rust, ", "portable-core, ", 1)
        with self.assertRaises(ValueError):
            policy["check_gate_config"](broken)

    def test_toolchain_drift_fails(self):
        with self.assertRaises(ValueError):
            policy["check_toolchain"]('rust-version = "1.85.1"', 'channel = "stable"')

    def test_all_valid_job_id_styles_must_be_aggregated(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        for job_id in ("integration_v2", "Build2", "_quality"):
            with self.subTest(job_id=job_id), self.assertRaises(ValueError):
                added = workflow.replace(
                    "\n  ci-gate:\n",
                    f"\n  {job_id}:\n    runs-on: ubuntu-24.04\n"
                    "    steps:\n      - run: echo done\n\n  ci-gate:\n",
                )
                policy["check_gate_config"](added)


class ChecksumTests(unittest.TestCase):
    def test_tampering_missing_and_extra_files_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "candidate.tar.gz"
            artifact.write_bytes(b"original")
            checksums(root, False)
            checksums(root, True)
            artifact.write_bytes(b"modified")
            with self.assertRaises(ValueError):
                checksums(root, True)
            artifact.unlink()
            with self.assertRaises(ValueError):
                checksums(root, True)
            artifact.write_bytes(b"original")
            (root / "extra.whl").write_bytes(b"unlisted")
            with self.assertRaises(ValueError):
                checksums(root, True)

    def test_symlinks_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "candidate.tar.gz").symlink_to("/etc/hosts")
            with self.assertRaises(ValueError):
                checksums(root, False)


if __name__ == "__main__":
    unittest.main()
