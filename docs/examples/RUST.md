# Rust examples

Use Rust 1.85.1 or newer and add the crate from crates.io:

```sh
cargo add bionic-reader
```

The `reading::Reader` examples below use the application API introduced in
0.2.0. Earlier CLI-only releases do not expose that module; check
[crates.io](https://crates.io/crates/bionic-reader) for registry availability.

## Start with a reusable reader

This is a complete `src/main.rs`. Try each subsequent snippet separately inside
`main`, before `Ok(())`, using these imports, `reader`, and `options`. Examples
that use files expect `article.md` and `reading.toml` in the current directory.

```rust
use bionic_reader::reading::{Options, Reader};
use bionic_reader::settings::{
    ControlPolicy, Emphasis, InputFormat, LengthRule, OutputFormat, Profile,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reader = Reader::new();
    let options = Options::default();
    let html = reader.render("Hello world", OutputFormat::Html, &options)?;
    assert_eq!(html, "<strong>Hel</strong>lo <strong>wor</strong>ld");
    println!("{html}");
    Ok(())
}
```

`Options` is independent of terminal detection and CLI configuration. Calls
start fresh emphasis-frequency sequences, so one reader can process many
documents without carrying word-selection state between them.

## Render all four output formats

```rust
for format in [
    OutputFormat::Html,
    OutputFormat::Markdown,
    OutputFormat::Ansi,
    OutputFormat::Plain,
] {
    println!("{}", reader.render("Hello world", format, &options)?);
}
```

HTML produces `<strong>Hel</strong>lo <strong>wor</strong>ld`; Markdown produces
`**Hel**lo **wor**ld`. ANSI uses terminal styling. Plain adds no emphasis and
strips terminal controls. HTML output is a fragment, and ordinary text is escaped
rather than interpreted as HTML.

## Choose a profile or override its values

```rust
for profile in [Profile::Subtle, Profile::Balanced, Profile::Strong] {
    let configured = Options {
        profile: Some(profile),
        ..Options::default()
    };
    println!("{}", reader.render(
        "Reading complex documentation becomes easier.",
        OutputFormat::Html,
        &configured,
    )?);
}

let configured = Options {
    profile: Some(Profile::Strong),
    fraction: Some(0.4),
    every: Some(2),
    min_word_length: Some(4),
    ..Options::default()
};
let html = reader.render("Read the first part of selected words.", OutputFormat::Html, &configured)?;
```

| Profile | Fraction | Every | Minimum word length |
| --- | ---: | ---: | ---: |
| `Subtle` | 0.3 | 2 | 4 |
| `Balanced` (default) | 0.5 | 1 | 2 |
| `Strong` | 0.7 | 1 | 2 |

Explicit numeric values override profiles. `every = Some(2)` selects the first
eligible word, then every second eligible word. Length counts letter graphemes.

## Apply length rules

```rust
let configured = Options {
    fraction: Some(0.7),
    length_rules: Some(vec![
        LengthRule { max_length: 4, fraction: 0.25 },
        LengthRule { max_length: 8, fraction: 0.5 },
    ]),
    ..Options::default()
};
let html = reader.render("Read complex documentation", OutputFormat::Html, &configured)?;
```

Thresholds are inclusive and must increase. The first matching rule applies;
words longer than eight letters use the fallback fraction, 0.7. Fractions of
0.0 and 1.0 are valid: no prefix emphasis or whole selected words, respectively.

## Choose bold, underline, or reverse

```rust
for emphasis in [Emphasis::Bold, Emphasis::Underline, Emphasis::Reverse] {
    let configured = Options {
        emphasis: Some(emphasis),
        ..Options::default()
    };
    println!("{}", reader.render("Hello", OutputFormat::Html, &configured)?);
    println!("{}", reader.render("Hello", OutputFormat::Ansi, &configured)?);
}
```

HTML uses `<strong>`, `<u>`, or `<span class="bionic-reverse">`. Reverse HTML
needs CSS such as `.bionic-reverse { color: white; background: black; }`.
Markdown always uses `**strong**`, regardless of the selected emphasis attribute.

## Transform Markdown and optionally emphasize code

```rust
let source = "# Hello `world`\n\nRead [the guide](https://example.com).\n";
let markdown_options = Options {
    input_format: Some(InputFormat::Markdown),
    ..Options::default()
};
for format in [OutputFormat::Html, OutputFormat::Markdown, OutputFormat::Ansi, OutputFormat::Plain] {
    println!("{}", reader.render(source, format, &markdown_options)?);
}

let include_code = Options {
    skip_code: Some(false),
    ..markdown_options.clone()
};
let html = reader.render("Read `hello` now.", OutputFormat::Html, &include_code)?;
```

Code and link destinations are protected by default. HTML renders Markdown
structure, escapes raw HTML, and removes unsafe link schemes. ANSI and Markdown
output retain Markdown source syntax. Plain output preserves that source syntax
without emphasis; it does not extract rendered prose. Markdown-to-Markdown
rejects `skip_code = Some(false)`.

## Disable added emphasis

```rust
let disabled = Options {
    enabled: false,
    input_format: Some(InputFormat::Markdown),
    ..Options::default()
};
assert_eq!(
    reader.render("# Hello `world`\n", OutputFormat::Html, &disabled)?,
    "<h1>Hello <code>world</code></h1>\n",
);
```

Conversion and safe-control filtering still apply.

## Obtain text segments and preserve Unicode

```rust
let parts = reader.segments("Hello world", &options)?;
assert_eq!(parts[0].text, "Hel");
assert!(parts[0].emphasized);
let visible: String = parts.iter().map(|part| part.text.as_str()).collect();
assert_eq!(visible, "Hello world");
assert!(reader.segments("", &options)?.is_empty());

let text = "Español: acción. 日本語。ภาษาไทย\nCafe\u{301} 👩🏽‍💻 123";
let restored: String = reader.segments(text, &options)?
    .iter().map(|part| part.text.as_str()).collect();
assert_eq!(restored, text);
```

Segments contain only `text` and `emphasized`. Use them in a GUI or your own
renderer; escape literal segments if constructing HTML. They strip terminal
controls and preserve the remaining Unicode and whitespace. Digits and emoji
receive no added emphasis. Markdown segments retain source syntax and protect
code rather than returning rendered prose.

To serialize segments as JSON, add the registry dependency:

```sh
cargo add serde_json
```

```rust
let json = serde_json::to_string_pretty(&reader.segments("Hello world", &options)?)?;
std::fs::write("segments.json", json)?;
```

## Handle existing terminal controls

```rust
let colored = "\x1b[31mHello world\x1b[0m";
let safe = Options { controls: Some(ControlPolicy::Safe), ..Options::default() };
let ansi = reader.render(colored, OutputFormat::Ansi, &safe)?;
assert_eq!(reader.render(colored, OutputFormat::Plain, &safe)?, "Hello world");

// Explicit preservation for trusted terminal input only.
let trusted = Options { controls: Some(ControlPolicy::Preserve), ..Options::default() };
let ansi = reader.render(colored, OutputFormat::Ansi, &trusted)?;
let terminal = Options { input_format: Some(InputFormat::Terminal), ..Options::default() };
let ansi = reader.render(colored, OutputFormat::Ansi, &terminal)?;
```

Safe ANSI output retains recognized styling and safe terminal hyperlinks while
removing active controls. Plain output and segments always strip controls,
including when preservation was requested.

## Parse explicit TOML and convert files

Default features include configuration parsing. Given `reading.toml` containing
`profile = "strong"` and `fraction = 0.25`, and an `article.md` document:

```rust
let mut configured = Options::parse_config(&std::fs::read_to_string("reading.toml")?)?;
configured.fraction = Some(0.6);
configured.input_format = Some(InputFormat::Markdown);
let source = std::fs::read_to_string("article.md")?;
let body = reader.render(&source, OutputFormat::Html, &configured)?;
let page = format!(
    "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Reading</title><body>{body}</body></html>"
);
std::fs::write("article.html", page)?;
std::fs::write("article-bionic.md", reader.render(&source, OutputFormat::Markdown, &configured)?)?;
```

`Options::parse_config()` takes contents and performs no I/O. `style = "never"`
disables emphasis; `auto` enables it deterministically. Session-only settings and
the CLI output-format preference do not configure `Reader` calls. Choose output
format explicitly. No configuration file, terminal state, `TERM`, or `NO_COLOR`
is inspected by these application APIs.

## Process batches and handle errors

```rust
let documents = ["First document", "Second document", "Third document"];
let pages: Vec<String> = documents.iter()
    .map(|text| reader.render(text, OutputFormat::Html, &options))
    .collect::<Result<_, _>>()?;

let invalid = Options { fraction: Some(1.5), ..Options::default() };
match reader.render("Hello", OutputFormat::Html, &invalid) {
    Err(error) => {
        assert!(error.is_invalid_input());
        eprintln!("{error}");
    }
    Ok(_) => panic!("expected invalid options to fail"),
}
```

The typed API uses `Result`; error inspection includes `is_invalid_input()` and
`io_kind()`. Rust strings are valid UTF-8 by construction.

## Work with bytes or streams

Rust also retains the lower-level document APIs. `document::render()` takes a
byte slice and returns bytes. Plain and ANSI paths can preserve malformed UTF-8
outside removed controls; markup conversion requires valid UTF-8.

```rust
use bionic_reader::document;
use bionic_reader::settings::Settings;

let settings = Settings { format: OutputFormat::Plain, ..Settings::default() };
assert_eq!(document::render(&[b'H', 0xff, b'i'], &settings, false)?, vec![b'H', 0xff, b'i']);
```

With the default features, `document::render_stream()` reads and writes through
the standard I/O traits:

```rust
use bionic_reader::document::render_stream;
use bionic_reader::settings::Settings;
use std::io::{BufReader, BufWriter, Write};

let settings = Settings {
    input_format: InputFormat::Markdown,
    format: OutputFormat::Html,
    ..Settings::default()
};
let mut input = BufReader::new(std::fs::File::open("article.md")?);
let mut output = BufWriter::new(std::fs::File::create("article-stream.html")?);
render_stream(&mut input, &mut output, &settings, true)?;
output.flush()?;
```

Markdown streaming uses temporary files and bounded blocks. A block or reference
catalog exceeding 1 MiB uses a literal, unstyled fallback, escaped for the chosen
format as needed. Total document size is not subject to the whole-string 8 MiB
Markdown limit. Streaming is a separate API from `reading::Reader`.

## Supply custom segmentation and inspect emphasis ranges

Custom segmenters implement `WordSegmenter`. This example keeps Unicode word
boundaries but excludes the literal word `ignore` from emphasis:

```rust
use bionic_reader::document;
use bionic_reader::segmentation::{FixationEngine, UnicodeSegmenter, WordSegmenter};
use bionic_reader::settings::Settings;
use std::ops::Range;

#[derive(Debug)]
struct SkipIgnore(UnicodeSegmenter);

impl WordSegmenter for SkipIgnore {
    fn word_ranges(&self, text: &str) -> Vec<Range<usize>> {
        self.0.word_ranges(text).into_iter()
            .filter(|range| &text[range.clone()] != "ignore")
            .collect()
    }
}

let custom = SkipIgnore(UnicodeSegmenter::new());
let settings = Settings { format: OutputFormat::Html, ..Settings::default() };
let output = document::render_with_segmenter(b"Hello ignore world", &settings, true, &custom)?;
assert_eq!(String::from_utf8(output)?, "<strong>Hel</strong>lo ignore <strong>wor</strong>ld");

let mut fixation = FixationEngine::new(&settings);
let prefixes: Vec<&str> = fixation.spans("Hello ignore world", &custom)
    .into_iter().map(|range| &"Hello ignore world"[range]).collect();
assert_eq!(prefixes, vec!["Hel", "wor"]);
```

Custom ranges must be ordered, non-overlapping, within the string, and aligned to
UTF-8 and extended grapheme boundaries. Invalid implementations can panic.
These are byte ranges; the high-level text segments avoid exposing offsets.
`FixationEngine` retains frequency state across `spans()` calls, unlike `Reader`.

## Choose dependency features and understand limits

Default features include the CLI, native streaming, and configuration. For an
application needing only the portable transformation engine:

```sh
cargo add bionic-reader --no-default-features
```

Add `--features config` for explicit TOML parsing, or `--features native` for
configuration, file-backed settings, and document streaming. These change which
APIs are available; the examples using configuration or streaming require the
corresponding features.

`reading::Options` supports `enabled` and optional `profile`, `fraction`, `every`,
`min_word_length`, `length_rules`, `emphasis`, `input_format`, `controls`, and
`skip_code`. Fractions must be finite and in `[0, 1]`. Word counts must be positive
and no greater than 4,294,967,295 in this portable API.

Whole-string Markdown input is limited to 8 MiB of UTF-8. Other whole-string
inputs are bounded by available memory. Interactive reading and shell reading
sessions remain CLI features. See the [library guide](../LIBRARY.md) for the
shared contract and [BUILD.md](../BUILD.md) for local package builds.
