import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { release } from 'node:os';
import { performance } from 'node:perf_hooks';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { gzipSync, brotliCompressSync, constants } from 'node:zlib';

if (process.argv[2] === '--cold') {
  const start = performance.now();
  const { createReader } = await import('@arturoaviles/bionic-reader');
  const imported = performance.now();
  const reader = await createReader();
  const initialized = performance.now();
  reader.render('Hello world', { format: 'html' });
  console.log(JSON.stringify({ import_ms: imported - start, initialize_ms: initialized - imported }));
} else {
  const wasm = await readFile(process.argv[2]);
  const cold = [];
  for (let index = 0; index < 5; index += 1) {
    const start = performance.now();
    const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--cold'], { encoding: 'utf8' });
    if (child.status !== 0) throw new Error(child.stderr || 'Cold-start measurement failed');
    cold.push({ ...JSON.parse(child.stdout), process_wall_ms: performance.now() - start });
  }
  const { createReader } = await import('@arturoaviles/bionic-reader');
  const reader = await createReader();
  const workloads = [
    { name: 'ASCII prose', unit: 'Reading documentation with balanced emphasis. ', repetitions: 256 },
    { name: 'Multilingual prose', unit: '你好世界 ภาษาไทย مرحبا café café Привет 👨‍👩‍👧 ', repetitions: 128 },
  ];
  const warm = [];
  for (const workload of workloads) {
    const input = workload.unit.repeat(workload.repetitions);
    for (const operation of ['render_html', 'segments']) {
      const run = () => operation === 'render_html' ? reader.render(input, { format: 'html' }) : reader.segments(input);
      run();
      const samples = [];
      let outputUnits = 0;
      for (let index = 0; index < 15; index += 1) {
        const start = performance.now();
        outputUnits += run().length;
        samples.push(performance.now() - start);
      }
      warm.push({ ...workload, input_utf8_bytes: Buffer.byteLength(input), operation, samples_ms: samples, output_units: outputUnits });
    }
  }
  console.log(JSON.stringify({
    package_version: process.argv[3],
    measured_at: new Date().toISOString(),
    environment: { node: process.version, platform: process.platform, architecture: process.arch, os_release: release() },
    artifact: {
      sha256: createHash('sha256').update(wasm).digest('hex'),
      raw_bytes: wasm.length,
      gzip_level_9_bytes: gzipSync(wasm, { level: 9 }).length,
      brotli_quality_11_bytes: brotliCompressSync(wasm, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length,
    },
    methodology: 'Installed package, full multilingual data, default balanced settings; five fresh Node processes for cold import/initialization, one warmup and fifteen samples per warm workload. Node local-file loading; browser network transfer is not measured.',
    cold,
    warm,
  }, null, 2));
}
