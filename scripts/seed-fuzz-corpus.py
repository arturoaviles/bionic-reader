#!/usr/bin/env python3
"""Generate the curated byte-stream regression corpus for all fuzz runs."""

import base64
from pathlib import Path

corpus = Path("fuzz/corpus/byte_stream")
corpus.mkdir(parents=True, exist_ok=True)
seeds = [
    "café cafe\u0301 niño 你好世界 日本語 ภาษาไทย 👨‍👩‍👧‍👦".encode(),
    b"\x1b[2;31mreading\x1b[22m \x1b[4:0mword \x1b[4:3mtext\x1b[0m",
    b"\x1b]52;c;YXVkaXQ=\x07before \x1b]8;;https://example.test\x1b\\link\x1b]8;;\x1b\\",
    b"# Heading\n\n[link](javascript:alert(1)) `code`\n```rust\nlet value = 1;\n```\n",
    b"word\xff\xfe\r\n\x1b[",
    b"a" * 70000 + b" next",
    b"a" * 10920 + "ฟ".encode() + b"a" * 59077 + b" next",
    base64.b64decode(
        "AqDguLbguKnguLLguYTguJE84oCN8J+g5aW95DiW55WMIOaXpeZorOiqniDguKDguLLguKnguLLguYTguJE84oCN8J+Rpg=="
    ),
    b"\0\x1b]" + b"x" * 70000 + b"\x1b\\ next",
    b"\0\x1bP" + b"x" * 70000 + b"\x1b\\ next",
    b"\0\x1b[" + b"0" * 70000 + b"m next",
    b"\0" + "漢".encode() * 8192 + b" next",
    b"\0a" + "\u0301".encode() * 35000 + b" next",
    b"\0\x1b]0;" + "Üₜ".encode() + b"\xc2\x9c next",
    b"\0\x1bP" + "Üₜ".encode() + b"\x9c next",
    b"\0\x1b(\0 next",
    b"\0\x1b" + b"(" * 9000 + b"\0 next",
    b"\0\x9b2;31mreading\x9b22m next\x9b0m",
    b"\0\xc2\x9b2;31mreading\xc2\x9b22m next\xc2\x9b0m",
    b"\0\x9d8;;https://example.test/\xc3\x9c\x9creading\x9d8;;\x9c",
    b"\0/\xd8\x94",
    b"\0a"
    + b"\xcc\x81" * 1327
    + b"\xcc"
    + b"\xcc\x81" * 22210
    + b"\xcc"
    + b"\xcc\x81" * 11462
    + b" next",
    b"\0a" + b"\xcc" * 32768,
    b"\0a" + b"\xcc" * 9875 + b"\x8b" * 52 + b"\xcc" * 22893,
]
for index, seed in enumerate(seeds):
    (corpus / f"seed-{index}").write_bytes(seed)
