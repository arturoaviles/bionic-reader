#!/usr/bin/env python3
"""Verify public behavior through an installed Python distribution."""

import importlib
import importlib.metadata
import json
import os
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

library = importlib.import_module("bionic_reader")
corpus_path = (
    Path(sys.argv[1]) if len(sys.argv) > 1 else Path("tests/fixtures/library.json")
)


class InstalledLibrary(unittest.TestCase):
    def test_native_module_and_distribution_versions_agree(self) -> None:
        self.assertEqual(
            library.__version__, importlib.metadata.version("bionic-reader")
        )

    def test_application_renders_without_a_terminal(self) -> None:
        self.assertEqual(
            library.render("Hello world", format="html"),
            "<strong>Hel</strong>lo <strong>wor</strong>ld",
        )

    def test_shared_corpus(self) -> None:
        corpus = json.loads(corpus_path.read_text(encoding="utf-8"))
        reader = library.Reader()
        for case in corpus["render"]:
            with self.subTest(case=case["name"]):
                if "error" in case:
                    with self.assertRaises(library.BionicError) as failure:
                        reader.render(
                            case["text"], format=case["format"], **case["options"]
                        )
                    self.assertEqual(failure.exception.code, case["error"])
                else:
                    self.assertEqual(
                        reader.render(
                            case["text"], format=case["format"], **case["options"]
                        ),
                        case["output"],
                    )
        for case in corpus["segments"]:
            with self.subTest(case=case["name"]):
                self.assertEqual(
                    reader.segments(case["text"], **case["options"]), case["output"]
                )

    def test_unicode_and_numeric_failures_are_structured(self) -> None:
        for text, options in [("\ud800", {}), ("hello", {"fraction": float("nan")})]:
            with self.subTest(text=repr(text), options=options):
                with self.assertRaises(library.BionicError) as failure:
                    library.render(text, format="html", **options)
                self.assertEqual(failure.exception.code, "invalid_input")

    def test_ambient_terminal_state_is_not_library_configuration(self) -> None:
        with patch.dict(
            os.environ, {"NO_COLOR": "1", "TERM": "dumb", "XDG_CONFIG_HOME": "/absent"}
        ):
            self.assertEqual(
                library.render("Hello", format="html"), "<strong>Hel</strong>lo"
            )

    def test_explicit_configuration(self) -> None:
        options = library.parse_config("profile = 'strong'\nfraction = 0.25\n")
        self.assertEqual(
            library.render("reading", format="html", **options),
            "<strong>re</strong>ading",
        )

    def test_markdown_limit_is_reported(self) -> None:
        with self.assertRaises(library.BionicError) as failure:
            library.render(
                "a" * (8 * 1024 * 1024 + 1), format="html", input_format="markdown"
            )
        self.assertEqual(failure.exception.code, "invalid_input")


if __name__ == "__main__":
    unittest.main(argv=[sys.argv[0]])
