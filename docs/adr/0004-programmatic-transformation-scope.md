# Programmatic transformation and React rendering

Expose document transformation through callable Rust, Python, and JavaScript
APIs, sharing the CLI's transformation settings and input/output formats.
Interactive terminal reading and shell reading sessions remain CLI features;
embedding those terminal interactions is outside the package parity contract.
This boundary gives application developers the same text-processing capabilities
without requiring them to embed terminal interaction.

JavaScript supports Node and browsers, and React supports server-side rendering.
Applications must not need a backend service or a separately installed CLI to
render bionic text. The initial React component accepts plain text and renders
markup literally; Markdown input, raw HTML input, and transformation of arbitrary
React children are outside its initial scope. These restrictions keep the
component's content and rendering contract explicit while the general APIs retain
document-format support.

Provide formatted output and ordered text segments carrying their text and
emphasis state, so clients can implement custom presentation without translating
Rust byte offsets into other languages' string indices. The initial Python and
JavaScript APIs process whole Unicode strings; streaming and raw-byte interfaces
in those languages are deferred. Existing Rust byte and streaming APIs remain
available. Input-size limits and failures must be documented explicitly.

New programmatic entry points use explicit options and built-in defaults, with
emphasis enabled by default. They do not implicitly read CLI configuration,
terminal state, NO_COLOR, or TERM; loading CLI configuration is an explicit
operation. This makes equivalent calls deterministic across application
environments.

Formatted-output calls require an explicit output format: ANSI, plain, Markdown,
or HTML. Input defaults to plain text and emphasis uses the existing balanced
defaults. Text-segment calls and the React component do not need an output
format. Existing lower-level Rust interfaces retain their compatibility contract.

Reject invalid options and malformed Unicode with structured,
language-appropriate errors rather than silently replacing characters. Apply the
existing safe document-control policy by default; text segments represent visible
text after that policy is applied, preserving its Unicode content and whitespace.
Trusted terminal-control preservation is an explicit formatted-output option,
not a React behavior.

Initial Python and JavaScript APIs use the shared built-in multilingual
segmenter. Custom word-segmenter hooks in those languages are deferred; preserve
Rust's independently exposed segmentation and fixation extension APIs from
ADR 0002. The initial parity promise therefore covers built-in segmentation,
document formats, and emphasis settings rather than every Rust extension point.

The React component preserves whitespace and renders text nodes with styled
spans, using bold prefixes by default and allowing CSS customization. It does
not inject generated HTML or add semantic spoken emphasis to every word.
