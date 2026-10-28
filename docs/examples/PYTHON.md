# Python examples

Use the shared local transformation engine from Python 3.11 or newer. These
examples cover the application API introduced in 0.2.0. See
[installation details](../INSTALL.md) for supported macOS/Linux platforms.

```sh
python -m pip install bionic-reader
```

The snippets below share this setup:

```python
from bionic_reader import BionicError, Reader, parse_config, render, segments

reader = Reader()
```

## Render HTML, Markdown, ANSI, or plain text

Module-level functions are convenient for individual calls. Reuse `Reader` when
processing multiple documents. `render()` always requires an explicit format.

```python
assert render("Hello world", format="html") == (
    "<strong>Hel</strong>lo <strong>wor</strong>ld"
)
assert render("Hello world", format="markdown") == "**Hel**lo **wor**ld"
assert render("Hello world", format="plain") == "Hello world"
print(render("Hello world", format="ansi"))

html = reader.render("Another document", format="html")
```

ANSI output contains terminal escape sequences. Plain output adds no emphasis
and strips terminal controls. HTML output is a fragment; plain-text input is
escaped rather than interpreted as HTML.

## Choose profiles and explicit settings

```python
text = "Reading complex documentation becomes easier."

for profile in ("subtle", "balanced", "strong"):
    print(reader.render(text, format="html", profile=profile))

html = reader.render(
    text,
    format="html",
    profile="strong",
    fraction=0.4,
    every=2,
    min_word_length=4,
)
```

| Profile | Fraction | Every | Minimum word length |
| --- | ---: | ---: | ---: |
| `subtle` | 0.3 | 2 | 4 |
| `balanced` (default) | 0.5 | 1 | 2 |
| `strong` | 0.7 | 1 | 2 |

Explicit values override the profile. `every=2` selects the first eligible word,
then every second eligible word. Minimum length counts letter graphemes.

## Set emphasis length by word length

```python
html = reader.render(
    "Read complex documentation",
    format="html",
    fraction=0.7,
    length_rules=[
        {"max_length": 4, "fraction": 0.25},
        {"max_length": 8, "fraction": 0.5},
    ],
)

assert render("Hello", format="html", fraction=0) == "Hello"
assert render("Hello", format="html", fraction=1) == "<strong>Hello</strong>"
```

Rules use the first matching inclusive threshold. Words up to four letters use
25%; words from five through eight use 50%; longer words use the fallback 70%.
Thresholds must increase.

## Use bold, underline, or reverse emphasis

```python
assert render("Hello", format="html", emphasis="bold") == "<strong>Hel</strong>lo"
assert render("Hello", format="html", emphasis="underline") == "<u>Hel</u>lo"
assert render("Hello", format="html", emphasis="reverse") == (
    '<span class="bionic-reverse">Hel</span>lo'
)

for emphasis in ("bold", "underline", "reverse"):
    print(reader.render("Hello world", format="ansi", emphasis=emphasis))
```

HTML reverse emphasis requires CSS in the consuming page:

```css
.bionic-reverse { color: white; background: black; }
```

Markdown always uses `**strong**` syntax, regardless of `emphasis`.

## Process Markdown and protect code

````python
source = """# Hello `world`

Read [the guide](https://example.com).

```python
print("Hello world")
```
"""

for output_format in ("html", "markdown", "ansi", "plain"):
    result = reader.render(
        source, input_format="markdown", format=output_format
    )
    print(result)

assert reader.render(
    "# Hello `world`\n", input_format="markdown", format="html"
) == "<h1><strong>Hel</strong>lo <code>world</code></h1>\n"
````

Code and link destinations are protected by default. Markdown-to-Markdown and
Markdown-to-ANSI retain source syntax such as heading markers and backticks.
Markdown-to-plain preserves the source syntax too; it does not extract rendered
prose. HTML output escapes raw HTML and removes unsafe link schemes.

To include code in added emphasis, select HTML or ANSI output:

```python
html = reader.render(
    'Read `print("hello")` now.',
    input_format="markdown",
    format="html",
    skip_code=False,
)
```

Markdown-to-Markdown rejects `skip_code=False` because code cannot contain
CommonMark emphasis without changing its literal content.

## Disable added emphasis while retaining conversion

```python
assert reader.render(
    "# Hello `world`\n",
    input_format="markdown",
    format="html",
    enabled=False,
) == "<h1>Hello <code>world</code></h1>\n"

assert render("<tag> & text", format="html", enabled=False) == (
    "&lt;tag&gt; &amp; text"
)
```

Control filtering and format conversion still apply when emphasis is disabled.

## Get text segments for custom presentation

```python
parts = segments("Hello world")
assert parts == [
    {"text": "Hel", "emphasized": True},
    {"text": "lo ", "emphasized": False},
    {"text": "wor", "emphasized": True},
    {"text": "ld", "emphasized": False},
]

parts = reader.segments("Reading complex documentation", profile="subtle")
assert "".join(part["text"] for part in parts) == "Reading complex documentation"
assert reader.segments("") == []

markdown_parts = reader.segments("# Hello `world`\n", input_format="markdown")
```

Segments contain literal text and an emphasis flag, without generated markup,
terminal controls, or string offsets. Markdown segments retain Markdown source
and protect code; they are not the result of rendering the Markdown to prose.

