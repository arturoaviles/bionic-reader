# JavaScript, TypeScript, browser, and React examples

Install the npm package into your application. Node.js usage requires Node 22 or
newer. These examples cover the application API introduced in 0.2.0.

```sh
npm install @arturoaviles/bionic-reader
```

## Initialize once with ESM or CommonJS

Save ESM examples as `.mjs`, or use a project with `"type": "module"`. The
remaining core examples share this initialized reader:

```js
import { createReader } from '@arturoaviles/bionic-reader';

const reader = await createReader();
```

Initialization loads the bundled WebAssembly engine. After it resolves,
`render()`, `segments()`, and `parseConfig()` are synchronous. Initialization is
shared across factory calls; individual documents have independent frequency
state. Transformation runs locally without a service or a separate CLI process.

CommonJS uses an async function instead of top-level `await`:

```js
const { createReader } = require('@arturoaviles/bionic-reader');

async function main() {
  const reader = await createReader();
  console.log(reader.render('Hello world', { format: 'html' }));
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
```

## Render HTML, Markdown, ANSI, or plain text

```js
console.log(reader.render('Hello world', { format: 'html' }));
// <strong>Hel</strong>lo <strong>wor</strong>ld

console.log(reader.render('Hello world', { format: 'markdown' }));
// **Hel**lo **wor**ld

console.log(reader.render('Hello world', { format: 'ansi' }));
// Terminal escape sequences emphasize word beginnings.

console.log(reader.render('Hello world', { format: 'plain' }));
// Hello world
```

`format` is required for `render()`. Plain output adds no emphasis and strips
terminal controls. HTML output is a fragment; text input is escaped rather than
interpreted as HTML.

## Choose profiles or explicit settings

```js
const text = 'Reading complex documentation becomes easier.';
for (const profile of ['subtle', 'balanced', 'strong']) {
  console.log(reader.render(text, { format: 'html', profile }));
}

const html = reader.render(text, {
  format: 'html',
  profile: 'strong',
  fraction: 0.4,
  every: 2,
  min_word_length: 4,
});
```

| Profile | Fraction | Every | Minimum word length |
| --- | ---: | ---: | ---: |
| `subtle` | 0.3 | 2 | 4 |
| `balanced` (default) | 0.5 | 1 | 2 |
| `strong` | 0.7 | 1 | 2 |

Explicit numeric settings override profiles. `every: 2` selects the first
eligible word, then every second eligible word. Length counts letter graphemes.
Options use snake_case, including in JavaScript.

## Apply rules according to word length

```js
const html = reader.render('Read complex documentation', {
  format: 'html',
  fraction: 0.7,
  length_rules: [
    { max_length: 4, fraction: 0.25 },
    { max_length: 8, fraction: 0.5 },
  ],
});

console.log(reader.render('Hello', { format: 'html', fraction: 0 })); // Hello
console.log(reader.render('Hello', { format: 'html', fraction: 1 })); // <strong>Hello</strong>
```

Thresholds are inclusive and must increase. The first matching threshold applies;
words longer than eight letters use the fallback fraction, 0.7.

## Use bold, underline, or reverse

```js
for (const emphasis of ['bold', 'underline', 'reverse']) {
  console.log(reader.render('Hello', { format: 'html', emphasis }));
  console.log(reader.render('Hello', { format: 'ansi', emphasis }));
}
```

HTML uses `<strong>`, `<u>`, or `<span class="bionic-reverse">`. For reverse
HTML, supply CSS such as `.bionic-reverse { color: white; background: black; }`.
Markdown always emits `**strong**`, regardless of `emphasis`.

## Transform Markdown and protect code

````js
const source = '# Hello `world`\n\n'
  + 'Read [the guide](https://example.com).\n\n'
  + '```js\nconsole.log("Hello world");\n```\n';

for (const format of ['html', 'markdown', 'ansi', 'plain']) {
  console.log(reader.render(source, { input_format: 'markdown', format }));
}

console.log(reader.render('# Hello `world`\n', {
  input_format: 'markdown', format: 'html',
}));
// <h1><strong>Hel</strong>lo <code>world</code></h1>

const includingCode = reader.render('Read `hello` now.', {
  input_format: 'markdown', format: 'html', skip_code: false,
});
````

Code and link destinations are protected by default. HTML renders Markdown
structure, escapes raw HTML, and removes unsafe link schemes. ANSI and Markdown
output retain Markdown source syntax. Plain output preserves that syntax too;
it does not extract rendered prose. Setting `skip_code: false` is supported for
HTML and ANSI, but rejected for Markdown-to-Markdown conversion.

## Disable added emphasis while keeping conversion

```js
console.log(reader.render('# Hello `world`\n', {
  input_format: 'markdown', format: 'html', enabled: false,
}));
// <h1>Hello <code>world</code></h1>

console.log(reader.render('<tag> & text', { format: 'html', enabled: false }));
// &lt;tag&gt; &amp; text
```

