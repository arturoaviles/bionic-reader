import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createReader } from '@arturoaviles/bionic-reader';
import { BionicReaderProvider, BionicText } from '@arturoaviles/bionic-reader/react';

const reader = await createReader();
const present = (props) => createElement(BionicReaderProvider, { reader }, createElement(BionicText, props));
const html = renderToStaticMarkup(present({ text: 'Hello world', id: 'reading' }));
assert.match(html, /white-space:pre-wrap/);
assert.match(html, /font-weight:700/);
assert.match(html, /id="reading"/);
assert.equal(html, '<span id="reading" style="white-space:pre-wrap"><span style="font-weight:700">Hel</span>lo <span style="font-weight:700">wor</span>ld</span>');
assert.throws(() => renderToStaticMarkup(createElement(BionicText, { text: 'Hello' })), { code: 'initialization_failed' });
assert.throws(() => renderToStaticMarkup(createElement(BionicReaderProvider, { reader: Promise.resolve(reader) })), { code: 'initialization_failed' });
assert.throws(() => renderToStaticMarkup(present({ text: '', dangerouslySetInnerHTML: { __html: '<script>bad</script>' } })), { code: 'invalid_input' });
for (const [text, escaped] of [
  ['<script> Hello\nworld', '&lt;script&gt; Hello\nworld'],
  ['<SCRIPT>UPPER</SCRIPT>', '&lt;SCRIPT&gt;UPPER&lt;/SCRIPT&gt;'],
  ['<ScRiPt>mixed</ScRiPt>', '&lt;ScRiPt&gt;mixed&lt;/ScRiPt&gt;'],
]) {
  const literal = renderToStaticMarkup(present({ text, options: { enabled: false } }));
  assert.equal(literal, `<span style="white-space:pre-wrap">${escaped}</span>`);
}
const custom = renderToStaticMarkup(present({ text: 'Hello', emphasisClassName: 'prefix', emphasisStyle: { color: 'red' } }));
assert.match(custom, /class="prefix"/);
assert.match(custom, /color:red/);
const large = 'hello '.repeat(80000);
const largeMarkup = renderToStaticMarkup(present({ text: large }));
const expectedWord = '<span style="font-weight:700">hel</span>lo ';
assert.equal(largeMarkup, `<span style="white-space:pre-wrap">${expectedWord.repeat(80000)}</span>`);
console.log('Installed React component SSR, literal text, styling, and readiness checks passed.');
