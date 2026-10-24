#!/usr/bin/env python3
"""Smoke-check an installed bionic-reader binary through its public CLI."""

from __future__ import annotations

import argparse
import errno
import os
import pty
import select
import shlex
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Mapping, NamedTuple, NoReturn, Optional, Sequence

COMMAND_TIMEOUT = 10.0
SESSION_TIMEOUT = 10.0
PROMPT = b"BIONIC_SMOKE_READY> "
SMOKE_ANSI = b"\x1b[1mread\x1b[22ming"


class CommandResult(NamedTuple):
    returncode: int
    stdout: bytes
    stderr: bytes


def fail(message: str) -> NoReturn:
    raise SystemExit(f"installed CLI smoke check failed: {message}")


def describe(data: bytes) -> str:
    excerpt = data[:500]
    suffix = "..." if len(data) > len(excerpt) else ""
    return f"{excerpt!r}{suffix}"


def isolated_environment(home: Path, config_home: Path, path: str) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(config_home),
            "PATH": path,
            "TERM": "dumb",
            "BASH_SILENCE_DEPRECATION_WARNING": "1",
        }
    )
    for name in (
        "NO_COLOR",
        "BASH_ENV",
        "ENV",
        "BIONIC_READER_CONTROL_SOCKET",
        "BIONIC_READER_DISABLE",
        "BIONIC_READER_MARKER_TOKEN",
        "BIONIC_READER_SESSION",
    ):
        env.pop(name, None)
    return env


