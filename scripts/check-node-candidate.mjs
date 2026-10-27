import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFile, mkdtemp, rm } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

if (!process.env.npm_execpath) throw new Error('Run through npm run check:node');
const root = fileURLToPath(new URL('../', import.meta.url));
const candidate = resolve(process.argv[2]);
const temporary = await mkdtemp(join(tmpdir(), 'bionic-reader-node-'));
function run(command, args) {
  const result = spawnSync(command, args, { cwd: temporary, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed with status ${result.status}`);
}
function npm(args) { run(process.execPath, [process.env.npm_execpath, ...args]); }
try {
  npm(['install', '--ignore-scripts', '--no-audit', '--no-fund', '--omit=optional', candidate]);
  const require = createRequire(join(temporary, 'consumer.cjs'));
  assert.throws(() => require.resolve('react'), { code: 'MODULE_NOT_FOUND' });
  for (const name of ['check-node-library.mjs', 'check-react-library.mjs']) {
    await copyFile(join(root, 'scripts', name), join(temporary, name));
  }
  run(process.execPath, [join(temporary, 'check-node-library.mjs'), join(root, 'tests/fixtures/library.json')]);
  for (const name of ['reading.mts', 'reading.cts']) {
    await copyFile(join(root, 'examples/node', name), join(temporary, name));
  }
  const compiler = join(root, 'node_modules/typescript/bin/tsc');
  run(process.execPath, [compiler, '--strict', '--noEmit', '--module', 'nodenext', '--target', 'es2022', 'reading.mts', 'reading.cts']);
  if (process.env.BIONIC_REACT_VERSION) {
    const version = process.env.BIONIC_REACT_VERSION;
    const types = process.env.BIONIC_REACT_TYPES ?? '19.3.0';
    npm(['install', '--ignore-scripts', '--no-audit', '--no-fund', '--omit=optional', '--no-save', `react@${version}`, `react-dom@${version}`, `@types/react@${types}`]);
    run(process.execPath, [join(temporary, 'check-react-library.mjs')]);
    await copyFile(join(root, 'examples/react/reading.tsx'), join(temporary, 'reading.tsx'));
    run(process.execPath, [compiler, '--strict', '--noEmit', '--jsx', 'react-jsx', '--module', 'nodenext', '--target', 'es2022', 'reading.tsx']);
  }
} finally {
  await rm(temporary, { recursive: true, force: true });
}
