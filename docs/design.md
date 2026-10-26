# Design and behavior

Bionic Reader is a local Rust command-line program for word-prefix emphasis in
text streams and opt-in interactive shell sessions. It keeps text on the user's
machine and preserves source content and layout. It does not promise a reading
speed improvement.

See the [README](../README.md) for installation and use, [ADR 0002](adr/0002-document-rendering-and-reader.md)
for the document-rendering and reader decisions, and the [verification record](verification.md)
for tested behavior and release gates.

## Unicode text processing

The default segmenter uses ICU4X Unicode word boundaries, including automatic
support for complex scripts, then aligns ranges to extended grapheme clusters.
`FixationEngine` selects letter-prefix ranges from those word ranges. Fractions
count letter graphemes, keeping combining marks and joined graphemes intact;
punctuation and normalization are preserved. Applications can provide their
own `WordSegmenter` and reuse a `FixationEngine` across text fragments.

The default settings emphasize half of each eligible word, select every word,
and require at least two letters. Subtle, balanced, and strong profiles set
fraction, frequency, and minimum length. Inclusive length rules can override
the fraction for shorter words. An explicit profile is applied before numeric
overrides from the same settings source.

## Document rendering and controls

Document input can be ordinary text, CommonMark Markdown, or trusted terminal
output. Output can be ANSI, plain text, Markdown, or escaped HTML. ANSI and
plain processing preserve ordinary bytes, including malformed UTF-8; Markdown
and HTML conversion require valid UTF-8. The interactive reader displays
malformed bytes with replacement characters.

The default safe control policy retains valid SGR styling and OSC 8 hyperlinks
with HTTP, HTTPS, or mailto destinations for ANSI output, while removing other
terminal controls. With ANSI output, `--controls preserve` or
`--input-format terminal` preserves controls for trusted sources. Plain output
removes all controls regardless of that policy. HTML escapes raw HTML and only
emits links with safe destinations.
Markdown parsing protects code, link destinations, escapes, and entities; code
is protected by default. Markdown-to-Markdown conversion rejects code
emphasis when code protection is disabled, since emphasis markers would change
literal code content.
Markdown output always uses CommonMark strong syntax, independent of the
configured emphasis style. HTML reverse emphasis emits a `bionic-reverse` class;
consumers supply CSS if they want that class to change foreground and background
colors.

## Streaming and interactive reading

Terminal transformation is incremental and deterministic by default. A
trailing word is held until a boundary arrives; a word exceeding the 64 KiB
pending limit is passed through unchanged. With safe document controls, an
incomplete control sequence over 8 KiB is discarded and drained without retaining
its payload. Trusted terminal input preserves control bytes while bounding the
transformer's retained parser state.
`--low-latency` flushes a stalled partial word
unchanged after 50 ms for ANSI filtering of text/terminal input; reading sessions use the same
time-bounded behavior to keep command output responsive.

Markdown streaming spools input to disk and processes bounded blocks. Blocks
and reference-definition catalogs are each limited to 1 MiB; oversized input
uses an unstyled literal fallback, escaped when the selected output format
requires it. The in-memory Markdown rendering API accepts inputs up to 8 MiB;
use the streaming API for larger documents.

The interactive reader also spools the document to disk and stores line
positions in memory. It reads view rows and search ranges from the spool,
protects Markdown code and links when requested, wraps on grapheme boundaries,
and restores terminal state when it exits. Its line index grows with the
document's line count; the document text itself is not retained in memory.

## Configuration and shell sessions

Settings load from built-in defaults, an optional TOML file, and command-line
overrides, in that order. The default path is
`$XDG_CONFIG_HOME/bionic-reader/config.toml` when `XDG_CONFIG_HOME` is absolute,
otherwise `~/.config/bionic-reader/config.toml`. A profile in a source is
applied before that source's explicit numeric values. Additional command
exclusions are appended to the built-in safety list.

Opt-in shell sessions are supported in Bash, Zsh, and fish. Their initializers
and framework adapters use the same session implementation. Known interactive
commands and explicit exclusions run in passthrough; background output,
nested programs, and application-generated prompts cannot always be classified
reliably. The session controls `on`, `off`, `toggle`, `reload`, and `bypass`
affect the current session.

The current native candidate targets are macOS and GNU or musl Linux on x86-64
and ARM64. Windows is outside the current target matrix. Packages are built
from this repository. GitHub Releases distribute verified native archives,
Linux system packages, Python wheels, and standalone Node tarballs using the
policy in [RELEASE.md](RELEASE.md). The README retains every package-manager
installation route. Native package managers select architecture/libc from their metadata; Homebrew
uses a generated release formula published to its tap. Cargo/Snap retain their
repository routes. Package publication is isolated from release preparation.
Registry publication and host-specific validation remain separate gates.
