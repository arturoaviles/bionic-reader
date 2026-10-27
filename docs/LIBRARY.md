# Use Bionic Reader in applications

The Rust, Python, and JavaScript APIs share the CLI's Rust transformation engine,
including multilingual word boundaries, profiles, Markdown handling, and safe
terminal controls. React renders its text segments. Processing runs locally;
applications do not launch the CLI or call a service.

Packages are published on [PyPI](https://pypi.org/project/bionic-reader/),
[npm](https://www.npmjs.com/package/@arturoaviles/bionic-reader), and
[crates.io](https://crates.io/crates/bionic-reader). The current Python/npm
releases provide the CLI, and the Rust crate exposes its existing lower-level
APIs. The new application APIs documented below require the forthcoming library
release. See [installation](INSTALL.md) for package-manager details.

## Rust

Rust 1.85.1 is the minimum compiler. Add the crate from crates.io:

```sh
cargo add bionic-reader
```

```rust
use bionic_reader::reading::{Options, Reader};
use bionic_reader::settings::OutputFormat;

let reader = Reader::new();
let options = Options::default();
let html = reader.render("Hello world", OutputFormat::Html, &options)?;
let segments = reader.segments("Hello world", &options)?;
// In a function returning Result<(), bionic_reader::Error>.
```

`reading::Reader` is the deterministic whole-string API. The existing
`document`, `transformer`, `segmentation`, and settings APIs remain available for
byte streams and custom segmentation. Default features retain the CLI. Disable
default features for the portable engine; enable `config` for explicit TOML
parsing or `native` for file-backed settings and document streaming. `cli`
includes `native` and terminal/session support. The library installs no allocator.

## Python

Install from PyPI into an application environment, using Python 3.11 or newer.

```sh
python -m pip install bionic-reader
```

```python
from bionic_reader import Reader, BionicError, parse_config

reader = Reader()
html = reader.render("Hello world", format="html", profile="balanced")
parts = reader.segments("Hello world", every=2)
# Each item is {"text": "...", "emphasized": True or False}.
options = parse_config('profile = "subtle"')
markdown = reader.render("Hello world", format="markdown", **options)
```

Module-level `render()` and `segments()` are convenience calls; reuse `Reader`
when processing many documents. `BionicError` is a `ValueError` with
`code == "invalid_input"`. `bionic_reader.__version__` matches wheel metadata.
The native extension and retained CLI ship together; no separate executable
installation is needed for imports.

## JavaScript and Node

Use Node 22 or newer and install from npm:

```sh
npm install @arturoaviles/bionic-reader
```

Both ESM and CommonJS provide the same factory:

```js
import { createReader } from '@arturoaviles/bionic-reader';
// CommonJS: const { createReader } = require('@arturoaviles/bionic-reader');
const reader = await createReader();
const html = reader.render('Hello world', { format: 'html', profile: 'balanced' });
const parts = reader.segments('Hello world', { every: 2 });
const options = reader.parseConfig('profile = "subtle"');
```

`createReader()` asynchronously loads bundled WebAssembly. Calls share one loaded
engine; subsequent `render()`, `segments()`, and `parseConfig()` calls are
synchronous. Initialization failures reject with `code ==
"initialization_failed"`; invalid inputs throw with `code == "invalid_input"`.
The APIs do not require React. TypeScript declarations ship with the package;
[ESM](../examples/node/reading.mts) and
[CommonJS](../examples/node/reading.cts) examples demonstrate both module systems.

Browser bundlers resolve a browser loader instead of Node filesystem imports.
Serve the bundled `.wasm` as an asset under your application's normal asset
policy. Initialization fetches that asset once; it requires permission to load
and compile WebAssembly. The [Vite example](../examples/browser) builds an
installed-package consumer with document controls and a text-segment preview.

## React

Install React 18.3 or 19 alongside the npm package. Initialize the reader before
rendering, and pass it through the provider:

```jsx
import { createReader } from '@arturoaviles/bionic-reader';
import { BionicReaderProvider, BionicText } from '@arturoaviles/bionic-reader/react';

const reader = await createReader();
const content = <BionicReaderProvider reader={reader}>
  <BionicText text={'Hello world\nAnother line'}
    options={{ profile: 'balanced' }}
    emphasisStyle={{ fontWeight: 600 }} />
</BionicReaderProvider>;
```

`BionicText` accepts plain Unicode text, renders literal text nodes and styled
spans, and preserves whitespace with `white-space: pre-wrap`. Prefixes default
to font weight 700. Customize them through `emphasisClassName` or
`emphasisStyle`; normal span attributes, `className`, and `style` configure the
outer span. There is no generated HTML injection or semantic spoken emphasis on
every prefix. Markdown, HTML, and arbitrary React children are outside this
component's contract; markup in `text` stays literal.

Initialize independently before server rendering and client hydration, using
identical text and options. An absent provider, a promise in place of a reader,
or an uninitialized reader fails clearly. Rendering starts no downloads and does
not use Suspense to initialize the engine. Applications own their loading and
error UI. See the [generic SSR example](../examples/react) and the
[Next.js example](../examples/next). The Next.js example validates Pages Router
with Webpack; other router/bundler combinations need their own integration check.

## Options and document contracts

Formatted calls require an explicit `ansi`, `plain`, `markdown`, or `html` output
format. Input defaults to `text`; `input_format` also accepts `markdown` and
`terminal`. Emphasis is enabled with balanced defaults: `fraction = 0.5`,
`every = 1`, and `min_word_length = 2`. A profile (`subtle`, `balanced`, `strong`)
is applied before explicit numeric settings. Options use the same snake-case
names in all languages:

| Option | Meaning |
| --- | --- |
| `enabled` | Add emphasis; false still performs format conversion and safe-control filtering. |
| `fraction` | Finite fraction in `[0, 1]` of selected word beginnings. |
| `every` | Select the first eligible word, then every Nth eligible word; positive integer. |
| `min_word_length` | Minimum letter-grapheme count for an eligible word; positive integer. |
| `length_rules` | Increasing inclusive `{max_length, fraction}` thresholds; positive counts and valid fractions. |
| `emphasis` | `bold`, `underline`, or `reverse` for formatted output. |
| `controls` | `safe` by default; explicit `preserve` permits trusted terminal controls in supported formatted output. |
| `skip_code` | Protect Markdown code from additional emphasis; true by default. |

Word-count options must be at most 4,294,967,295 in the portable API. Invalid,
unknown, non-finite, or incorrectly typed options fail. Python and JavaScript
require Unicode strings and reject unpaired surrogates. They do not expose raw
bytes, streaming, or custom segmenters. Rust retains those lower-level APIs.
Whole-string Markdown input has an 8 MiB UTF-8 limit; other inputs remain bounded
by runtime memory, with output/segment copies proportional to document size.

Ordered segments contain only `text` and `emphasized`, with no language-specific
string offsets. Concatenation gives visible text after safe control filtering;
Unicode and whitespace are preserved. Empty input returns an empty list. For
Markdown input, segments retain literal Markdown source and honor protected code;
they are not an HTML or Markdown parser result. React always requests plain,
safe text segments. A reader resets emphasis-frequency state for every call.

These entry points never discover CLI configuration or inspect the terminal,
`NO_COLOR`, or `TERM`. To load CLI preferences, read a file yourself and pass its
TOML contents to Rust `Options::parse_config()`, Python `parse_config()`, or
JavaScript `reader.parseConfig()`. `style = "never"` disables added emphasis;
`auto` is deterministically enabled. Session-only fields are ignored. No I/O is
performed by these parsing functions. See the [configuration guide](CONFIGURATION.md)
for profile values and rule behavior, and the [usage guide](USAGE.md) for format
restrictions and control handling.

## Package verification

Local package builds and installed-candidate checks are documented in
[BUILD.md](BUILD.md#build-and-validate-library-packages).

The language-neutral [fixture corpus](../tests/fixtures/library.json) is consumed
through public Rust, installed Python, Node ESM/CommonJS, and browser APIs. CI
adds native wheel imports on the six existing macOS/Linux targets, Python
3.11–3.15, Node 22/24/26 on Linux/macOS/Windows, React 18/19 types and hydration,
and current Chrome/Edge/Firefox/Safari. A configured job is not validation
evidence: release acceptance requires these jobs to pass. Native Windows CLI,
Python, and Rust distribution are outside the initial target matrix.

Cargo owns the package version. Native Python and Wasm engines carry that
version; packaging rejects stale or missing Wasm assets, release preparation
verifies their checksums and matching performance evidence, and installed tests
compare Python metadata. Raising the Python minimum to 3.11 and npm minimum to
Node 22 also affects users of those distributions solely for their CLI. Existing
CLI commands and native package identities are retained.

## Size and performance evidence

The first release retains the complete multilingual data without a fixed size
ceiling. Run `scripts/report-library-performance.mjs` from an installed consumer
(copy the script there first) to record Wasm raw, gzip, and Brotli sizes, five
fresh-process startup samples, and fifteen warm render/segment samples for ASCII
and multilingual documents. The JSON includes runtime, OS/CPU, document sizes,
package version, Wasm digest, and methodology. Timings are advisory and depend on
the machine; Node file-loading measurements do not represent browser network
startup. Release preparation requires a report matching the exact shared Wasm
payload and publishes it as a versioned JSON asset. Browser evidence records the
actual engine and version; WebKit alone does not verify Safari.

The [initial local baseline](library-baseline.json), measured on macOS ARM64 with
Node 26.12.1, records 3,303,732 raw Wasm bytes, 2,185,130 gzip bytes, and
1,878,908 Brotli bytes. Median initialization was 11.233 ms. Warm HTML render /
segment medians were 0.776 / 0.976 ms for ASCII and 5.300 / 5.391 ms for the
multilingual workload. These measurements describe the artifact digest in that
report and are not cross-machine performance guarantees.

The [verification record](library-verification.md) separates completed checks
from pending release evidence and review limits. Local verification on 2026-10-27 covered Rust 1.85.1, native macOS ARM64 Python
3.11–3.15 imports, installed Node 22.23.4/24.22.1/26.12.1 ESM/CommonJS, React 18.3.1/19.3.1 declarations
and SSR, the browser corpus in Chrome 156.0.8078.48 and WebKit 27.2, and generic
React plus Next.js production hydration in Chrome. Actual Safari, Edge, Firefox,
Windows, and other native wheel targets still require the configured CI runs
before release acceptance. Local Safari remote automation was disabled; WebKit
results are recorded separately. No release or registry publication was performed.
