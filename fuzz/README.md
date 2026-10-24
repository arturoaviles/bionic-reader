# Byte-stream fuzzing

Install the pinned tool with `cargo +nightly-2026-10-22 install cargo-fuzz --version 0.13.2 --locked`.
From the repository root, run:

```sh
cargo +nightly-2026-10-22 fuzz run byte_stream -- -max_total_time=60 -max_len=131072
```

The target checks malformed-byte passthrough, incremental rendering equivalence,
safe-control filtering across arbitrary chunks, and Markdown/document parser
robustness. Property tests separately check grapheme boundaries, exact content
preservation, and repeat-safe emphasis. CI runs both suites. Reproduce a crash
with `cargo +nightly-2026-10-22 fuzz run byte_stream path/to/artifact`.

Generated corpora, crash artifacts, and build outputs are ignored by Git. Commit
minimized regression cases as ordinary tests after fixing a finding.
