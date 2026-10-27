import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { cp, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { createServer } from 'node:net';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const example = process.argv[2];
const candidate = resolve(process.argv[3]);
if (!['browser', 'react', 'next'].includes(example)) throw new Error('Choose browser, react, or next');
if (!process.env.npm_execpath) throw new Error('Run this check through npm run check:example');
const probe = createServer();
probe.listen(0, '127.0.0.1');
await once(probe, 'listening');
const port = probe.address().port;
await new Promise((done) => probe.close(done));
const temporary = await mkdtemp(join(tmpdir(), 'bionic-reader-example-'));
let server;
let serverError;

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: temporary, stdio: 'inherit', ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed with status ${result.status}`);
}

try {
  await cp(join(root, 'examples', example), temporary, { recursive: true });
  const npm = (args, options) => run(process.execPath, [process.env.npm_execpath, ...args], options);
  npm(['install', '--ignore-scripts', '--no-audit', '--no-fund', candidate]);
  if (process.env.BIONIC_REACT_VERSION && example === 'react') {
    const version = process.env.BIONIC_REACT_VERSION;
    npm(['install', '--ignore-scripts', '--no-audit', '--no-fund', '--no-save', `react@${version}`, `react-dom@${version}`]);
  }
  npm(['run', 'build'], { env: { ...process.env, NEXT_TELEMETRY_DISABLED: '1' } });
  const args = example === 'browser'
    ? [join(temporary, 'node_modules/vite/bin/vite.js'), 'preview', '--host', '127.0.0.1', '--port', String(port), '--strictPort']
    : example === 'react'
      ? [join(temporary, 'server.mjs')]
      : [join(temporary, 'node_modules/next/dist/bin/next'), 'start', '--hostname', '127.0.0.1', '--port', String(port)];
  server = spawn(process.execPath, args, { cwd: temporary, stdio: 'inherit', detached: process.platform !== 'win32', windowsHide: true, env: { ...process.env, BIONIC_EXAMPLE_PORT: String(port) } });
  server.on('error', (error) => { serverError = error; });
  const started = Date.now();
  const url = `http://127.0.0.1:${port}`;
  for (;;) {
    if (serverError) throw serverError;
    if (server.exitCode !== null) throw new Error('Example server exited before readiness');
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(1000) });
      const ok = response.ok;
      await response.body?.cancel();
      if (ok) break;
    } catch (error) {
      if (Date.now() - started > 60000) throw new Error(`Example did not become ready: ${error.message}`);
    }
    if (Date.now() - started > 60000) throw new Error('Example did not become ready');
    await new Promise((done) => setTimeout(done, 100));
  }
  if (process.env.BIONIC_BROWSER === 'safari') {
    run('python3', [join(root, 'scripts/check-safari-library.py'), url, join(root, 'tests/fixtures/library.json')]);
  } else {
    const check = example === 'browser' ? 'check-browser-library.mjs' : 'check-react-hydration.mjs';
    run(process.execPath, [join(root, 'scripts', check), url, join(root, 'tests/fixtures/library.json')]);
  }
} finally {
  try {
    if (server?.pid && server.exitCode === null) {
      const closed = once(server, 'close');
      const terminate = (signal) => {
        if (process.platform === 'win32') {
          const result = spawnSync('taskkill', ['/pid', String(server.pid), '/T', '/F'], { timeout: 5000 });
          if (result.error) throw result.error;
        } else {
          try { process.kill(-server.pid, signal); }
          catch (error) { if (error.code !== 'ESRCH') throw error; }
        }
      };
      terminate('SIGTERM');
      const stopped = await Promise.race([closed.then(() => true), delay(3000, false, { ref: false })]);
      if (!stopped) {
        terminate('SIGKILL');
        if (!await Promise.race([closed.then(() => true), delay(5000, false, { ref: false })])) {
          throw new Error('Example server did not exit after forced shutdown');
        }
      }
    }
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}
