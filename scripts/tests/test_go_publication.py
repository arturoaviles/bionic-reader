"""Regressions for Go engine promotion and public-proxy release identity."""

import hashlib
import json
import runpy
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
go = runpy.run_path(str(ROOT / "scripts/publish-go.py"))


class GoPublicationTests(unittest.TestCase):
    def candidate(self, root):
        wasm = b"\0asm\x01\0\0\0test candidate"
        checksum = hashlib.sha256(wasm).hexdigest()
        identity = f'const Version = "0.3.0"\nconst engineSHA256 = "{checksum}"\n'
        (root / "internal/engine").mkdir(parents=True)
        (root / "internal/engine/engine.wasm").write_bytes(wasm)
        (root / "engine_version.go").write_text(identity)
        source = {"version": "0.3.0", "commit": "a" * 40, "engine_sha256": checksum}
        download = {
            "Path": go["MODULE"],
            "Version": "v0.3.0",
            "Sum": "h1:module",
            "GoModSum": "h1:manifest",
            "Dir": str(root),
            "Origin": {
                "Hash": source["commit"],
                "URL": "https://" + go["MODULE"],
                "Ref": "refs/tags/v0.3.0",
            },
        }
        return wasm, identity, source, download

    def test_engine_requires_matching_version_and_complete_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            wasm, identity, source, _ = self.candidate(Path(directory))
            self.assertEqual(
                go["verify_engine"](wasm, identity, "0.3.0"), source["engine_sha256"]
            )
            for data, declaration, version in (
                (wasm + b"tampered", identity, "0.3.0"),
                (wasm, identity, "0.2.0"),
                (b"not wasm", identity, "0.3.0"),
                (wasm, 'const Version = "0.3.0"', "0.3.0"),
            ):
                with self.assertRaises(ValueError):
                    go["verify_engine"](data, declaration, version)

    def test_proxy_must_prove_tag_commit_and_both_module_sums(self):
        with tempfile.TemporaryDirectory() as directory:
            _, _, source, download = self.candidate(Path(directory))
            go["verify_download"](download, source)
            changes = [
                {"Version": "v0.2.0"},
                {"Path": "example.com/other"},
                {"Sum": ""},
                {"GoModSum": ""},
                {"Origin": {}},
                {"Origin": {**download["Origin"], "Hash": "b" * 40}},
                {"Origin": {**download["Origin"], "Ref": "refs/heads/main"}},
                {"Origin": {**download["Origin"], "URL": "https://example.com/repo"}},
            ]
            for change in changes:
                with self.subTest(change=change), self.assertRaises(ValueError):
                    go["verify_download"]({**download, **change}, source)

    def test_proxy_engine_must_equal_attested_candidate(self):
        with tempfile.TemporaryDirectory() as directory:
            _, _, source, download = self.candidate(Path(directory))
            with self.assertRaisesRegex(ValueError, "attested"):
                go["verify_download"](download, {**source, "engine_sha256": "b" * 64})

    def test_invalid_publication_identity_fails_before_network_access(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "source.json").write_text(
                json.dumps(
                    {
                        "module": "other/module",
                        "version": "0.3.0",
                        "commit": "a" * 40,
                    }
                )
            )
            with self.assertRaisesRegex(ValueError, "Invalid Go"):
                go["publish"](root)
