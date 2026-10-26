# Using Bionic Reader

## Usage examples

Pass one string with `-t` / `--text`, or pipe text on standard input:

```sh
bionic-reader -t 'Bionic reading keeps the original words.'
cat article.txt | bionic-reader
printf 'A stream can be transformed.\n' | bionic-reader --style always

# Emphasize 40% of every second eligible word of at least four letters.
bionic-reader -t 'Read the first part of selected words.' --fraction 0.4 --every 2 --min-word-length 4

# Keep ANSI emphasis when saving output or sending it through another pipe.
bionic-reader --style always -t 'Save the emphasized text.' > reading.ansi

# Preserve the input without adding emphasis.
printf 'Original text\n' | bionic-reader --style never
```

The default `--style auto` adds emphasis only on terminal stdout, honoring nonempty `NO_COLOR` and `TERM=dumb`. `--style always` explicitly overrides these hints; `--style never` adds no emphasis. For ANSI output, the default safe policy keeps recognized presentation controls and removes other terminal controls. Use `--controls preserve` or `--input-format terminal` only for trusted ANSI input; plain output always removes controls. With no `-t`, an interactive standard input shows help instead of waiting for text.

Tune word selection with `--fraction`, `--every`, and `--min-word-length`:

```sh
bionic-reader -t 'Read the first part of each word.' --fraction 0.4 --every 2
```

`fraction` is a finite value from `0` to `1`; `every` and `min-word-length` must be positive integers. The default fraction is `0.5`, default frequency is `1`, and default minimum length is `2`. Word boundaries use ICU4X Unicode word segmentation and its automatic complex-script data. Emphasis boundaries use extended grapheme clusters, so accents and emoji sequences stay intact. Only letter graphemes count toward fixation length; digits and emoji receive no added emphasis. Internal apostrophes stay within words, while hyphens separate segments. Automatic segmentation is not a promise of perfect linguistic analysis for every language; the library accepts custom segmenters.

## Document formats and safe input

```sh
# Preserve Markdown headings, lists, links, and code while adding terminal emphasis.
bionic-reader --input-format markdown --style always < article.md

# Export Markdown emphasis or escaped HTML. Code is protected by default.
bionic-reader --input-format markdown --format markdown < article.md > emphasized.md
bionic-reader --input-format markdown --format html < article.md > article.html

# Remove terminal escapes without adding emphasis; preserve the remaining source text.
bionic-reader --format plain < colored-output.txt > plain.txt

# Explicitly preserve all controls in trusted terminal output.
command-producing-terminal-output | bionic-reader --input-format terminal
```

`--format ansi|plain|markdown|html` selects a representation; `--style` controls
whether emphasis is added. Markdown/HTML exports add emphasis even when stdout
is redirected, unless `--style never` is selected. Markdown always uses
CommonMark `**strong**` syntax, regardless of `--emphasis`. HTML uses strong or
underline markup; reverse emphasis emits a `bionic-reverse` class that requires
CSS in the consuming page. Plain output strips terminal controls and adds no
markup. Markdown source processing protects code, link destinations, escapes,
and entities. For Markdown input, `--skip-code=false` allows code emphasis in
ANSI or HTML output; Markdown-to-Markdown output rejects that setting because
CommonMark code cannot contain emphasis without changing its literal content.
HTML output escapes raw HTML
and removes unsafe link schemes. When reprocessing exported Markdown, select
`--input-format markdown`; ordinary text input treats markup as literal text.

ANSI document input defaults to `--controls safe`, which retains SGR styling
and safe OSC 8 hyperlinks while removing active controls such as clipboard
commands, cursor movement, DCS, and unsafe hyperlinks. Tabs, line endings, and
ordinary content are retained. Use `--controls preserve` or
`--input-format terminal` with ANSI output only for trusted terminal bytes.
Plain output always strips controls; reading sessions preserve terminal
controls because shell applications require them. ANSI/plain processing
preserves malformed UTF-8 bytes outside removed controls; Markdown and HTML
conversion report invalid UTF-8 as an error.

Normal filtering is deterministic across input chunking and delays. It waits for
a complete word or passes oversized words through at the 64 KiB pending bound.
For ANSI filtering of text or terminal input, `--low-latency` flushes a stalled partial word unchanged after
50 ms; its remaining bytes stay unchanged until the next boundary. Reading
sessions retain this latency behavior. This explicit mode can change which words
receive emphasis depending on input timing.

Markdown streaming spools input to disk and parses bounded blocks. A block or
reference-definition catalog exceeding 1 MiB uses an unstyled literal fallback,
with Markdown/HTML escaping as appropriate; document size itself is unlimited
by the streaming API. The in-memory Markdown-input API accepts up to 8 MiB; use
`render_stream` for larger Markdown documents.

## Fixation profiles and preview

```sh
bionic-reader --profile subtle -t 'A lighter reading preference.'
bionic-reader --profile balanced --fraction 0.6 -t 'Explicit values override profiles.'
bionic-reader --profile strong --emphasis underline -t 'Emphasis without brighter text.'

# Short words get 25%, medium words 50%, and longer words the fallback 70%.
bionic-reader --length-rule 4:0.25 --length-rule 8:0.5 --fraction 0.7 -t 'Adjust emphasis by word length.'

# Open a live preview; use --print for a noninteractive preview.
bionic-reader preview
bionic-reader preview --text 'Reading complex documentation becomes easier.' --print --style always
```

Profiles initialize `(fraction, every, minimum length)` to subtle `(0.3, 2, 4)`,
balanced `(0.5, 1, 2)`, or strong `(0.7, 1, 2)`. Explicit numeric flags override
profile values. Length-rule thresholds are inclusive and must increase; longer
words use `fraction`. `--min-word-length 4` skips words of three letters or fewer.
`--emphasis bold|underline|reverse` selects a terminal attribute, preserving
existing colors and restoring pre-existing intensity/underline/reverse state.

## Interactive document reader

```sh
bionic-reader read article.txt
cat article.txt | bionic-reader read
```

The reader spools input to a temporary file and uses the controlling terminal for
keyboard input, so piped documents work. It strips active controls before display,
wraps on grapheme boundaries, responds to resizing, and restores terminal settings
on exit, errors, and handled termination signals. Invalid UTF-8 is shown with replacement characters. A grapheme exceeding 4 KiB is displayed as one replacement cell while its full source range remains intact on disk. A line-position index is kept
in memory; document text and search scans stay on disk.

| Key | Action |
| --- | --- |
| Arrow keys / `j`, `k` | Scroll |
| Page Up / Page Down | Move a page |
| Home / End / `g`, `G` | First / last position |
| `/`, then Enter | Search |
| `n`, `N` | Next / previous match |
| `+`, `-` | Increase / decrease fixation fraction |
| `[` / `]` | Adjust emphasis frequency |
| `m` / `M` | Adjust minimum word length |
| `p` | Cycle profiles |
| `s` | Show or hide the settings panel |
| `?` | Help |
| `q`, Escape, Ctrl-C | Exit |

Use `--input-format markdown read article.md` to protect code and link destinations in the reader.

Reader adjustments apply to the current view and do not overwrite the config
file. The reader requires a controlling terminal; exported formats work in
noninteractive pipelines.
