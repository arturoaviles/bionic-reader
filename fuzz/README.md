# Byte-stream fuzzing

The `byte_stream` libFuzzer target checks byte preservation when emphasis is
disabled, equivalence between whole-input and incremental transformation,
chunk-independent safe-control filtering, and robustness of Markdown document
conversion for valid UTF-8. The property suite in `tests/properties.rs`
separately checks grapheme boundaries, content preservation, repeat-safe
emphasis, and chunking invariants.

Input bytes select valid fixation fractions, frequencies, minimum word lengths,
and emphasis attributes, so the streaming invariants also exercise settings
beyond the default profile.

Install the same pinned nightly and cargo-fuzz version used in CI:

```sh
rustup toolchain install nightly-2026-10-22 --profile minimal
cargo +nightly-2026-10-22 install cargo-fuzz --version 0.13.2 --locked
```

From the repository root, run a bounded fuzz session:

```sh
cargo +nightly-2026-10-22 fuzz run byte_stream -- \
  -max_total_time=60 -max_len=131072 -timeout=10
```

CI seeds the corpus with multilingual text, Markdown, terminal controls,
malformed bytes, and oversized tokens before running this target. Cargo tests
run the property suite separately.

Reproduce a saved failure by passing its artifact path as the corpus:

```sh
cargo +nightly-2026-10-22 fuzz run byte_stream path/to/artifact
```

Generated corpora, crash artifacts, and build outputs are ignored by Git.
After fixing a finding, keep a minimized regression as an ordinary test.
