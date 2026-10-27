#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
output=${1:-"$repo_root/dist/library"}
binding_cli=${WASM_BINDGEN:-wasm-bindgen}
[[ $("$binding_cli" --version) == 'wasm-bindgen 0.2.129' ]] || {
  printf 'WebAssembly packaging requires wasm-bindgen 0.2.129\n' >&2
  exit 69
}
cargo build --locked --release --target wasm32-unknown-unknown --package bionic-reader-wasm
mkdir -p "$output/lib/wasm"
cp -R packaging/npm/lib/. "$output/lib/"
"$binding_cli" --target web --out-name engine --out-dir "$output/lib/wasm" \
  "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/bionic_reader_wasm.wasm"
cargo metadata --locked --no-deps --format-version 1 > "$output/cargo-metadata.json"
python3 - "$output" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
metadata = json.loads((output / "cargo-metadata.json").read_text())
version = next(p["version"] for p in metadata["packages"] if p["name"] == "bionic-reader")
wasm = (output / "lib/wasm/engine_bg.wasm").read_bytes()
identity = {"type": "module", "version": version, "wasm_sha256": hashlib.sha256(wasm).hexdigest()}
(output / "lib/package.json").write_text(json.dumps(identity, indent=2) + "\n")
(output / "lib/version.js").write_text("export const VERSION = " + json.dumps(version) + ";\n")
(output / "cargo-metadata.json").unlink()
PY
