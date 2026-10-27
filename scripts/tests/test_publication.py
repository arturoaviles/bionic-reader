"""Regressions for native registry payloads and publication source identity."""

import base64
import hashlib
import io
import json
import runpy
import tarfile
import tempfile
import unittest
from pathlib import Path

from test_release import bundle, library_fixture

ROOT = Path(__file__).resolve().parents[2]
registry = runpy.run_path(str(ROOT / "scripts/package-registries.py"))
publisher = runpy.run_path(str(ROOT / "scripts/publish-npm.py"))
standalone = runpy.run_path(str(ROOT / "scripts/package-release-node.py"))
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
            self.assertEqual(manifest["exports"]["."]["import"], "./lib/node.js")
            self.assertEqual(manifest["exports"]["./react"]["import"], "./lib/react.js")
            self.assertEqual(manifest["engines"]["node"], ">=22")
            self.assertIn("lib/wasm/engine_bg.wasm", base)
            actual = set()
            for path in (root / "npm").glob("*-native-*.tgz"):
                native = registry["read_package"](path)
                identity = json.loads(native["package.json"][0])
                actual.add(identity["name"])
                if identity["name"].endswith("-musl"):
                    self.assertEqual(identity["libc"], ["musl"])
                elif identity["name"].endswith("-gnu"):
                    self.assertEqual(identity["libc"], ["glibc"])
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

    def test_static_musl_standalone_accepts_glibc_without_relaxing_registry_selection(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            library = root / "library"
            for name, (data, _) in library_fixture().items():
                path = library / name.removeprefix("lib/")
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
            for target, expected in (
                ("x86_64-unknown-linux-musl", ["musl", "glibc"]),
                ("aarch64-unknown-linux-musl", ["musl", "glibc"]),
                ("x86_64-unknown-linux-gnu", ["glibc"]),
            ):
                archive = root / f"bionic-reader-v0.1.0-{target}.tar.gz"
                with tarfile.open(archive, "w:gz") as output:
                    for name, data in (
                        ("bin/bionic-reader", b"native binary"),
                        ("LICENSE", b"MIT"),
                    ):
                        member = tarfile.TarInfo(f"bionic-reader-0.1.0-{target}/{name}")
                        member.size = len(data)
                        output.addfile(member, io.BytesIO(data))
                package = standalone["package"](archive, root / "standalone", library)
                manifest = json.loads(
                    registry["read_package"](package)["package.json"][0]
                )
                self.assertEqual(manifest["libc"], expected)

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

    def test_registry_rejects_different_api_metadata(self):
        for key, value in (
            ("exports", {}),
            ("engines", {"node": ">=26"}),
            ("peerDependencies", {"react": "^19"}),
        ):
            with self.subTest(key=key), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / "source"
                source.mkdir()
                for target in registry["TARGETS"]:
                    built = bundle(root / target, target)
                    name = f"bionic-reader-v0.1.0-{target}.npm.tgz"
                    (source / name).write_bytes((built / name).read_bytes())
                changed = next(source.glob("*.tgz"))
                files = registry["read_package"](changed)
                manifest = json.loads(files["package.json"][0])
                manifest[key] = value
                files["package.json"] = (json.dumps(manifest).encode(), 0o644)
                registry["write_package"](changed, files)
                with self.assertRaisesRegex(ValueError, "metadata"):
                    registry["registry_packages"](source, root / "npm", "0.1.0")

    def test_registry_rejects_incomplete_or_mismatched_library_payloads(self):
        for fault in ("missing", "version", "wasm"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / "source"
                source.mkdir()
                for target in registry["TARGETS"]:
                    built = bundle(root / target, target)
                    name = f"bionic-reader-v0.1.0-{target}.npm.tgz"
                    (source / name).write_bytes((built / name).read_bytes())
                changed = next(source.glob("*.tgz"))
                files = registry["read_package"](changed)
                if fault == "missing":
                    del files["lib/react.js"]
                elif fault == "version":
                    identity = json.loads(files["lib/package.json"][0])
                    identity["version"] = "0.2.0"
                    files["lib/package.json"] = (json.dumps(identity).encode(), 0o644)
                else:
                    files["lib/wasm/engine_bg.wasm"] = (b"different engine", 0o644)
                registry["write_package"](changed, files)
                with self.assertRaises(ValueError):
                    registry["registry_packages"](source, root / "npm", "0.1.0")


if __name__ == "__main__":
    unittest.main()
