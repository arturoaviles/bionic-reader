#!/usr/bin/env python3
"""Index an immutable Go release and verify a fresh proxy-backed consumer."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any

MODULE = "github.com/arturoaviles/bionic-reader"


def verify_engine(wasm: bytes, identity: str, version: str) -> str:
    """Reject stale or modified embedded engines before promotion."""
    declared = re.search(r'^const Version = "([^"]+)"$', identity, re.MULTILINE)
    digest = re.search(
        r'^const engineSHA256 = "([0-9a-f]{64})"$', identity, re.MULTILINE
    )
    checksum = hashlib.sha256(wasm).hexdigest()
    if (
        not wasm.startswith(b"\0asm\x01\0\0\0")
        or declared is None
        or declared[1] != version
        or digest is None
        or digest[1] != checksum
    ):
        raise ValueError("Go engine version/checksum does not match the release")
    return checksum


def verify_download(download: dict[str, Any], source: dict[str, Any]) -> None:
    """Require the proxy's checksum-verified module to originate at the release tag."""
    origin = download.get("Origin", {})
    if (
        download.get("Path") != MODULE
        or download.get("Version") != "v" + source["version"]
        or not str(download.get("Sum", "")).startswith("h1:")
        or not str(download.get("GoModSum", "")).startswith("h1:")
        or origin.get("Hash") != source["commit"]
        or origin.get("URL") != "https://" + MODULE
        or origin.get("Ref") != "refs/tags/v" + source["version"]
    ):
        raise ValueError("Go proxy module origin differs from the verified release")
    directory = Path(download["Dir"])
    checksum = verify_engine(
        (directory / "internal/engine/engine.wasm").read_bytes(),
        (directory / "engine_version.go").read_text(),
        source["version"],
    )
    if checksum != source["engine_sha256"]:
        raise ValueError("Go proxy engine differs from the attested CI candidate")


def publish(directory: Path) -> None:
    """Request proxy indexing, then compile and test an independently fetched module."""
    source = json.loads((directory / "source.json").read_text())
    if (
        source.get("module") != MODULE
        or not re.fullmatch(r"[0-9a-f]{40}", source["commit"])
        or not re.fullmatch(
            r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)",
            source["version"],
        )
    ):
        raise ValueError("Invalid Go publication identity")
    environment = {
        **os.environ,
        "GOPROXY": "https://proxy.golang.org",
        "GOSUMDB": "sum.golang.org",
        "GOPRIVATE": "",
        "GONOPROXY": "",
        "GONOSUMDB": "",
        "GOTOOLCHAIN": "local",
        "CGO_ENABLED": "0",
    }
    package = MODULE + "@v" + source["version"]
    with tempfile.TemporaryDirectory(prefix="bionic-reader-go-consumer-") as temporary:
        root = Path(temporary)
        subprocess.run(
            ["go", "mod", "init", "example.com/consumer"],
            cwd=root,
            env=environment,
            check=True,
        )
        # Fetching a tagged module through the public proxy is Go publication.
        download = json.loads(
            subprocess.check_output(
                ["go", "mod", "download", "-json", package],
                cwd=root,
                env=environment,
                text=True,
            )
        )
        verify_download(download, source)
        subprocess.run(["go", "get", package], cwd=root, env=environment, check=True)
        (root / "main.go").write_text("""package main
import (
    "context"
    "fmt"
    bionicreader "github.com/arturoaviles/bionic-reader"
)
func main() {
    ctx := context.Background()
    reader, err := bionicreader.NewReader(ctx)
    if err != nil { panic(err) }
    defer reader.Close()
    result, err := reader.Render(ctx, "Hello world", bionicreader.HTML, bionicreader.Options{})
    if err != nil { panic(err) }
    if result != "<strong>Hel</strong>lo <strong>wor</strong>ld" { panic(result) }
    fmt.Println("Verified Go registry consumer", bionicreader.Version)
}
""")
        subprocess.run(["go", "run", "."], cwd=root, env=environment, check=True)
        subprocess.run(["go", "test", MODULE], cwd=root, env=environment, check=True)
        subprocess.run(["go", "mod", "verify"], cwd=root, env=environment, check=True)
    print(
        f"Published and verified {package}; documentation: https://pkg.go.dev/{package}"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    publish(parser.parse_args().directory)
