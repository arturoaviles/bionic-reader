# Go module with an embedded Rust engine

Provide `github.com/arturoaviles/bionic-reader` as a root Go module. The user
selected embedded WebAssembly and authorized implementation without further
design questions. This extends ADR 0005's single-engine decision to Go.

The alternative native CGO adapter would require platform libraries and a C
toolchain. A separate Go implementation would duplicate Unicode segmentation,
Markdown handling, and safety behavior. Embed the shared engine instead, using
wazero with no CGO or runtime downloads. Go 1.25 is the initial minimum, matching
wazero 1.12.0. Validate Linux, macOS, and Windows consumers in CI.

A private Rust WASI command adapter accepts one JSON request and emits one JSON
response. It exposes document rendering, text segments, explicit TOML parsing,
and the Cargo-owned version. This avoids a pointer-based ABI and JavaScript host
imports. A Go Reader owns the compiled module and runtime; each operation gets
an isolated WASI instance with stdin/stdout only, no mounted filesystem or network.
Calls are safe concurrently. Context cancellation closes only that instance;
Close waits for active calls and releases the reader's runtime. No CLI process
is spawned. Whole-string contracts, safe controls, profile overrides, and the
8 MiB Markdown limit remain those of ADR 0004. Streaming and custom segmenter
callbacks remain outside this Go API.

Options use pointers where zero/false differs from an omitted default. Shared
fixtures verify parity with Rust, Python, and JavaScript. Typed errors expose
invalid_input, initialization_failed, engine_failed, and closed_reader codes;
context cancellation and deadline errors retain Go's standard identities.

Commit the generated engine and its version/checksum declaration so `go get`
provides a complete module without Rust or a build hook. Rebuild from locked
Rust 1.85.1 source with canonical source-path mappings in CI and require exact
byte equality. A separate main-only job attests the engine without executing it.
Release preparation checks that attested candidate against the tagged source.
The engine is also included in the GitHub Release checksum manifest.

Go publication uses existing root `vMAJOR.MINOR.PATCH` tags and needs no upload
token, additional repository, or prefixed submodule tag. Cargo remains the version
authority. After a normal GitHub Release is published, the optional `go` channel
requests the tagged module through proxy.golang.org with sum.golang.org enabled.
It checks module origin/tag/commit, both module sums, and the attested engine
digest, then runs a fresh consumer and shared API tests with CGO disabled.
pkg.go.dev documentation discovery is asynchronous; proxy availability and a
working consumer are the publication gate. Published tags are never rewritten.