Control filtering and format conversion still apply.

## Get text segments, Unicode text, or JSON

```js
const parts = reader.segments('Hello world');
console.log(parts);
// [
//   { text: 'Hel', emphasized: true },
//   { text: 'lo ', emphasized: false },
//   { text: 'wor', emphasized: true },
//   { text: 'ld', emphasized: false },
// ]

const visible = parts.map((part) => part.text).join('');
const json = JSON.stringify(parts, null, 2);
const empty = reader.segments(''); // []
const markdownParts = reader.segments('# Hello `world`\n', { input_format: 'markdown' });

const multilingual = 'Español: acción. 日本語。ภาษาไทย\nCafe\u0301 👩🏽‍💻 123';
console.assert(reader.segments(multilingual).map((part) => part.text).join('') === multilingual);
```

Segments contain literal text and an emphasis flag, without generated markup,
terminal controls, or string offsets. Markdown segments retain source syntax and
protect code; they are not rendered Markdown prose. Unicode segmentation needs
no language option. Digits and emoji receive no added emphasis. Text preservation
is subject to terminal-control filtering.

## Parse explicit TOML preferences

```js
const options = reader.parseConfig(`
profile = "strong"
fraction = 0.25
every = 1
min_word_length = 2
emphasis = "bold"
style = "always"
`);

const html = reader.render('Reading documentation', { ...options, format: 'html' });
const overridden = reader.render('Reading documentation', {
  ...options, fraction: 0.6, format: 'html',
});
```

`parseConfig()` takes TOML contents and performs no I/O. `style = "never"`
disables added emphasis; `auto` enables it deterministically. Session-only fields
are ignored. The CLI output-format preference does not select API output format;
pass `format` in each render call.

## Read files, write exports, and process batches in Node

Create `reading.toml` with the configuration above and an `article.md` document:

```js
import { readFile, writeFile } from 'node:fs/promises';

const options = reader.parseConfig(await readFile('reading.toml', 'utf8'));
const source = await readFile('article.md', 'utf8');
const settings = { ...options, input_format: 'markdown' };
const body = reader.render(source, { ...settings, format: 'html' });
const page = '<!doctype html><html lang="en"><meta charset="utf-8">'
  + `<title>Reading</title><body>${body}</body></html>`;

await writeFile('article.html', page, 'utf8');
await writeFile('article-bionic.md', reader.render(source, {
  ...settings, format: 'markdown',
}), 'utf8');
await writeFile('segments.json', JSON.stringify(reader.segments(source, settings), null, 2), 'utf8');

const documents = ['First document', 'Second document', 'Third document'];
const pages = documents.map((text) => reader.render(text, { format: 'html' }));
```

Each document starts a fresh emphasis-frequency sequence. Only initialization
and file I/O are asynchronous here; transformation calls are synchronous.

## Handle initialization and input errors

```js
try {
  const initialized = await createReader();
  try {
    initialized.render('Hello', { format: 'html', fraction: 1.5 });
  } catch (error) {
    if (error.code !== 'invalid_input') throw error;
    console.error(error.message);
  }
} catch (error) {
  if (error.code !== 'initialization_failed') throw error;
  console.error('Could not initialize the engine:', error.message);
}
```

Unknown options, invalid formats or types, non-finite fractions, unpaired
surrogates, and unsupported combinations fail rather than being silently
accepted. Error objects have a stable `code`; there is no exported error-class
constructor to import.

## Use TypeScript declarations

The package includes its own declarations:

```ts
import { createReader, type Options, type Reader } from '@arturoaviles/bionic-reader';

function renderArticle(reader: Reader, text: string, options: Options = {}): string {
  return reader.render(text, { ...options, format: 'html' });
}

const reader = await createReader();
const html = renderArticle(reader, 'Hello world', { profile: 'balanced' });
```

## Render in a browser with DOM nodes

In a browser application using a bundler, the same package import selects the
browser loader. The bundler must serve the bundled `.wasm` asset, and the page's
policy must permit fetching and compiling it. Initialization fetches the engine
asset once; text transformation itself needs no backend. See the
[browser application](../../examples/browser) for a complete consumer.

Given an element `<div id="reading"></div>`, this browser entry point builds
styled spans from literal text:

```js
import { createReader } from '@arturoaviles/bionic-reader';

const reader = await createReader();
const container = document.querySelector('#reading');
container.style.whiteSpace = 'pre-wrap';
const fragment = document.createDocumentFragment();

for (const part of reader.segments('Hello <world>\nAnother line')) {
  const span = document.createElement('span');
  span.textContent = part.text;
  if (part.emphasized) span.style.fontWeight = '600';
  fragment.append(span);
}
container.replaceChildren(fragment);
```

