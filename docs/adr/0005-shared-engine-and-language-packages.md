# One Rust engine across language packages

Keep one Rust segmentation and transformation implementation, with native Python
bindings and WebAssembly bindings for JavaScript. React renders shared text
segments through a thin adapter. Separate language ports would simplify some
runtime integration but require ongoing synchronization of Unicode boundaries,
Markdown handling, safety policies, and emphasis behavior. Accept native-wheel
and WebAssembly initialization work to keep behavior consistent instead.

Extend the existing Python and npm package identities with importable APIs while
retaining their CLI command. Expose React through a separate npm entry point;
consumers of the core JavaScript API must not need React. Cargo remains the
version authority across packages, extending ADR 0003's distribution contract.

The portable transformation engine must be separable from terminal interaction
and other operating-system services. Choosing WebAssembly does not prove the
current dependency graph portable without changes.

JavaScript exposes an asynchronous createReader() factory that loads bundled
WebAssembly once and returns synchronous render() and segments() methods. React
receives an initialized reader through a provider. Initialize before server
rendering and client hydration, with equivalent settings on both sides. Missing
initialization produces a clear error; component rendering does not start
downloads, silently switch output, or manage loading through Suspense. This
makes engine readiness an explicit application responsibility and preserves
deterministic server and client output.

Require Python 3.11+ and Node 22+, and support React 18.3 and 19. Retain the Rust
library's 1.85.1 minimum; binding build tools may use a separate newer toolchain.
Raising the existing Python/Node metadata minimums is a documented compatibility
change, including for users who install those packages only for the CLI. These
runtime baselines favor maintained releases over retaining end-of-life runtime
support through older binding dependencies. The lifecycle references are the
[Python support table](https://devguide.python.org/versions/) and
[Node release schedule](https://github.com/nodejs/Release).

Initial native Python/Rust support retains the existing macOS/Linux targets.
The JavaScript API also supports Windows, without promising native Windows CLI
or Python support. Browser support covers current Chrome, Edge, Firefox, and
Safari, with exact tested versions recorded at release. Validate each advertised
target before claiming support. Provide TypeScript declarations, Node ESM and
CommonJS entry points, and installed-package examples for Vite and Next.js SSR,
including React hydration checks.

Retain full multilingual behavior in the first release without a fixed artifact
size ceiling. Publish compressed WebAssembly size and cold-start/warm-render
benchmarks as the initial baseline; do not silently remove language support to
reduce size. Whole-document processing retains documented engine limits,
including the existing 8 MiB in-memory Markdown input limit.

Release acceptance requires a shared language-neutral fixture corpus consumed
by Rust, Python, and JavaScript, installed-package tests, and runnable Rust,
Python, Node, browser, and React examples. Verify matching text segments and
formatted output across languages, including multilingual text, whitespace,
escaping, invalid options, and consistent server rendering/client hydration.
Build and package these additions through the existing verified-artifact release
process; registry publication retains ADR 0003's separate channel requirements.
