"""Regressions for trusted Cargo candidate identity, integrity and retry behavior."""

import hashlib
import io
import json
import runpy
import tarfile
import tempfile
import unittest
import urllib.error
import zipfile
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
publisher = runpy.run_path(str(ROOT / "scripts/publish-crates-io.py"))
COMMIT = "a" * 40
REPO = "arturoaviles/bionic-reader"
SOURCE = {"commit": COMMIT, "ci_run": f"https://github.com/{REPO}/actions/runs/123"}


def crate(version="0.2.0", commit=COMMIT, dirty=False):
    """Create a minimal candidate containing real Cargo identity records."""
    contents = io.BytesIO()
    with tarfile.open(fileobj=contents, mode="w:gz") as archive:
        for name, data in (
            (
                "Cargo.toml",
                f'[package]\nname = "bionic-reader"\nversion = "{version}"\n',
            ),
            (
                ".cargo_vcs_info.json",
                json.dumps(
                    {
                        "git": {"sha1": commit, "dirty": dirty},
                        "path_in_vcs": "",
                    }
                ),
            ),
        ):
            member = tarfile.TarInfo(f"bionic-reader-{version}/{name}")
            encoded = data.encode()
            member.size = len(encoded)
            archive.addfile(member, io.BytesIO(encoded))
    return contents.getvalue()


class CargoPublicationTests(unittest.TestCase):
    def test_candidate_must_match_clean_release_identity(self):
        validate = publisher["validate_crate"]
        candidate = crate()
        self.assertEqual(
            validate(candidate, "0.2.0", COMMIT), hashlib.sha256(candidate).hexdigest()
        )
        for data in (crate(commit="b" * 40), crate(dirty=True)):
            with self.assertRaisesRegex(ValueError, "clean release"):
                validate(data, "0.2.0", COMMIT)
        with self.assertRaises(KeyError):
            validate(crate(version="0.1.0"), "0.2.0", COMMIT)

    def stage(
        self, root, *, changes=None, digest=None, extra_file=False, expired=False
    ):
        """Mock only GitHub transport; exercise run, digest and archive validation."""
        candidate = crate()
        contents = io.BytesIO()
        with zipfile.ZipFile(contents, "w") as archive:
            archive.writestr("bionic-reader-0.2.0.crate", candidate)
            if extra_file:
                archive.writestr("unexpected", b"extra")
        data = contents.getvalue()
        run = {
            "head_sha": COMMIT,
            "head_branch": "main",
            "event": "push",
            "path": ".github/workflows/ci.yml",
            "status": "completed",
            "conclusion": "success",
            "head_repository": {"full_name": REPO},
            **(changes or {}),
        }
        artifact = {
            "id": 456,
            "name": "bionic-reader-cargo-crate",
            "expired": expired,
            "digest": digest or "sha256:" + hashlib.sha256(data).hexdigest(),
        }

        def download(arguments, *, stdout, check):
            self.assertTrue(check)
            self.assertEqual(arguments[-1], f"repos/{REPO}/actions/artifacts/456/zip")
            stdout.write(data)

        stage = publisher["stage_cargo"]
        with patch.dict(
            stage.__globals__,
            {
                "command": unittest.mock.Mock(
                    side_effect=[json.dumps(run), json.dumps({"artifacts": [artifact]})]
                ),
            },
        ), patch("subprocess.run", side_effect=download):
            stage(SOURCE, "0.2.0", REPO, root / "publication")

    def test_stages_exact_original_ci_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.stage(root)
            record = publisher["load_candidate"](root / "publication")
            self.assertEqual(record["commit"], COMMIT)
            path = root / "publication/bionic-reader-0.2.0.crate"
            self.assertEqual(
                record["sha256"], hashlib.sha256(path.read_bytes()).hexdigest()
            )
            self.assertTrue((root / "publication/publish-crates-io.py").is_file())

    def test_rejects_other_sources_failed_runs_and_modified_archives(self):
        faults = [
            {"changes": {"head_sha": "b" * 40}},
            {"changes": {"event": "pull_request"}},
            {"changes": {"conclusion": "failure"}},
            {"changes": {"head_repository": {"full_name": "other/repo"}}},
            {"digest": "sha256:" + "0" * 64},
            {"extra_file": True},
            {"expired": True},
        ]
        for fault in faults:
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                with self.assertRaises(ValueError):
                    self.stage(Path(directory), **fault)

    def test_staged_candidate_tampering_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.stage(root)
            (root / "publication/bionic-reader-0.2.0.crate").write_bytes(
                crate(dirty=True)
            )
            with self.assertRaises(ValueError):
                publisher["load_candidate"](root / "publication")

    def test_existing_version_is_skipped_only_for_identical_checksum(self):
        source = {"version": "0.2.0", "sha256": "a" * 64}
        for checksum in (source["sha256"], "b" * 64):
            response = io.BytesIO(
                json.dumps({"version": {"checksum": checksum}}).encode()
            )
            with patch("urllib.request.urlopen", return_value=response):
                if checksum == source["sha256"]:
                    self.assertTrue(publisher["already_published"](source))
                else:
                    with self.assertRaisesRegex(ValueError, "different bytes"):
                        publisher["already_published"](source)

    def test_only_registry_404_means_unpublished(self):
        for status in (404, 403, 429, 500):
            with self.subTest(status=status), patch(
                "urllib.request.urlopen",
                side_effect=urllib.error.HTTPError(
                    "https://crates.io", status, "failure", {}, None
                ),
            ):
                if status == 404:
                    self.assertFalse(
                        publisher["already_published"]({"version": "0.2.0"})
                    )
                else:
                    with self.assertRaises(urllib.error.HTTPError):
                        publisher["already_published"]({"version": "0.2.0"})

    def test_repackaged_candidate_must_match_tested_bytes(self):
        verify = publisher["verify_source"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "package/bionic-reader-0.2.0.crate"
            archive.parent.mkdir()
            archive.write_bytes(b"different candidate")
            with patch.dict(
                verify.__globals__,
                {
                    "command": unittest.mock.Mock(
                        side_effect=[
                            COMMIT,
                            json.dumps({"target_directory": str(root)}),
                        ]
                    ),
                },
            ), patch("subprocess.run"):
                with self.assertRaisesRegex(ValueError, "Repackaged release differs"):
                    verify(
                        {"commit": COMMIT, "version": "0.2.0", "sha256": "a" * 64}, root
                    )


if __name__ == "__main__":
    unittest.main()
