import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { createReader } from '@arturoaviles/bionic-reader';

const reader = await createReader();
assert.equal(reader.render('Hello world', { format: 'html' }),
  '<strong>Hel</strong>lo <strong>wor</strong>ld');
const corpus = JSON.parse(await readFile(process.argv[2], 'utf8'));
for (const item of corpus.render) {
  const operation = () => reader.render(item.text, { ...item.options, format: item.format });
  if (item.error) {
    assert.throws(operation, { name: 'BionicError', code: item.error }, item.name);
  } else {
    assert.equal(operation(), item.output, item.name);
  }
}
for (const item of corpus.segments) {
  assert.deepEqual(reader.segments(item.text, item.options), item.output, item.name);
}
for (const text of ['\ud800', '\udc00', 'valid\ud800text']) {
  assert.throws(() => reader.render(text, { format: 'html' }), { code: 'invalid_input' });
}
for (const fraction of [NaN, Infinity, -Infinity]) {
  assert.throws(() => reader.render('Hello', { format: 'html', fraction }), { code: 'invalid_input' });
}
assert.throws(() => reader.render('Hello', { format: 'html', fraction: () => 0.5 }), { code: 'invalid_input' });
assert.throws(() => reader.render('Hello'), { code: 'invalid_input' });
assert.throws(() => reader.render('a'.repeat(8 * 1024 * 1024 + 1), {
  format: 'html', input_format: 'markdown',
}), { code: 'invalid_input' });
const configured = reader.parseConfig("profile = 'strong'\nfraction = 0.25\n");
assert.equal(reader.render('reading', { ...configured, format: 'html' }), '<strong>re</strong>ading');
process.env.NO_COLOR = '1';
process.env.TERM = 'dumb';
assert.equal(reader.render('Hello', { format: 'html' }), '<strong>Hel</strong>lo');
const fromRequire = createRequire(import.meta.url)('@arturoaviles/bionic-reader');
const commonJS = await fromRequire.createReader();
assert.deepEqual(commonJS.segments('Hello'), reader.segments('Hello'));
const another = await createReader();
assert.deepEqual(another.segments('Hello world', { every: 2 }), reader.segments('Hello world', { every: 2 }));
console.log(`Installed Node ESM/CommonJS: ${corpus.render.length + corpus.segments.length} fixtures and validation checks passed.`);
