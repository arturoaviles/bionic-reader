# Rust style

This project follows [Google's Rust style guide](https://google.github.io/styleguide/rustguide.html), reviewed on 2026-10-23, together with the [Rust style guide](https://doc.rust-lang.org/style-guide/) and [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/). Apply the Google guide's application-code guidance: this crate implements one CLI and its terminal engine. Keep changes direct and preserve the command-line and byte-stream contracts in [design.md](design.md).

## Code and API conventions

- Keep the crate root declarative. Name a leaf module for its primary abstraction: `settings` contains `Settings`, and `transformer` contains `Transformer`. Session and shell modules contain related operations rather than one primary type.
- Import items from one module per `use` statement. Import types by name, qualify external free functions with their module, and scope trait imports to the function when they are used there alone. Qualified names such as `io::Error` and POSIX `libc` types are acceptable where they clarify a family of related types. Inline tests may use `super::*`.
- Use stable Rust and the checked-in rustfmt configuration: edition 2024 and a 100-column limit. Keep hexadecimal literals lowercase and escape invisible characters in literals. Do not skip formatting for personal preferences.
- Prefer borrowing and unique ownership. Use flexible borrowed inputs such as `&str`, `&Path`, and slices. Avoid deep clones that only work around a borrow. Do not introduce generic `Into` parameters, traits, or macros merely to organize application code.
- Make boolean arguments understandable from their names or a call-site comment; use an enum when the distinction needs a domain name. Prefer named fields for values that travel beyond a small local context. Use standard trait names, derives, and `Default` for options structs.
- Document public contracts, configuration precedence, byte preservation, errors, and any input-dependent panics. Include useful examples. Mark pure functions and values whose results must be consumed with `#[must_use]`. Match optional data directly instead of checking and then unwrapping it. Use `expect` only for invariants, with a message explaining the assumption; tests also need useful failure messages.
- Keep recoverable failures in `Result`, including I/O and configuration failures. Never recover access to a poisoned mutex's contents. Keep unsafe operations behind safe functions, with a `SAFETY:` comment and an explanation of why a safe equivalent is unavailable.
- Use descriptive names, explicit types where inference obscures an operation, and straightforward loops when iterator chains obscure control flow. Bind iterator chains before using them in a `for` header. Add assertion context when it explains a failure rather than repeating its condition.
- Avoid unstable features, hidden dependency APIs, work before `main`, custom operator meanings, and `catch_unwind`. Add expected API pairs together, such as `len` and `is_empty`, when introducing them.

## Application-specific decisions

Google's guide explicitly permits contextual choices. The following apply here:

- Chromium build rules, flat GN source layouts, `chromium::import!`, Chromium task sequences, Fuchsia mutexes, and their test frameworks apply to those platforms. This project uses Cargo, standard Rust threads and mutexes, and `#[test]`. It retains the PTY architecture required by [ADR 0001](adr/0001-session-wide-terminal-transformation.md).
- The session's worker threads and main loop share runtime settings and stop flags through `Arc`. Shared ownership keeps those lifetimes explicit without unsafe borrowed thread state. Exclusive data remains locally owned.
- Byte-processing functions construct owned output buffers, so returning `Vec<u8>` is intentional. Callers immediately write or inspect those bytes. Application helpers may accept slices when no iterator flexibility is needed.
- `Style::{Auto, Always, Never}` is the complete, fixed emission policy and remains exhaustive. `Settings` and `Overrides` intentionally support struct update syntax with `Default`; they use the guide's explicit exception to `#[non_exhaustive]` for options structs.
- Clap and Serde derives implement their established command-line and configuration contracts. There is no project-specific macro language. The completion generator's replacement of a generated Bash registration name operates on generated output, rather than substituting values into a hand-written format string.
- Stable Rust 1.85.1 remains the minimum compiler. Nightly-only rustfmt options and compiler lints are not enabled. Review checks import grouping, literal case, and other guidance that stable rustfmt cannot enforce.

## Enforcement

The Cargo manifest denies missing public documentation and debug implementations, ignored results, unsafe operations outside explicit unsafe blocks, undocumented unsafe blocks, bare unwraps, glob imports, missing error/panic documentation, and missing `must_use` annotations on public candidates. Clippy's normal correctness, API, and ownership checks run with all warnings denied.

CI also checks formatting, builds documentation with warnings denied, executes documentation examples, and runs unit, CLI, terminal, installation, and framework tests. See [CONTRIBUTING.md](../CONTRIBUTING.md) for commands. These tools cover mechanical rules; review must still assess ownership, naming, API design, unsafe justification, and readability.
