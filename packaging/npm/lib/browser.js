import { createInitializedReader } from './runtime.js';

export function createReader() {
  return createInitializedReader(async () => {
    const response = await fetch(new URL('./wasm/engine_bg.wasm', import.meta.url));
    if (!response.ok) throw new Error(`Unable to load bionic reader engine: HTTP ${response.status}`);
    return response.arrayBuffer();
  });
}
