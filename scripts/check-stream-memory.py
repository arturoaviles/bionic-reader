#!/usr/bin/env python3
"""Check that transformed streaming input does not accumulate document-sized buffers."""

import argparse
import os
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path


def resident_bytes(pid: int) -> int:
    for line in Path(f"/proc/{pid}/status").read_text(encoding="ascii").splitlines():
        if line.startswith("VmRSS:"):
            return int(line.split()[1]) * 1024
    raise RuntimeError("child has no resident-memory measurement")


def wait_for_peak(process: subprocess.Popen[bytes]) -> tuple[int, int]:
    deadline = time.monotonic() + 180
    while True:
        pid, status, usage = os.wait4(process.pid, os.WNOHANG)
        if pid:
            code = os.waitstatus_to_exitcode(status)
            process.returncode = code
            return code, usage.ru_maxrss * 1024
        if time.monotonic() >= deadline:
            raise subprocess.TimeoutExpired(process.args, 180)
        time.sleep(0.02)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    if sys.platform != "linux":
        raise SystemExit("the streaming-memory gate requires Linux /proc and ru_maxrss")
    binary = args.binary.absolute()
    ready = threading.Event()
    resume = threading.Event()
    failures: list[BaseException] = []
    block = (
        "Reading café documentation preserves words and whitespace.\n" * 128
    ).encode()
    warm_blocks = (8 * 1024 * 1024) // len(block)
    measured_blocks = (32 * 1024 * 1024) // len(block)
    with tempfile.TemporaryDirectory(prefix="bionic-memory-") as temporary:
        environment = dict(os.environ, HOME=temporary, XDG_CONFIG_HOME=temporary)
        with tempfile.TemporaryFile() as diagnostics:
            process = subprocess.Popen(
                [str(binary), "--style", "always"],
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=diagnostics,
                env=environment,
            )
            assert process.stdin is not None
            input_stream = process.stdin

            def write_input() -> None:
                try:
                    for _ in range(warm_blocks):
                        input_stream.write(block)
                    input_stream.flush()
                    ready.set()
                    if not resume.wait(30):
                        raise RuntimeError(
                            "memory sampler did not resume the input writer"
                        )
                    for _ in range(measured_blocks):
                        input_stream.write(block)
                except (OSError, RuntimeError) as error:
                    failures.append(error)
                finally:
                    try:
                        input_stream.close()
                    except OSError as error:
                        failures.append(error)
                    ready.set()

            writer = threading.Thread(target=write_input, daemon=True)
            writer.start()
            try:
                if not ready.wait(120):
                    raise RuntimeError("stream did not complete its memory warmup")
                if failures:
                    raise RuntimeError("input writer failed") from failures[0]
                baseline = resident_bytes(process.pid)
                resume.set()
                # Measure this executable only. RUSAGE_CHILDREN can retain an earlier
                # compiler's peak when the invoking shell execs this Python process.
                status, peak = wait_for_peak(process)
                writer.join(timeout=10)
                if writer.is_alive() or failures or status != 0:
                    diagnostics.seek(0)
                    raise RuntimeError(
                        f"streaming invocation failed ({status}): "
                        + diagnostics.read(4096).decode("utf-8", errors="replace")
                    )
                growth = max(0, peak - baseline)
                if peak > 192 * 1024 * 1024 or growth > 24 * 1024 * 1024:
                    raise RuntimeError(
                        f"stream retained document-sized memory: baseline={baseline}, "
                        f"peak={peak}, growth={growth} bytes"
                    )
                print(
                    f"Streaming memory passed: 40 MiB input, warm RSS {baseline}, "
                    f"peak RSS {peak}, growth {growth} bytes"
                )
            finally:
                resume.set()
                if process.poll() is None:
                    process.kill()
                    process.wait()
                writer.join(timeout=10)


if __name__ == "__main__":
    main()
