import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';

const url = process.argv[2];
const corpus = JSON.parse(await readFile(process.argv[3], 'utf8'));
const selected = process.env.BIONIC_BROWSER ?? 'chromium';
const browserType = { chromium, firefox, webkit }[selected];
assert.ok(browserType, `Unknown test browser: ${selected}`);
const browser = await browserType.launch({
  headless: true,
  ...(process.env.BIONIC_BROWSER_CHANNEL ? { channel: process.env.BIONIC_BROWSER_CHANNEL } : {}),
});
try {
  const page = await browser.newPage();
  page.on('pageerror', (error) => console.error(error));
  await page.goto(url);
  await page.locator('#render').waitFor({ state: 'visible' });
  await page.waitForFunction(() => !document.querySelector('#render').disabled);
  const actual = await page.evaluate((cases) => {
    const reader = window.bionicReader;
    return {
      render: cases.render.map((item) => {
        try { return { output: reader.render(item.text, { ...item.options, format: item.format }) }; }
        catch (error) { return { error: error.code }; }
      }),
      segments: cases.segments.map((item) => reader.segments(item.text, item.options)),
      invalidUnicode: (() => {
        try { reader.render('\ud800', { format: 'html' }); return false; }
        catch (error) { return error.code; }
      })(),
    };
  }, corpus);
  for (const [index, item] of corpus.render.entries()) {
    assert.deepEqual(actual.render[index], item.error ? { error: item.error } : { output: item.output }, item.name);
  }
  for (const [index, item] of corpus.segments.entries()) {
    assert.deepEqual(actual.segments[index], item.output, item.name);
  }
  assert.equal(actual.invalidUnicode, 'invalid_input');
  await page.locator('#text').fill('<script> Hello\nworld');
  await page.locator('#options').fill('{}');
  await page.locator('#format').selectOption('html');
  await page.locator('#render').click();
  assert.equal(await page.locator('#preview').textContent(), '<script> Hello\nworld');
  assert.equal(await page.locator('#preview script').count(), 0);
  const evidence = { browser: selected, channel: process.env.BIONIC_BROWSER_CHANNEL, version: browser.version(), fixtures: corpus.render.length + corpus.segments.length };
  console.log(JSON.stringify(evidence));
  if (process.env.BIONIC_EXAMPLE_EVIDENCE) await writeFile(process.env.BIONIC_EXAMPLE_EVIDENCE, JSON.stringify(evidence) + '\n');
} finally {
  await browser.close();
}
