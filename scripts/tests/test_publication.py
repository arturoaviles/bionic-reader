"""Regressions for native registry payloads and publication source identity."""

import base64
import hashlib
import json
import runpy
import tempfile
import unittest
from pathlib import Path

from test_release import bundle

ROOT = Path(__file__).resolve().parents[2]
registry = runpy.run_path(str(ROOT / "scripts/package-registries.py"))
publisher = runpy.run_path(str(ROOT / "scripts/publish-npm.py"))
publication = runpy.run_path(str(ROOT / "scripts/stage-publication.py"))


class PublicationTests(unittest.TestCase):
    def test_only_published_matching_source_can_publish(self):
        release = {"isDraft": False, "isPrerelease": False, "tagName": "v0.1.0"}
        source = {"tag": "v0.1.0", "commit": "abc"}
        validate = publication["validate_source"]
        self.assertEqual(validate("v0.1.0", "abc", release, source), "0.1.0")
        for changed in (
            {**release, "isDraft": True},
            {**release, "isPrerelease": True},
            {**release, "tagName": "v0.2.0"},
        ):
            with self.assertRaises(ValueError):
                validate("v0.1.0", "abc", changed, source)
        with self.assertRaises(ValueError):
            validate("v0.1.0", "moved", release, source)
        with self.assertRaises(ValueError):
            validate("v00.1.0", "abc", release, source)

    def test_registry_launcher_matches_all_six_native_payloads(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            for target in registry["TARGETS"]:
                built = bundle(root / target, target)
                (source / f"bionic-reader-v0.1.0-{target}.npm.tgz").write_bytes(
                    (built / f"bionic-reader-v0.1.0-{target}.npm.tgz").read_bytes()
                )
            registry["registry_packages"](source, root / "npm", "0.1.0")
            files = list((root / "npm").glob("*.tgz"))
            self.assertEqual(len(files), 7)
            base = registry["read_package"](
                root / "npm/bionic-reader-npm-base-v0.1.0.tgz"
            )
            manifest = json.loads(base["package.json"][0])
            self.assertEqual(len(manifest["optionalDependencies"]), 6)
            self.assertNotIn("bin/native/bionic-reader", base)
            self.assertFalse(manifest.get("scripts"))
            actual = set()
            for path in (root / "npm").glob("*-native-*.tgz"):
                native = registry["read_package"](path)
                identity = json.loads(native["package.json"][0])
                actual.add(identity["name"])
                self.assertEqual(identity["version"], "0.1.0")
                self.assertEqual(
                    native["bin/bionic-reader"][0], b"verified native executable"
                )
                self.assertTrue(identity["preferUnplugged"])
                self.assertTrue(
                    identity["repository"]["url"].endswith(
                        "arturoaviles/bionic-reader.git"
                    )
                )
            self.assertEqual(actual, set(manifest["optionalDependencies"]))

    def test_existing_registry_version_requires_identical_tarball(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "package.tgz"
            path.write_bytes(b"original package bytes")
            digest = (
                "sha512-"
                + base64.b64encode(hashlib.sha512(path.read_bytes()).digest()).decode()
            )
            publisher["check_existing"](path, digest)
            path.write_bytes(b"modified package bytes")
            with self.assertRaises(ValueError):
                publisher["check_existing"](path, digest)

    def test_registry_rejects_changed_shared_launcher(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            for target in registry["TARGETS"]:
                built = bundle(root / target, target)
                path = source / f"bionic-reader-v0.1.0-{target}.npm.tgz"
                path.write_bytes((built / path.name).read_bytes())
            modified = next(source.glob("*.tgz"))
            files = registry["read_package"](modified)
            files["bin/bionic-reader.js"] = (b"different launcher", 0o755)
            registry["write_package"](modified, files)
            with self.assertRaises(ValueError):
                registry["registry_packages"](source, root / "npm", "0.1.0")


if __name__ == "__main__":
    unittest.main()
