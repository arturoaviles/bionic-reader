# Changelog

## 0.3.0 — 2026-10-27

### Added

- Go module with an embedded Rust/WebAssembly engine, requiring Go 1.25+ and
  no CGO, Rust installation, separate CLI, or runtime downloads.
- Context-aware rendering, text segments, explicit TOML preferences, typed
  errors, concurrent calls, and explicit reader resource cleanup.
- Go examples, cross-platform conformance and race checks, engine reproduction
  and provenance, vulnerability scanning, and public Go module proxy publication.

## 0.2.0 — 2026-10-27

### Added

- Deterministic Rust document rendering and ordered text segments through
  `reading::Reader`, with explicit options and formats.
- Native Python `Reader`, `render`, `segments`, and explicit configuration APIs.
- JavaScript ESM/CommonJS and browser APIs backed by bundled WebAssembly;
  initialize once with `createReader()` and transform documents synchronously.
- React `BionicReaderProvider` and `BionicText` with literal text rendering,
  whitespace preservation, CSS customization, server rendering, and hydration.
- Shared multilingual conformance fixtures, installed-package examples, portable
  engine features, aggregate dependency inventories, and Wasm performance reports.

### Changed

- Python packages require Python 3.11 or newer, including CLI-only installations.
- npm packages require Node 22 or newer, including CLI-only installations.
- CLI installation commands and existing lower-level Rust APIs remain available.
  The Rust minimum remains 1.85.1.

### Migration

Upgrade Python and Node before updating these packages. Install from the usual
registries; no separate CLI or backend service is needed for library calls.
JavaScript callers must await `createReader()` before using synchronous methods.
React applications initialize their readers before server rendering and client
hydration. Formatted API calls require an explicit output format; configuration
files are parsed only when explicitly supplied by the caller.
