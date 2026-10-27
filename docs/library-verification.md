# Library implementation verification

The approved scope is [spec #11](https://github.com/arturoaviles/bionic-reader/issues/11)
and its implementation tickets #12–#21. The review baseline is
`63a52aea16724af69601dd94b7b9d9a05d96b1a9`. This record describes local work on
2026-10-27, before CI execution or publication.

## Executed checks

- Rust 1.85.1: 299 native tests passed; seven network-dependent framework tests
  remained ignored. Core-only library/corpus tests, five Rustdoc examples,
  documentation, workspace Clippy/typechecking, fuzz workspace typechecking and
  Clippy, formatting, and completion drift checks passed.
- A packaged `.crate` was extracted outside the checkout and its public Rust
  example ran with default features disabled.
- The macOS ARM64 ABI3 wheel passed the public corpus on Python 3.11–3.15.
  The final wheel passed pip, uv pip, and uv tool installation, API/CLI use, and
  uninstall checks. Import metadata matched the compiled Cargo version.
- Installed npm APIs passed 37 shared fixtures plus validation checks with Node
  22.23.4, 24.22.1, and 26.12.1. ESM/CommonJS examples typechecked. A complete
  local npm installation exercised both its native CLI and callable API.
- React 18.3.1 and 19.3.1 passed installed runtime and declaration checks.
  Production generic SSR/hydration passed in Chrome 156.0.8078.48, including
  configuration updates, emphasis disabling, control filtering, literal markup,
  Unicode, and whitespace. The Next.js 16.4.3 Pages Router/Webpack production
  example also passed Chrome hydration checks.
- The installed browser package passed the corpus in actual Chrome
  156.0.8078.48 and Playwright WebKit 27.2. These are separate observations;
  WebKit results do not prove actual Safari behavior.
- Twenty-one release/publication/CI/SBOM regression tests passed. Distribution
  source checks, actionlint, yamllint, zizmor, mypy, Ruff, spelling, and offline
  documentation links passed. An actual CycloneDX 1.5 inventory included CLI,
  PyO3, and wasm-bindgen components with separate dependency references.
- [Performance measurements](library-baseline.json) identify the exact Wasm
  digest, compression methods, runtime, workload, and cold/warm samples.

## Standards

The parallel Standards reviewer reported three preliminary findings:

1. Example server shutdown waited indefinitely and could skip temporary-directory
   cleanup. Shutdown now has a bounded graceful wait and forced termination,
   handles an exited process, and removes its isolated consumer in `finally`.
2. The original native CLI SBOM omitted the new embedded engines. The bundle
   inventory now includes CLI, Python-extension, and wasm32 dependency graphs;
   a regression checks component and reference preservation.
3. Registry repacking silently selected the last target's API manifest. It now
   compares shared manifests across targets and rejects differing exports,
   runtime engines, or React peer requirements. Negative tests first reproduced
   these failures and now pass.

## Spec

The parallel Spec reviewer independently identified the missing Python-extension
SBOM graph. The aggregate inventory described above addresses that finding.

Both review agents stopped at a service usage limit before completing their
reports. Their findings were fixed and a local failure/resource/edge-case audit
continued, but the two independent reviews are incomplete. A subsequent complete
review remains appropriate before release acceptance.

Preliminary findings: Standards 3 (all addressed), Spec 1 (addressed). The review
completion limit and the unexecuted acceptance matrix remain open.

## Pending release evidence

The configured CI matrix must still run against the committed candidate for
Linux/macOS native targets beyond this host, GNU/musl wheel imports, Node APIs
on Linux/Windows, actual Edge/Firefox/Safari, framework-loader tests, and existing
coverage/security/provenance gates. Local Firefox automation could not start
under the host's macOS app-data restrictions; Safari remote automation was
disabled and no personal browser settings were changed. No successful result is
inferred from those setup failures.

The source changes and host artifacts are ready for that validation. They are
not a verified multi-platform release. Registry uploads, release publication,
credential changes, and a Git push were not performed.
