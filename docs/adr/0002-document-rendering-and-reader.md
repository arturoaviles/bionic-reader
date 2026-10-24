# Document rendering, multilingual fixation, and interactive reading

The engineering-priority review and the request to fix all its gaps expand the
original Latin-only terminal MVP. Rust 1.85.1, byte preservation, opt-in shell
sessions, and repository-based distribution remain constraints.

Use ICU4X word boundaries with compiled dictionary/LSTM data for complex scripts,
and extended grapheme boundaries for emphasis positions. Expose segmentation and
fixation spans independently so clients can supply a different word segmenter.
Preserve input encoding, punctuation, normalization, whitespace, and line endings.
Invalid UTF-8 remains byte-preserving in terminal/plain processing; document
formats that require Unicode must report their policy explicitly.

Keep the existing incremental terminal renderer and add document rendering for
ANSI, plain, Markdown, and escaped HTML. Parse Markdown structure before selecting
text spans; protect code by default and preserve link destinations. Plain output
removes terminal escapes. Markdown and HTML output must preserve literal content
through escaping and avoid adding repeated emphasis to already rendered words.

Document input defaults to safe terminal controls. Active commands such as OSC 52,
cursor operations, and DCS are removed rather than executed by the receiving
terminal. Safe presentation SGR and safe hyperlink destinations may survive ANSI
rendering. `--controls preserve` and `--input-format terminal` explicitly select
trusted terminal input. Reading sessions keep their terminal-control contract.
These policies resolve the conflict between preserving arbitrary terminal bytes
and displaying untrusted documents safely; safe filtering is deliberately not a
promise to preserve control bytes that have active terminal effects.

Normal filtering waits for a word boundary and is independent of input timing.
Bounded oversized-word passthrough remains. The former 50 ms partial-word fallback
is available with `--low-latency`, and remains appropriate inside reading sessions.
This supersedes the unconditional filtering deadline in the original design.

Provide subtle, balanced, and strong profiles, inclusive length-specific fraction
rules, an interactive fixation preview, and bold/underline/reverse attributes.
Configuration precedence is defaults, file profile, file values, flag profile,
then explicit flag values. Auto styling honors nonempty NO_COLOR and TERM=dumb;
an explicit always policy overrides these hints.

Add a document reader with scrolling, paging, search, navigation, live settings,
Unicode-aware wrapping, resizing, and terminal restoration. Spool piped documents
to disk and obtain keyboard input from the controlling terminal. Keep working
memory bounded by indexes and view buffers rather than retaining document text.

Verification includes generated property tests, reproducible fuzz targets with
time-limited CI runs, Unicode/ANSI/Markdown regression cases, real reader PTYs,
and a Linux streaming-memory gate. No reading-speed improvement is promised.