For example, create HTML with your own CSS classes:

```python
from html import escape

def custom_html(text):
    pieces = []
    for part in reader.segments(text):
        literal = escape(part["text"])
        if part["emphasized"]:
            literal = f'<span class="fixation">{literal}</span>'
        pieces.append(literal)
    return '<div class="reading">' + "".join(pieces) + "</div>"

html = custom_html("Hello <world>\nAnother line")
```

```css
.reading { white-space: pre-wrap; }
.fixation { font-weight: 600; }
```

Escape every literal segment when constructing HTML. A GUI can instead map the
same segments to styled text widgets.

## Preserve multilingual text and whitespace

```python
text = "Español: acción. Français: déjà. 日本語。ภาษาไทย\nCafe\u0301 👩🏽‍💻 123"
html = reader.render(text, format="html")
assert "".join(part["text"] for part in reader.segments(text)) == text
```

Built-in Unicode segmentation handles word and grapheme boundaries without a
language option. Digits and emoji receive no added emphasis. Preservation is
subject to terminal-control filtering.

## Handle terminal formatting

```python
colored = "\x1b[31mHello world\x1b[0m"
terminal_output = reader.render(colored, format="ansi", controls="safe")
assert reader.render(colored, format="plain") == "Hello world"
assert "".join(p["text"] for p in reader.segments(colored)) == "Hello world"

# Explicit preservation for trusted terminal input only.
trusted_output = reader.render(colored, format="ansi", controls="preserve")
trusted_output = reader.render(colored, format="ansi", input_format="terminal")
```

Safe ANSI output retains recognized styling and safe terminal hyperlinks while
removing active controls. Plain output and text segments always strip terminal
controls, even when `controls="preserve"` is supplied.

## Parse TOML configuration explicitly

```python
options = parse_config('''
profile = "strong"
fraction = 0.25
every = 1
min_word_length = 2
emphasis = "bold"
style = "always"
''')
html = reader.render("Reading documentation", format="html", **options)

# Merge first so each keyword is passed only once.
options = {**options, "fraction": 0.6}
html = reader.render("Reading documentation", format="html", **options)
```

`parse_config()` accepts TOML contents, not a filename. `style="never"` disables
added emphasis; `style="auto"` enables it deterministically. Session-only fields
are ignored. Parsed CLI `format` does not select the API output format; supply
`format` in `render()`.

## Read files and export HTML, Markdown, or JSON

Create `reading.toml` with the configuration above and an `article.md` document:

```python
import json
from pathlib import Path

options = parse_config(Path("reading.toml").read_text(encoding="utf-8"))
options = {**options, "input_format": "markdown"}
source = Path("article.md").read_text(encoding="utf-8")
body = reader.render(source, format="html", **options)
page = (
    '<!doctype html><html lang="en"><meta charset="utf-8">'
    f'<title>Reading</title><body>{body}</body></html>'
)
Path("article.html").write_text(page, encoding="utf-8")
Path("article-bionic.md").write_text(
    reader.render(source, format="markdown", **options), encoding="utf-8"
)
Path("segments.json").write_text(
    json.dumps(reader.segments(source, **options), ensure_ascii=False, indent=2),
    encoding="utf-8",
)
```

## Process a batch

```python
documents = ["First document", "Second document", "Third document"]
pages = [reader.render(doc, format="html", profile="balanced") for doc in documents]
```

Each call starts a fresh emphasis-frequency sequence.

## Handle errors and inspect the installed version

```python
import bionic_reader

print(bionic_reader.__version__)

try:
    reader.render("Hello", format="html", fraction=1.5)
except BionicError as error:
    assert isinstance(error, ValueError)
    assert error.code == "invalid_input"
    print(str(error))
```

Invalid formats, unknown options, incorrect option types, malformed Unicode,
non-finite fractions, and unsupported combinations raise `BionicError`. The
required Python `format` argument must be supplied; omitting it is a normal
Python `TypeError`.

## Options and limits

| Option | Values |
| --- | --- |
| `format` | Required by `render()`: `ansi`, `plain`, `markdown`, `html` |
| `input_format` | `text` (default), `markdown`, `terminal` |
| `enabled` | Boolean, default `True` |
| `profile` | `subtle`, `balanced`, `strong` |
| `fraction` | Finite number in `[0, 1]` |
| `every`, `min_word_length` | Positive integers |
| `length_rules` | Increasing inclusive `{max_length, fraction}` thresholds |
| `emphasis` | `bold` (default), `underline`, `reverse` |
| `controls` | `safe` (default), `preserve` |
| `skip_code` | Boolean, default `True` |

The API processes whole Unicode strings. Markdown input is limited to 8 MiB of
UTF-8; other inputs are bounded by runtime memory. Word-count settings cannot
exceed 4,294,967,295. Python does not expose raw bytes, streaming, or custom
segmenter callbacks.

Calls do not discover configuration files or inspect terminal state, `NO_COLOR`,
or `TERM`. The wheel also installs the CLI; interactive reading and shell reading
sessions remain CLI features. See the [library guide](../LIBRARY.md) for the
shared contract and [BUILD.md](../BUILD.md) for local package builds.