Using `textContent` keeps literal markup literal. Use `reader.render()` instead
when your application needs exported HTML or Markdown strings.

## Render plain text with React

Install React 18.3 or 19 and React DOM alongside the package if your application
does not already provide them:

```sh
npm install react react-dom
```

In a JSX-enabled browser application, initialize before rendering:

```jsx
import { createRoot } from 'react-dom/client';
import { createReader } from '@arturoaviles/bionic-reader';
import { BionicReaderProvider, BionicText } from '@arturoaviles/bionic-reader/react';

const reader = await createReader();
createRoot(document.getElementById('root')).render(
  <BionicReaderProvider reader={reader}>
    <BionicText
      text={'Hello world\nAnother line'}
      options={{ profile: 'balanced', fraction: 0.5, every: 1 }}
      className="reading"
      emphasisClassName="fixation"
      emphasisStyle={{ fontWeight: 600 }}
      style={{ lineHeight: 1.7 }}
    />
  </BionicReaderProvider>,
);
```

`BionicText` preserves whitespace with `white-space: pre-wrap`. Prefixes default
to font weight 700; customize them with `emphasisClassName` or `emphasisStyle`.
Normal span attributes and `style` apply to the outer span. Supported `options`
are `enabled`, `profile`, `fraction`, `every`, `min_word_length`, and
`length_rules`. For example, `options={{ enabled: false }}` hides added emphasis.

The component accepts plain text, including text that happens to contain markup.
It renders literal text nodes and styled spans. Markdown interpretation, raw
HTML input, arbitrary children, and injected HTML are outside its contract.
Reader initialization and loading/error UI belong to the application; an absent
provider or uninitialized reader fails clearly.

## Server-render and hydrate React

Server rendering can use ordinary JavaScript without JSX:

```js
import { createElement } from 'react';
import { renderToString } from 'react-dom/server';
import { createReader } from '@arturoaviles/bionic-reader';
import { BionicReaderProvider, BionicText } from '@arturoaviles/bionic-reader/react';

const reader = await createReader();
const view = createElement(BionicReaderProvider, { reader },
  createElement(BionicText, { text: 'Hello world', options: { profile: 'balanced' } }),
);
const markup = renderToString(view);
// Include markup inside <div id="root"> in the HTML response.
```

Initialize independently in the browser, then hydrate that element with the
same text and options:

```js
import { createElement } from 'react';
import { hydrateRoot } from 'react-dom/client';
import { createReader } from '@arturoaviles/bionic-reader';
import { BionicReaderProvider, BionicText } from '@arturoaviles/bionic-reader/react';

const reader = await createReader();
hydrateRoot(document.getElementById('root'),
  createElement(BionicReaderProvider, { reader },
    createElement(BionicText, { text: 'Hello world', options: { profile: 'balanced' } }),
  ),
);
```

See the [React application](../../examples/react) and
[Next.js application](../../examples/next) for complete consumers. The Next.js
example uses Pages Router and Webpack; other setups need their own integration
check.

## Options and limits

| Option | Values |
| --- | --- |
| `format` | Required by `render()`: `ansi`, `plain`, `markdown`, `html` |
| `input_format` | `text` (default), `markdown`, `terminal` |
| `enabled` | Boolean, default `true` |
| `profile` | `subtle`, `balanced`, `strong` |
| `fraction` | Finite number in `[0, 1]` |
| `every`, `min_word_length` | Positive integers |
| `length_rules` | Increasing inclusive `{max_length, fraction}` thresholds |
| `emphasis` | `bold` (default), `underline`, `reverse` |
| `controls` | `safe` (default), `preserve` |
| `skip_code` | Boolean, default `true` |

The general API also supports existing terminal controls:

```js
const colored = '\x1b[31mHello world\x1b[0m';
const safe = reader.render(colored, { format: 'ansi', controls: 'safe' });
const plain = reader.render(colored, { format: 'plain' }); // Hello world
// Explicit preservation for trusted terminal input only:
const trusted = reader.render(colored, { format: 'ansi', controls: 'preserve' });
const terminal = reader.render(colored, { format: 'ansi', input_format: 'terminal' });
```

Safe ANSI output retains recognized styling and safe terminal hyperlinks while
removing active controls. Plain output and segments always strip terminal
controls, including when preservation is requested. React always uses safe,
plain-text segments.

JavaScript accepts whole Unicode strings, not raw bytes or streams, and exposes
no custom segmenter callbacks. Markdown input is limited to 8 MiB of UTF-8;
other inputs are bounded by runtime memory. Word counts cannot exceed
4,294,967,295. Calls do not discover CLI configuration or inspect terminal state,
`NO_COLOR`, or `TERM`. Interactive terminal reading and shell reading sessions
remain CLI features. See the [library guide](../LIBRARY.md) for the shared
contract and [BUILD.md](../BUILD.md) for local package builds.
