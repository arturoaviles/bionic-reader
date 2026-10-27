import { writeFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { chromium, firefox, webkit } from 'playwright';

const url = process.argv[2];
const initial = await (await fetch(url)).text();
assert.match(initial, /id="reading"/);
assert.match(initial, /font-weight:700/);
assert.match(initial, /Server rendered/);
const browserType = { chromium, firefox, webkit }[process.env.BIONIC_BROWSER ?? 'chromium'];
const browser = await browserType.launch({
  headless: true,
  ...(process.env.BIONIC_BROWSER_CHANNEL ? { channel: process.env.BIONIC_BROWSER_CHANNEL } : {}),
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error' && /hydrat|react|bionic|wasm/i.test(message.text())) errors.push(message.text());
  });
  await page.goto(url);
  await page.locator('#hydration[data-ready="true"]').waitFor();
  assert.equal(await page.locator('#reading').textContent(), '<script> Hello\nworld café ภาษาไทย');
  assert.equal(await page.locator('#reading script, #reading strong').count(), 0);
  assert.equal(await page.locator('#reading').evaluate((node) => getComputedStyle(node).whiteSpace), 'pre-wrap');
  if (await page.locator('#emphasis-off').count()) {
    await page.locator('#emphasis-off').click();
    assert.equal(await page.locator('#reading span').count(), 0);
    assert.equal(await page.locator('#reading').textContent(), '<script> Hello\nworld café ภาษาไทย');
    await page.locator('#emphasis-change').click();
    assert.equal(await page.locator('#reading span').first().textContent(), 'script');
    await page.locator('#control-sample').click();
    assert.equal(await page.locator('#reading').textContent(), 'Hello\tworld\ncafé ภาษาไทย');
    assert.equal(await page.locator('#reading span').first().textContent(), 'Hello');
  }
  assert.deepEqual(errors, []);
  const evidence = { browser: browser.version(), hydration: 'passed', url };
  console.log(JSON.stringify(evidence));
  if (process.env.BIONIC_EXAMPLE_EVIDENCE) await writeFile(process.env.BIONIC_EXAMPLE_EVIDENCE, JSON.stringify(evidence) + '\n');
} finally {
  await browser.close();
}
