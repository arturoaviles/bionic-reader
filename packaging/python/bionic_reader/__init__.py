"""Local document transformation backed by the shared Rust engine."""

import json

from . import _native

__version__ = _native.__version__


class BionicError(ValueError):
    """A document/options failure with a stable machine-readable code."""

    def __init__(self, message, code="invalid_input"):
        super().__init__(message)
        self.code = code


def _unicode(value):
    if not isinstance(value, str):
        raise BionicError("text must be a Unicode string")
    try:
        value.encode("utf-8", errors="strict")
    except UnicodeError as error:
        raise BionicError("text contains malformed Unicode") from error
    return value


def _options(values):
    try:
        return json.dumps(values, allow_nan=False)
    except (TypeError, ValueError) as error:
        raise BionicError(str(error)) from error


class Reader:
    """A reusable reader; frequency starts fresh for each document."""

    def __init__(self):
        self._reader = _native.NativeReader()

    def render(self, text, *, format, **options):
        """Render a whole string in an explicitly selected output format."""
        try:
            return self._reader.render(_unicode(text), _unicode(format), _options(options))
        except ValueError as error:
            if isinstance(error, BionicError):
                raise
            raise BionicError(str(error)) from error

    def segments(self, text, **options):
        """Return ordered dictionaries containing text and an emphasized flag."""
        try:
            return json.loads(self._reader.segments(_unicode(text), _options(options)))
        except ValueError as error:
            if isinstance(error, BionicError):
                raise
            raise BionicError(str(error)) from error


def render(text, *, format, **options):
    """Render a whole document without CLI configuration or terminal detection."""
    return Reader().render(text, format=format, **options)


def segments(text, **options):
    """Return visible text segments using deterministic document settings."""
    return Reader().segments(text, **options)


def parse_config(contents):
    """Parse explicitly supplied CLI TOML text; no files are discovered or read."""
    try:
        return json.loads(_native.parse_config(_unicode(contents)))
    except ValueError as error:
        if isinstance(error, BionicError):
            raise
        raise BionicError(str(error)) from error


__all__ = ["BionicError", "Reader", "render", "segments", "parse_config"]
