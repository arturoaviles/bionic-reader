#!/usr/bin/env python3
"""Exercise installed browser APIs in actual Safari through its WebDriver service."""

from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
import time
from pathlib import Path
from typing import Any
from urllib.error import URLError
from urllib.request import Request, urlopen


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("url")
    parser.add_argument("corpus", type=Path)
    args = parser.parse_args()
    corpus = json.loads(args.corpus.read_text(encoding="utf-8"))
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    driver = subprocess.Popen(["/usr/bin/safaridriver", "-p", str(port)])
    session = None

    def command(method: str, path: str, data: Any = None) -> Any:
        body = None if data is None else json.dumps(data).encode()
        request = Request(
            f"http://127.0.0.1:{port}{path}",
            body,
            {"Content-Type": "application/json"},
            method=method,
        )
        with urlopen(request, timeout=15) as response:
            result = json.load(response)["value"]
        if isinstance(result, dict) and "error" in result:
            raise RuntimeError(result.get("message", result["error"]))
        return result

    try:
        for attempt in range(50):
            try:
                command("GET", "/status")
                break
            except URLError:
                if attempt == 49 or driver.poll() is not None:
                    raise
                time.sleep(0.1)
        created = command(
            "POST",
            "/session",
            {"capabilities": {"alwaysMatch": {"browserName": "safari"}}},
        )
        session = created["sessionId"]
        command("POST", f"/session/{session}/url", {"url": args.url})
        for attempt in range(100):
            ready = command(
                "POST",
                f"/session/{session}/execute/sync",
                {"script": "return Boolean(window.bionicReader)", "args": []},
            )
            if ready:
                break
            if attempt == 99:
                raise RuntimeError("Safari example did not initialize its reader")
            time.sleep(0.1)
        script = """
          const cases = arguments[0], reader = window.bionicReader;
          return {
            render: cases.render.map(item => {
              try { return {output: reader.render(item.text, {...item.options, format:item.format})}; }
              catch (error) { return {error:error.code}; }
            }),
            segments: cases.segments.map(item => reader.segments(item.text, item.options))
          };
        """
        actual = command(
            "POST",
            f"/session/{session}/execute/sync",
            {"script": script, "args": [corpus]},
        )
        for index, case in enumerate(corpus["render"]):
            expected = (
                {"error": case["error"]}
                if "error" in case
                else {"output": case["output"]}
            )
            if actual["render"][index] != expected:
                raise AssertionError(case["name"])
        for index, case in enumerate(corpus["segments"]):
            if actual["segments"][index] != case["output"]:
                raise AssertionError(case["name"])
        evidence = {
            "browser": "Safari",
            "version": created["capabilities"].get("browserVersion"),
            "fixtures": len(corpus["render"]) + len(corpus["segments"]),
        }
        print(json.dumps(evidence))
        if os.environ.get("BIONIC_EXAMPLE_EVIDENCE"):
            Path(os.environ["BIONIC_EXAMPLE_EVIDENCE"]).write_text(
                json.dumps(evidence) + "\n"
            )
    finally:
        try:
            if session:
                command("DELETE", f"/session/{session}")
        finally:
            driver.terminate()
            try:
                driver.wait(timeout=5)
            except subprocess.TimeoutExpired:
                driver.kill()
                driver.wait()


if __name__ == "__main__":
    main()
