# Go examples

Use Go 1.25 or newer. The module includes its engine, so consumers need no CGO,
Rust compiler, separate CLI, or runtime downloads. These APIs start with 0.3.0.

```sh
go get github.com/arturoaviles/bionic-reader
```

See the [package reference](https://pkg.go.dev/github.com/arturoaviles/bionic-reader)
for exported types. Go modules use immutable Git tags and the public module
proxy; no registry account or token is required.

## Initialize and close a reader

This is a complete program. Try subsequent snippets individually inside `main`,
replacing the first Render example after `defer reader.Close()` and using the
same `ctx` and `reader`.

```go
package main

import (
    "context"
    "fmt"
    bionicreader "github.com/arturoaviles/bionic-reader"
)

func main() {
    ctx := context.Background()
    reader, err := bionicreader.NewReader(ctx)
    if err != nil { panic(err) }
    defer reader.Close()
    result, err := reader.Render(ctx, "Hello world", bionicreader.HTML, bionicreader.Options{})
    if err != nil { panic(err) }
    fmt.Println(result)
    // <strong>Hel</strong>lo <strong>wor</strong>ld
}
```

The reader reuses a compiled engine and creates an isolated instance per call.
Calls are safe concurrently and start fresh emphasis-frequency sequences. Do not
copy a reader. `Close()` is idempotent and waits for active calls; cancel their
contexts to interrupt them. Initialization's context bounds initialization
without controlling the reader's later lifetime.

## Choose a format, profile, and explicit settings

```go
for _, format := range []bionicreader.Format{
    bionicreader.HTML, bionicreader.Markdown, bionicreader.ANSI, bionicreader.Plain,
} {
    result, err := reader.Render(ctx, "Hello world", format, bionicreader.Options{})
    if err != nil { panic(err) }
    fmt.Println(result)
}

options := bionicreader.Options{
    Profile: "strong",
    Fraction: bionicreader.Pointer(0.4),
    Every: bionicreader.Pointer(uint32(2)),
    MinWordLength: bionicreader.Pointer(uint32(4)),
}
result, err := reader.Render(ctx, "Read the first part of selected words.", bionicreader.HTML, options)
if err != nil { panic(err) }
fmt.Println(result)
```

HTML returns a fragment such as `<strong>Hel</strong>lo <strong>wor</strong>ld`.
Markdown produces `**Hel**lo **wor**ld`. ANSI adds terminal styling. Plain strips
terminal controls and adds no emphasis. Format is always explicit.

| Profile | Fraction | Every | Minimum word length |
| --- | ---: | ---: | ---: |
| `subtle` | 0.3 | 2 | 4 |
| `balanced` (default) | 0.5 | 1 | 2 |
| `strong` | 0.7 | 1 | 2 |

Explicit settings override profiles. Frequency selects the first eligible word,
then every Nth eligible word. Length counts letter graphemes.

## Preserve explicit zero and false values

Zero-value `Options{}` selects enabled, balanced defaults. Pointers distinguish
omitted values from explicit zero or false:

```go
for _, options := range []bionicreader.Options{
    {Fraction: bionicreader.Pointer(0.0)},
    {Enabled: bionicreader.Pointer(false)},
} {
    result, err := reader.Render(ctx, "Hello", bionicreader.HTML, options)
    if err != nil { panic(err) }
    fmt.Println(result) // Hello
}
```

Disabling emphasis still performs conversion and control filtering. A fraction
of 1.0 emphasizes whole selected words.

## Apply length rules and choose visual emphasis

```go
options := bionicreader.Options{
    Fraction: bionicreader.Pointer(0.7),
    LengthRules: []bionicreader.LengthRule{
        {MaxLength: 4, Fraction: 0.25},
        {MaxLength: 8, Fraction: 0.5},
    },
}
result, err := reader.Render(ctx, "Read complex documentation", bionicreader.HTML, options)
if err != nil { panic(err) }
fmt.Println(result)

for _, emphasis := range []string{"bold", "underline", "reverse"} {
    result, err := reader.Render(ctx, "Hello", bionicreader.HTML,
        bionicreader.Options{Emphasis: emphasis})
    if err != nil { panic(err) }
    fmt.Println(result)
}
```

Rules use the first matching inclusive threshold and must increase. The example
uses 25% through four letters, 50% from five through eight, then the global 70%.
HTML styles use `<strong>`, `<u>`, or `<span class="bionic-reverse">`; reverse
needs CSS such as `.bionic-reverse { color: white; background: black; }`.
The same attributes work in ANSI. Markdown always uses `**strong**`.

## Convert Markdown and protect code

```go
source := "# Hello `world`\n\nRead [the guide](https://example.com).\n"
options := bionicreader.Options{InputFormat: "markdown"}
for _, format := range []bionicreader.Format{
    bionicreader.HTML, bionicreader.Markdown, bionicreader.ANSI, bionicreader.Plain,
} {
    result, err := reader.Render(ctx, source, format, options)
    if err != nil { panic(err) }
    fmt.Println(result)
}
options.SkipCode = bionicreader.Pointer(false)
result, err := reader.Render(ctx, "Read `hello` now.", bionicreader.HTML, options)
if err != nil { panic(err) }
fmt.Println(result)
```

Code and link destinations are protected by default. HTML renders structure,
escapes raw HTML, and removes unsafe link schemes. Markdown and ANSI retain
source syntax; plain retains it too without emphasis, rather than extracting
rendered prose. `SkipCode: Pointer(false)` permits code emphasis for HTML and
ANSI, but is rejected for Markdown-to-Markdown.

## Obtain Unicode text segments and custom HTML

```go
parts, err := reader.Segments(ctx, "Hello world", bionicreader.Options{})
if err != nil { panic(err) }
for _, part := range parts { fmt.Printf("%q emphasized=%t\n", part.Text, part.Emphasized) }
// "Hel" emphasized=true
// "lo " emphasized=false
// "wor" emphasized=true
// "ld" emphasized=false

text := "Español: acción. 日本語。ภาษาไทย\nCafe\u0301 👩🏽‍💻 123"
parts, err = reader.Segments(ctx, text, bionicreader.Options{})
if err != nil { panic(err) }
var restored string
for _, part := range parts { restored += part.Text }
fmt.Println(restored == text) // true
```

Segments contain literal text and an emphasis flag, without markup, terminal
controls, or byte offsets. Empty input returns an empty slice. Markdown segments
retain source syntax and protect code, rather than returning rendered prose.
Unicode segmentation needs no language option; digits and emoji receive no added
emphasis. Preservation is subject to terminal-control filtering.

Add `"html"` and `"strings"` to the imports for custom HTML:

```go
parts, err := reader.Segments(ctx, "Hello <world>\nAnother line", bionicreader.Options{})
if err != nil { panic(err) }
var body strings.Builder
for _, part := range parts {
    literal := html.EscapeString(part.Text)
    if part.Emphasized { literal = `<span class="fixation">` + literal + `</span>` }
    body.WriteString(literal)
}
fmt.Println(`<div style="white-space: pre-wrap">` + body.String() + `</div>`)
```

Use `.fixation { font-weight: 600; }` in CSS. Escape each literal segment.

## Handle terminal controls

```go
colored := "\x1b[31mHello world\x1b[0m"
plain, err := reader.Render(ctx, colored, bionicreader.Plain, bionicreader.Options{})
if err != nil { panic(err) }
fmt.Println(plain) // Hello world
// Explicit preservation for trusted ANSI input only.
result, err := reader.Render(ctx, colored, bionicreader.ANSI,
    bionicreader.Options{Controls: "preserve"})
if err != nil { panic(err) }
fmt.Println(result)
```

Safe ANSI retains recognized styling and safe terminal hyperlinks while removing
active controls. `InputFormat: "terminal"` also opts into trusted ANSI preservation.
Plain output and segments always strip terminal controls.

## Parse explicit preferences and convert files

Add `"os"` and `"encoding/json"` to the imports. Given `reading.toml` containing
`profile = "strong"` and `fraction = 0.25`, and an `article.md` document:

```go
preferences, err := os.ReadFile("reading.toml")
if err != nil { panic(err) }
options, err := reader.ParseConfig(ctx, string(preferences))
if err != nil { panic(err) }
options.Fraction = bionicreader.Pointer(0.6)
options.InputFormat = "markdown"
source, err := os.ReadFile("article.md")
if err != nil { panic(err) }
body, err := reader.Render(ctx, string(source), bionicreader.HTML, options)
if err != nil { panic(err) }
page := `<!doctype html><html lang="en"><meta charset="utf-8"><title>Reading</title><body>` + body + `</body></html>`
if err := os.WriteFile("article.html", []byte(page), 0o644); err != nil { panic(err) }
markdown, err := reader.Render(ctx, string(source), bionicreader.Markdown, options)
if err != nil { panic(err) }
if err := os.WriteFile("article-bionic.md", []byte(markdown), 0o644); err != nil { panic(err) }
parts, err := reader.Segments(ctx, string(source), options)
if err != nil { panic(err) }
data, err := json.MarshalIndent(parts, "", "  ")
if err != nil { panic(err) }
if err := os.WriteFile("segments.json", data, 0o644); err != nil { panic(err) }
```

Parsing takes contents and performs no I/O. `style = "never"` disables emphasis;
`auto` enables it deterministically. Session-only fields are ignored. Output
format is always explicit. No configuration file, terminal state, `TERM`, or
`NO_COLOR` is inspected automatically.

## Cancel work and handle typed errors

Add `"errors"` and `"time"` to the imports:

```go
deadline, cancel := context.WithTimeout(ctx, time.Second)
defer cancel()
_, err = reader.Render(deadline, "Hello", bionicreader.HTML,
    bionicreader.Options{Fraction: bionicreader.Pointer(1.5)})
var failure *bionicreader.Error
switch {
case errors.Is(err, context.Canceled), errors.Is(err, context.DeadlineExceeded):
    fmt.Println("Operation canceled")
case errors.As(err, &failure):
    fmt.Println(failure.Code, failure.Message) // invalid_input ...
case err != nil:
    panic(err)
}
```

Codes are `invalid_input`, `initialization_failed`, `engine_failed`, and
`closed_reader`. `errors.Is(err, bionicreader.ErrClosed)` identifies a closed or
uninitialized reader. Canceled operations leave later calls usable. Go strings
with invalid UTF-8 are rejected rather than replaced.

## Options and limits

Options are `Enabled`, `Profile`, `Fraction`, `Every`, `MinWordLength`,
`LengthRules`, `Emphasis`, `InputFormat`, `Controls`, and `SkipCode`. JSON names
remain snake_case. Optional Boolean and numeric settings use pointers; an empty
profile or enum-like string selects the default. See the
[shared options contract](../LIBRARY.md#options-and-document-contracts).

Fractions must be finite and in `[0, 1]`; counts must be positive and at most
4,294,967,295. Whole-string Markdown input is limited to 8 MiB of UTF-8. Other
inputs are bounded by available memory and WebAssembly's linear-memory limit.
JSON, output, and isolated instances add memory overhead proportional to the
document size. Raw bytes, streaming, interactive terminal sessions, and custom
segmenter callbacks are outside the Go API.

Local engine builds and regeneration belong in [BUILD.md](../BUILD.md).
