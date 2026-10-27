import { readFile } from 'node:fs/promises';
import { createInitializedReader } from './runtime.js';

export function createReader() {
  return createInitializedReader(() => readFile(new URL('./wasm/engine_bg.wasm', import.meta.url)));
}
