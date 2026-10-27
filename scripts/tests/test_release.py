"""Regressions for release source validation and complete candidate promotion."""

import hashlib
import json
import os
import runpy
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
release = runpy.run_path(str(ROOT / "scripts/prepare-release.py"))
checksums = release["checksums"]
registry = runpy.run_path(str(ROOT / "scripts/package-registries.py"))
TARGET = "aarch64-apple-darwin"


def library_fixture():
    wasm = b"\0asm\x01\0\0\0"
    files = {
        name: (b"shared library fixture", 0o644)
        for name in (
            "lib/index.d.ts",
            "lib/runtime.js",
            "lib/node.js",
            "lib/node.cjs",
            "lib/browser.js",
            "lib/react.js",
            "lib/react.cjs",
            "lib/react.d.ts",
            "lib/wasm/engine.js",
            "lib/wasm/engine.d.ts",
            "lib/wasm/engine_bg.wasm.d.ts",
        )
    }
    files["lib/wasm/engine_bg.wasm"] = (wasm, 0o644)
    files["lib/version.js"] = (b'export const VERSION = "0.1.0";\n', 0o644)
    files["lib/package.json"] = (
        json.dumps(
            {"version": "0.1.0", "wasm_sha256": hashlib.sha256(wasm).hexdigest()}
        ).encode(),
        0o644,
    )
    return files


def bundle(root, target=TARGET):
    root.mkdir()
    for name in (
        f"bionic-reader-v0.1.0-{target}.tar.gz",
        f"bionic_reader-0.1.0-{target}.whl",
        "sbom.cdx.json",
    ):
        (root / name).write_bytes(b"verified bytes")
    system, cpu, libc = registry["TARGETS"][target]
    report = {
        "package_version": "0.1.0",
        "artifact": {"sha256": hashlib.sha256(b"\0asm\x01\0\0\0").hexdigest()},
        "cold": [{"initialize_ms": 1}],
        "warm": [{"samples_ms": [1]}],
        "environment": {"node": "fixture"},
    }
    (root / "library-performance.json").write_text(json.dumps(report))
    manifest = json.loads(
        (ROOT / "packaging/npm/package.json.in")
        .read_text()
        .replace("@VERSION@", "0.1.0")
    )
    manifest.update({"os": [system], "cpu": [cpu]})
    registry["write_package"](
        root / f"bionic-reader-v0.1.0-{target}.npm.tgz",
        {
            "package.json": (
                json.dumps(manifest).encode(),
                0o644,
            ),
            "bin/native/bionic-reader": (b"verified native executable", 0o755),
            "bin/bionic-reader.js": (b"shared verified launcher", 0o755),
            "LICENSE": (b"MIT", 0o644),
            **library_fixture(),
        },
    )
    checksums(root, False)
    return root


