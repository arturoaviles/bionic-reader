#!/usr/bin/env node
'use strict';

const { spawn } = require('node:child_process');
const os = require('node:os');
const fs = require('node:fs');
const path = require('node:path');

function targetPackage() {
  if (process.platform === 'darwin') {
    if (process.arch === 'x64') return '@arturoaviles/bionic-reader-darwin-x64';
    if (process.arch === 'arm64') return '@arturoaviles/bionic-reader-darwin-arm64';
  }

  if (process.platform === 'linux') {
    const arch = process.arch === 'x64' ? 'x64' : process.arch === 'arm64' ? 'arm64' : null;
    if (!arch) return null;

    const report = process.report && process.report.getReport
      ? process.report.getReport()
      : null;
    if (!report || !report.header) return null;
    const libc = report.header.glibcVersionRuntime ? 'gnu' : 'musl';
    return `@arturoaviles/bionic-reader-linux-${arch}-${libc}`;
  }

  return null;
}

const nativePackage = targetPackage();
if (!nativePackage) {
  process.stderr.write(
    `bionic-reader: no native package is available for ${process.platform}/${process.arch}\n`,
  );
  process.exitCode = 1;
} else {
  let executable;
  try {
    const bundled = path.join(__dirname, 'native', 'bionic-reader');
    executable = fs.existsSync(bundled)
      ? bundled
      : require.resolve(`${nativePackage}/bin/bionic-reader`);
  } catch (error) {
    process.stderr.write(
      `bionic-reader: the optional native package ${nativePackage} is missing. ` +
      `Reinstall the package for this platform.\n`,
    );
    process.exitCode = 1;
  }

  if (executable) {
    const child = spawn(executable, process.argv.slice(2), {
      env: process.env,
      stdio: 'inherit',
    });
    const forwardedSignals = ['SIGINT', 'SIGTERM', 'SIGHUP', 'SIGQUIT'];
    const handlers = new Map();
    let spawnFailed = false;

    for (const signal of forwardedSignals) {
      const handler = () => {
        try {
          child.kill(signal);
        } catch (_) {
          // The child may have exited between the signal and this callback.
        }
      };
      handlers.set(signal, handler);
      process.on(signal, handler);
    }

    child.once('error', (error) => {
      spawnFailed = true;
      // A spawn failure emits `error` and `close`, but may never emit `exit`.
      process.exitCode = 1;
      process.stderr.write(`bionic-reader: could not start native executable: ${error.message}\n`);
    });

    child.once('close', (code, signal) => {
      for (const [name, handler] of handlers) process.removeListener(name, handler);
      if (spawnFailed) {
        process.exitCode = 1;
      } else if (signal) {
        try {
          process.kill(process.pid, signal);
        } catch (_) {
          process.exitCode = 128 + (os.constants.signals[signal] || 1);
        }
      } else {
        process.exitCode = code === null ? 1 : code;
      }
    });
  }
}