def invoke(
    binary: Path,
    arguments: Sequence[str],
    input_bytes: bytes,
    env: Mapping[str, str],
    cwd: Path,
    label: str,
) -> CommandResult:
    try:
        process = subprocess.Popen(
            [str(binary), *arguments],
            cwd=cwd,
            env=dict(env),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
    except OSError as error:
        fail(f"could not start {label}: {error}")

    try:
        stdout, stderr = process.communicate(input_bytes, timeout=COMMAND_TIMEOUT)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        stdout, stderr = process.communicate()
        fail(
            f"{label} exceeded {COMMAND_TIMEOUT:g}s; "
            f"stdout={describe(stdout or b'')}, stderr={describe(stderr or b'')}"
        )
    if process.returncode is None:
        fail(f"{label} did not exit after its output was collected")
    return CommandResult(process.returncode, stdout or b"", stderr or b"")


def require_success(label: str, result: CommandResult) -> None:
    if result.returncode != 0:
        fail(
            f"{label} exited {result.returncode}; "
            f"stdout={describe(result.stdout)}, stderr={describe(result.stderr)}"
        )


def require_bytes(label: str, actual: bytes, expected: bytes) -> None:
    if actual != expected:
        fail(f"{label} produced {describe(actual)}, expected {describe(expected)}")


def require_contains(label: str, actual: bytes, expected: bytes) -> None:
    if expected not in actual:
        fail(f"{label} did not contain {expected!r}; output={describe(actual)}")


def run_cli_checks(binary: Path, root: Path, path: str) -> None:
    home = root / "cli-home"
    config_home = root / "cli-config"
    work = root / "cli-work"
    for directory in (home, config_home, work):
        directory.mkdir()
    env = isolated_environment(home, config_home, path)

    result = invoke(binary, ["--version"], b"", env, work, "--version")
    require_success("--version", result)
    require_contains("--version", result.stdout, b"bionic-reader ")

    result = invoke(binary, ["--help"], b"", env, work, "--help")
    require_success("--help", result)
    require_contains("--help", result.stdout, b"Bionic reading")

    result = invoke(
        binary,
        ["--style", "always", "-t", "reading"],
        b"",
        env,
        work,
        "forced-ANSI text input",
    )
    require_success("forced-ANSI text input", result)
    require_bytes("forced-ANSI text input", result.stdout, SMOKE_ANSI)

    malformed_input = b"reading\xff\x00\r\n"
    transformed_input = SMOKE_ANSI + b"\xff\x00\r\n"
    result = invoke(
        binary,
        ["--style", "always", "--controls", "preserve"],
        malformed_input,
        env,
        work,
        "forced-ANSI stdin input",
    )
    require_success("forced-ANSI stdin input", result)
    require_bytes("forced-ANSI stdin input", result.stdout, transformed_input)

    passthrough = b"\x1b[31mreading\x1b[0m\n\xff\x00\r\n"
    for style in ("auto", "never"):
        label = f"{style} style byte preservation"
        result = invoke(
            binary,
            ["--style", style, "--controls", "preserve"],
            passthrough,
            env,
            work,
            label,
        )
        require_success(label, result)
        require_bytes(label, result.stdout, passthrough)

    for label, arguments, source, expected in [
        ("plain export", ["--format", "plain"], b"\x1b[31mreading\x1b[0m", b"reading"),
        (
            "safe document controls",
            ["--style", "never"],
            b"\x1b]52;c;YXVkaXQ=\x07reading",
            b"reading",
        ),
        (
            "Markdown code preservation",
            ["--input-format", "markdown", "--format", "markdown"],
            b"`reading`",
            b"`reading`",
        ),
        (
            "profile override",
            [
                "--profile",
                "subtle",
                "--fraction",
                "1",
                "--every",
                "1",
                "--style",
                "always",
            ],
            b"reading",
            b"\x1b[1mreading\x1b[22m",
        ),
    ]:
        result = invoke(binary, arguments, source, env, work, label)
        require_success(label, result)
        require_bytes(label, result.stdout, expected)

    config_file = config_home / "bionic-reader" / "config.toml"
    config_file.parent.mkdir()
    config_file.write_text("fraction = 1.0\nstyle = 'always'\n", encoding="utf-8")
    result = invoke(
        binary,
        ["-t", "reading"],
        b"",
        env,
        work,
        "XDG TOML configuration",
    )
    require_success("XDG TOML configuration", result)
    require_bytes("XDG TOML configuration", result.stdout, b"\x1b[1mreading\x1b[22m")

    result = invoke(
        binary,
        ["--fraction", "0.5", "-t", "reading"],
        b"",
        env,
        work,
        "CLI-over-TOML precedence",
    )
    require_success("CLI-over-TOML precedence", result)
    require_bytes("CLI-over-TOML precedence", result.stdout, SMOKE_ANSI)

    explicit_config = work / "explicit.toml"
    explicit_config.write_text("fraction = 0.5\nstyle = 'always'\n", encoding="utf-8")
    result = invoke(
        binary,
        ["--config", str(explicit_config), "-t", "reading"],
        b"",
        env,
        work,
        "explicit TOML configuration",
    )
    require_success("explicit TOML configuration", result)
    require_bytes("explicit TOML configuration", result.stdout, SMOKE_ANSI)

    result = invoke(
        binary,
        ["--config", str(explicit_config), "--fraction", "1.0", "-t", "reading"],
        b"",
        env,
        work,
        "CLI-over-explicit-TOML precedence",
    )
    require_success("CLI-over-explicit-TOML precedence", result)
    require_bytes(
        "CLI-over-explicit-TOML precedence",
        result.stdout,
        b"\x1b[1mreading\x1b[22m",
    )

    shell_markers = {
        "bash": b"PROMPT_COMMAND",
        "zsh": b"precmd_functions",
        "fish": b"__bionic_reader_preexec",
    }
    completion_markers = {
        "bash": b"complete",
        "zsh": b"#compdef",
        "fish": b"complete -c",
    }
    for shell in ("bash", "zsh", "fish"):
        result = invoke(binary, ["init", shell], b"", env, work, f"{shell} initializer")
        require_success(f"{shell} initializer", result)
        require_contains(f"{shell} initializer", result.stdout, shell_markers[shell])

        result = invoke(
            binary,
            ["completions", shell],
            b"",
            env,
            work,
            f"{shell} completions",
        )
        require_success(f"{shell} completions", result)
        require_contains(
            f"{shell} completions", result.stdout, completion_markers[shell]
        )
        require_contains(f"{shell} completions", result.stdout, b"bionic-reader")

    result = invoke(binary, ["on"], b"", env, work, "outside-session control")
    if result.returncode == 0:
        fail("outside-session control unexpectedly succeeded")
    require_contains(
        "outside-session control",
        result.stderr,
        b"requires an active reading session",
    )


class PtySession:
    def __init__(self, pid: int, master: int) -> None:
        self.pid = pid
        self.master = master
        self.output = bytearray()
        self.wait_status: Optional[int] = None

    def send(self, data: bytes) -> None:
        os.write(self.master, data)

    def wait_for_prompt(self, count: int, timeout: float) -> bytes:
        previous_length = len(self.output)
        deadline = time.monotonic() + timeout
        while self.output.count(PROMPT) < count:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                fail(
                    f"Bash PTY did not produce prompt {count} within {timeout:g}s; "
                    f"transcript={describe(bytes(self.output))}"
                )
            ready, _, _ = select.select([self.master], [], [], min(remaining, 0.1))
            if not ready:
                self.poll_exit()
                continue
            try:
                chunk = os.read(self.master, 4096)
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
            if not chunk:
                break
            self.output.extend(chunk)
        if self.output.count(PROMPT) < count:
            fail(
                f"Bash PTY exited before prompt {count}; "
                f"transcript={describe(bytes(self.output))}"
            )
        return bytes(self.output[previous_length:])

    def poll_exit(self) -> Optional[int]:
        if self.wait_status is not None:
            return self.wait_status
        try:
            pid, status = os.waitpid(self.pid, os.WNOHANG)
        except ChildProcessError:
            return self.wait_status
        if pid:
            self.wait_status = status
        return self.wait_status

    def wait_for_exit(self, timeout: float) -> int:
        deadline = time.monotonic() + timeout
        while self.poll_exit() is None:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                fail(
                    f"Bash PTY did not exit within {timeout:g}s; "
                    f"transcript={describe(bytes(self.output))}"
                )
            ready, _, _ = select.select([self.master], [], [], min(remaining, 0.1))
            if ready:
                self.read_available()
        self.read_available()
        assert self.wait_status is not None
        return os.waitstatus_to_exitcode(self.wait_status)

    def read_available(self) -> None:
        while True:
            ready, _, _ = select.select([self.master], [], [], 0)
            if not ready:
                return
            try:
                chunk = os.read(self.master, 4096)
            except OSError as error:
                if error.errno == errno.EIO:
                    return
                raise
            if not chunk:
                return
            self.output.extend(chunk)

    def close(self) -> None:
        if self.poll_exit() is None:
            self.signal_group(signal.SIGTERM)
            deadline = time.monotonic() + 2.0
            while self.poll_exit() is None and time.monotonic() < deadline:
                ready, _, _ = select.select([self.master], [], [], 0.05)
                if ready:
                    self.read_available()
            if self.poll_exit() is None:
                self.signal_group(signal.SIGKILL)
                try:
                    _, status = os.waitpid(self.pid, 0)
                    self.wait_status = status
                except ChildProcessError:
                    pass
        try:
            os.close(self.master)
        except OSError:
            pass

    def signal_group(self, sig: signal.Signals) -> None:
        try:
            os.killpg(self.pid, sig)
        except ProcessLookupError:
            pass


def run_pty_check(binary: Path, root: Path, path: str) -> None:
    home = root / "session-home"
    config_home = root / "session-config"
    work = root / "session-work"
    for directory in (home, config_home, work):
        directory.mkdir()
    # Restore the exact tested command after package launchers such as Snap reset PATH.
    (home / ".bashrc").write_text(
        f"export PATH={shlex.quote(path)}\nPS1='BIONIC_SMOKE_READY> '\n",
        encoding="utf-8",
    )
    env = isolated_environment(home, config_home, path)
    env["SHELL"] = "/bin/bash"
    env["TERM"] = "xterm-256color"

    pid, master = pty.fork()
    if pid == 0:
        try:
            os.chdir(work)
            os.execve(
                str(binary),
                [str(binary), "session", "--shell", "/bin/bash"],
                env,
            )
        except BaseException:
            os._exit(127)

    session = PtySession(pid, master)
    try:
        initial_prompt = session.wait_for_prompt(1, SESSION_TIMEOUT)
        require_contains("immediate Bash prompt", initial_prompt, PROMPT)
        if b"\x1b[1m" in initial_prompt:
            fail("the immediate Bash prompt was wrapped in bold ANSI styling")

        session.send(b"printf 'SESSION reading\\n'\n")
        command_output = session.wait_for_prompt(2, SESSION_TIMEOUT)
        require_contains("Bash session command output", command_output, SMOKE_ANSI)
        require_contains("raw Bash session prompt", command_output, PROMPT)
        require_raw_prompt(session.output, 2)

        session.send(b"bionic-reader off\n")
        session.wait_for_prompt(3, SESSION_TIMEOUT)
        session.send(b"printf 'OFF reading\\n'\n")
        off_output = session.wait_for_prompt(4, SESSION_TIMEOUT)
        require_contains("session off output", off_output, b"OFF reading\r\n")
        if SMOKE_ANSI in off_output:
            fail("bionic-reader off still emphasized Bash command output")

        session.send(b"bionic-reader on\n")
        session.wait_for_prompt(5, SESSION_TIMEOUT)
        session.send(b"printf 'ON reading\\n'\n")
        on_output = session.wait_for_prompt(6, SESSION_TIMEOUT)
        require_contains("session on output", on_output, SMOKE_ANSI)
        require_contains("raw Bash session prompt", on_output, PROMPT)
        require_raw_prompt(session.output, 6)

        session.send(b"exit 7\n")
        status = session.wait_for_exit(SESSION_TIMEOUT)
        if status != 7:
            fail(f"Bash session returned exit status {status}, expected 7")
    finally:
        session.close()


def require_raw_prompt(transcript: bytearray, expected_count: int) -> None:
    starts: list[int] = []
    position = 0
    while True:
        position = transcript.find(PROMPT, position)
        if position < 0:
            break
        starts.append(position)
        position += len(PROMPT)
    if len(starts) < expected_count:
        fail("Bash session prompt count changed while checking raw prompts")
    for start in starts[:expected_count]:
        prefix = transcript[:start]
        bold_start = prefix.rfind(b"\x1b[1m")
        bold_end = prefix.rfind(b"\x1b[22m")
        if bold_start > bold_end:
            fail("Bash prompt appeared while bold styling was active")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "binary",
        type=Path,
        help="explicit path to the installed bionic-reader executable",
    )
    args = parser.parse_args()
    # Snap uses a symlink to its multicall launcher; resolving it changes argv[0].
    binary = args.binary.expanduser().absolute()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        fail(f"installed binary is missing or not executable: {binary}")
    if sys.platform not in ("darwin", "linux"):
        fail(
            f"interactive PTY smoke checks require Linux or macOS, found {sys.platform}"
        )
    if not Path("/bin/bash").is_file():
        fail("interactive Bash smoke check requires /bin/bash")

    with tempfile.TemporaryDirectory(prefix="bionic-reader-installed-smoke-") as name:
        root = Path(name)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        (bin_dir / "bionic-reader").symlink_to(binary)
        # Package-manager launchers may need Node/Corepack from the caller's PATH.
        # Our explicit installed command stays first; HOME/XDG remain isolated.
        path = os.pathsep.join((str(bin_dir), os.environ.get("PATH", os.defpath)))
        run_cli_checks(binary, root, path)
        run_pty_check(binary, root, path)

    print(f"Installed CLI smoke checks passed: {binary}")


if __name__ == "__main__":
    main()