class ReleaseTests(unittest.TestCase):
    def test_tag_must_be_normal_semver_and_match_cargo(self):
        manifest = '[package]\nversion = "0.1.0"\n'
        self.assertEqual(release["release_version"]("v0.1.0", manifest), "0.1.0")
        for tag in ("v0.2.0", "v00.1.0", "v0.1.0-rc.1", "v0.1.0+test", "0.1.0"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release["release_version"](tag, manifest)

    def test_only_successful_exact_main_ci_can_be_promoted(self):
        run = {
            "head_sha": "abc",
            "head_branch": "main",
            "event": "push",
            "path": ".github/workflows/ci.yml",
            "status": "completed",
            "conclusion": "success",
            "head_repository": {"full_name": "owner/repo"},
        }
        check = release["check_run"]
        check(run, "abc", "owner/repo")
        for key, value in (
            ("head_sha", "other"),
            ("head_branch", "feature"),
            ("event", "pull_request"),
            ("event", "workflow_dispatch"),
            ("path", ".github/workflows/other.yml"),
            ("status", "in_progress"),
            ("conclusion", "failure"),
            ("conclusion", "skipped"),
            ("head_repository", {"full_name": "fork/repo"}),
        ):
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                check({**run, key: value}, "abc", "owner/repo")

    def test_verified_bytes_preserved_and_sboms_do_not_collide(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "output"
            output.mkdir()
            for target in (TARGET, "x86_64-apple-darwin"):
                source = bundle(root / target, target)
                release["stage_bundle"](source, output, "0.1.0", target)
                sbom = output / f"bionic-reader-v0.1.0-{target}.sbom.cdx.json"
                self.assertEqual(sbom.read_bytes(), b"verified bytes")
            checksums(output, False)
            checksums(output, True)
            with self.assertRaises(ValueError):
                release["stage_bundle"](source, output, "0.1.0", target)

    def test_entire_release_stages_all_targets_and_aborts_on_bad_provenance(self):
        for fail_provenance in (False, True):
            with self.subTest(
                fail_provenance=fail_provenance
            ), tempfile.TemporaryDirectory() as d:
                root = Path(d)
                fixtures = root / "fixtures"
                fixtures.mkdir()
                for target in release["TARGETS"]:
                    source = bundle(fixtures / target, target)
                    if target.endswith("-linux-gnu"):
                        arch = target.split("-", 1)[0]
                        for suffix, label in (
                            ("deb", "amd64" if arch == "x86_64" else "arm64"),
                            ("rpm", arch),
                            ("pkg.tar.zst", arch),
                        ):
                            (
                                source / f"bionic-reader-0.1.0-1.{label}.{suffix}"
                            ).write_bytes(b"package")
                        checksums(source, False)
                output = root / "release"
                github_output = root / "github-output"
                run = {
                    "head_sha": "abc",
                    "head_branch": "main",
                    "event": "push",
                    "path": ".github/workflows/ci.yml",
                    "status": "completed",
                    "conclusion": "success",
                    "head_repository": {"full_name": "owner/repo"},
                    "html_url": "https://github.com/owner/repo/actions/runs/123",
                }

                def command(*args):
                    if args[:2] == ("git", "rev-parse"):
                        return "abc"
                    if args[:2] == ("git", "show"):
                        return '[package]\nversion = "0.1.0"\n'
                    if args == ("gh", "api", "repos/owner/repo/actions/runs/123"):
                        return json.dumps(run)
                    if args[:2] == ("gh", "api") and "/workflows/" in args[2]:
                        return json.dumps({"workflow_runs": [{**run, "id": 123}]})
                    self.fail(f"Unexpected command: {args}")

                real_run = subprocess.run

                def invoke(args, **kwargs):
                    if len(args) > 1 and args[1].endswith("render-homebrew-formula.py"):
                        return real_run(args, **kwargs)
                    if args[:3] == ["gh", "run", "download"]:
                        target = args[args.index("--name") + 1].removeprefix(
                            "bionic-reader-"
                        )
                        shutil.copytree(
                            fixtures / target, Path(args[args.index("--dir") + 1])
                        )
                    elif args[:3] == ["gh", "attestation", "verify"]:
                        if fail_provenance:
                            raise subprocess.CalledProcessError(1, args)
                    elif args[:3] != ["git", "merge-base", "--is-ancestor"]:
                        self.fail(f"Unexpected subprocess: {args}")
                    return subprocess.CompletedProcess(args, 0)

                globals_ = release["main"].__globals__
                with mock.patch.dict(globals_, {"command": command}), mock.patch.dict(
                    os.environ,
                    {
                        "GITHUB_REPOSITORY": "owner/repo",
                        "GITHUB_OUTPUT": str(github_output),
                    },
                ), mock.patch(
                    "sys.argv",
                    ["prepare-release.py", "v0.1.0", "--output", str(output)],
                ), mock.patch("subprocess.run", side_effect=invoke) as calls:
                    if fail_provenance:
                        with self.assertRaises(subprocess.CalledProcessError):
                            release["main"]()
                        self.assertFalse(github_output.exists())
                        self.assertFalse((output / "SHA256SUMS").exists())
                    else:
                        release["main"]()
                        checksums(output, True)
                        self.assertEqual(len(list(output.iterdir())), 47)
                        self.assertEqual(
                            github_output.read_text(), "tag=v0.1.0\nsha=abc\n"
                        )
                        provenance_calls = [
                            call
                            for call in calls.call_args_list
                            if call.args[0][:3] == ["gh", "attestation", "verify"]
                        ]
                        self.assertEqual(len(provenance_calls), 36)

    def test_tampered_incomplete_or_unexpected_bundles_fail(self):
        for failure in ("tampered", "missing", "extra", "gnu-packages", "report"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as d:
                root = Path(d)
                target = (
                    "x86_64-unknown-linux-gnu" if failure == "gnu-packages" else TARGET
                )
                source = bundle(root / "bundle", target)
                output = root / "output"
                output.mkdir()
                sbom = source / "sbom.cdx.json"
                if failure == "tampered":
                    sbom.write_bytes(b"tampered")
                elif failure == "missing":
                    sbom.unlink()
                    checksums(source, False)
                elif failure == "extra":
                    (source / "extra.txt").write_bytes(b"unexpected")
                    checksums(source, False)
                elif failure == "report":
                    (source / "library-performance.json").write_text(
                        '{"package_version":"0.2.0"}'
                    )
                    checksums(source, False)
                with self.assertRaises(ValueError):
                    release["stage_bundle"](source, output, "0.1.0", target)


if __name__ == "__main__":
    unittest.main()
