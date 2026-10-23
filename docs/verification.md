# Implementation verification

Validation performed on 2026-10-23. The implementation and independent standards/spec reviews used GPT-6 Luna subagents with maximum reasoning; the parent agent reviewed, corrected, and integrated the work.

## Executed checks

| Check | Result |
| --- | --- |
| Full suite on macOS ARM64, Rust 1.85.1 | 75 passed; seven network-dependent framework tests also passed separately |
| Full suite in a Debian ARM64 container, Rust 1.85.1 | 66 passed; seven framework tests run separately |
| Actual upstream framework loaders on both platforms | All seven passed: Bash-it, Oh My Bash, Oh My Zsh, Prezto, Zinit, Fisher, Oh My Fish |
| Minimum compiler | Rust 1.85.1 `cargo check --locked --all-targets` passed; Linux suite also ran on this compiler |
| Rust formatting and Clippy | Passed, with warnings denied |
| Rustdoc build and examples | Documentation built with warnings denied; the byte-transformation example passed on Rust 1.85.1 |
| ShellCheck and Bash/Zsh/fish syntax | Passed |
| Python Ruff, formatting, compilation, and strict mypy | Passed |
| Node syntax and native launcher integration | Passed |
| YAML, Homebrew formula syntax, package metadata, and generated completions | Passed |
| Gitleaks 8.30.1 | Full fetched-history scan passed; a synthetic deleted-token fixture failed as expected with its value redacted |
| Cargo audit 0.22.2 | No reported advisories in the 70 locked dependencies |
| macOS ARM64 wheel | Built and installed through pip, `uv pip`, and `uv tool`; each installed executable passed the shared CLI and PTY smoke checker |
| Local npm, Yarn 4.18.1 Plug’n’Play, and pnpm 12.14.0 installs on macOS ARM64 | Passed using candidate tarballs and the real native executable |
| Archive creation | Reproducibility and recursive integration/completion asset tests passed |
| Fresh source installation | Cargo install, installed CLI and PTY checks, and uninstall passed on macOS ARM64 and Debian ARM64 with Rust 1.85.1 |
| Cargo/crates.io candidate on macOS ARM64, Rust 1.85.1 | Verified 36-file archive, extracted-source install, installed CLI/PTY checks, uninstall, all 75 packaged-source tests, and the Rustdoc example passed; the earlier `cargo publish --dry-run --locked` passed without uploading |
| Workflow validation | actionlint 1.7.12, including expression-context validation and inline ShellCheck, passed |
| Copilot regressions | Real Bash subcommand/value completion and flag-help tests passed; both review threads resolved |

Terminal coverage includes macOS Bash 3.2, Linux Bash, Zsh, fish 3.6 on Linux, and fish 4.9.3 on macOS. It exercises prompts, existing hooks, exit status, editing, multiline input, EOF, Ctrl-C, Ctrl-Z, background/foreground jobs, resizing, terminal restoration, configuration reload, bypass, and adapter idempotence. Streaming coverage includes malformed bytes, split UTF-8 and ANSI sequences, combining marks, oversized words, slow input, and closed pipes.

## Review results

The standards review identified four concerns, all resolved and rechecked: compiler-version consistency, separation of word selection from ANSI emission, redundant initializer delegation, and repeated reload parameters. The spec review confirmed two session-guard defects involving invalid and split UTF-8; both were fixed and verified by real-terminal regressions. Its initial Bash prompt-status concern was withdrawn after the regression passed on the original implementation. No confirmed review finding remains outstanding.

The follow-up CI review fixed both Copilot comments: the upstream Bash generator's hyphenated-name dispatch mismatch and missing help descriptions for `--min-word-length` and `--style`. It added executable smoke checks shared by Cargo installs, native archives, pip/uv wheels, and native package-manager installs. The independent standards review found duplicated text/stdin smoke checks, which were consolidated; its follow-up and the spec review reported no remaining findings.

The first executing GitHub run exposed additional environment defects: unavailable macOS Python builds, an insufficient Go compiler for nFPM, a Linux Clippy warning, and insecure hosted-runner Zsh completion directories. CI now uses a Python version present in the official manifest for every runner architecture, the required Go compiler, direct token encoding, and corrected runner directory permissions while preserving framework security checks. Timing-sensitive CLI and split-UTF-8 expectations were replaced with deterministic timeout regressions and external stream coverage. The terminal-restoration harness now captures its baseline before the child starts. Native repository checks also fixed Debian metadata placement, content-path variable expansion, RPM 6 public-key import, and host ownership of container-created artifacts. The workflow also has pinned action commits, read-only permissions, timeouts, cancellation of superseded runs, and a separate scheduled RustSec audit.

Cargo is now included in the first-release distribution coverage. The crate's explicit file list and archive audit include runtime assets, all tests, and their support files while excluding developer state and unrelated tooling. CI installs the archive across the source-install matrix and runs the full packaged-source suite on Linux x86-64. The independent Cargo standards and spec reviews reported no remaining findings. crates.io publication and name ownership remain release prerequisites.

The Google Rust style review covers production code and Rust tests. [The style policy](rust-style.md) records the guide's application-code and platform exceptions. Changes include module and import conventions, public API/error/panic documentation, `must_use`, contextual test failures, borrowed configuration merging, and safe wrappers around necessary POSIX operations. `Transformer` now borrows configuration at construction and retains only its three emphasis values; a regression verifies that later configuration changes apply to a new stream. Unused per-command settings and unnecessary session configuration clones were removed. Terminal detection now uses safe standard-library APIs. Poisoned session mutexes return an error without accessing potentially inconsistent state; a regression test checks command and bypass rejection. Cargo lints enforce the mechanical rules, and CI builds Rustdoc and executes its examples. Zsh and fish completions were regenerated for the documented style values.

The independent Google-style standards review found unnecessary configuration clones, stale per-command fields, and an incomplete API description of byte preservation. These were fixed and re-reviewed with no remaining findings. The separate spec review found no behavior regressions; its documentation precision note was corrected. The parent reviewed and integrated both reports, then reran all 75 tests, the seven real framework loaders, and the Rustdoc example on Rust 1.85.1.

The executing Linux CI run exposed an existing race in the terminal-resize test: input forwarding could run `stty size` before the asynchronous resize signal reached the inner PTY. The test now polls for the observed inner dimensions with a fixed attempt limit, and prints the expected dimensions only after a successful observation. Its echoed command cannot satisfy the assertion. The terminal suite passed in a Debian ARM64 container, followed by 50 resize repetitions there and 20 on macOS ARM64.

## Release verification still required

The checkout contains candidate tooling; it is not a published release. CI configuration includes six native targets and wheel builds, signed Debian/Fedora/Arch repository install smokes, framework tests, and dependency audits. Remote results are available in [GitHub Actions](https://github.com/arturoaviles/bionic-reader/actions). CI has verified source installation on Linux and macOS x86-64 and ARM64, all six native archive builds/use checks, GNU/macOS wheel installations, musl wheel builds, real framework loaders, and dependency audits. Local Fedora 44 ARM64 signing, repository installation, installed CLI/PTY checks, and host cleanup also passed. Arch installation runs on a native x86-64 CI host; local ARM-to-x86 emulation could not run pacman’s syscall sandbox. The runner fixes above are included in subsequent PR checks; the latest PR check results are the source of truth for the complete matrix.

Before the first public release, validate installation, upgrade, uninstall, and session behavior from every requested manager on applicable supported hosts. This includes Cargo/crates.io, actual Homebrew and signed apt/dnf/pacman channels, Snap, pip/uv, and npm/Yarn/pnpm. Verify the macOS 11, glibc 2.28, and musl floors on their intended hosts, together with both CPU architectures. CI candidate checks exercise x86-64 and musl builds on modern hosted runners; they do not establish the declared minimum operating-system floors or public registry installations.

The [source repository](https://github.com/arturoaviles/bionic-reader) is public. Registry ownership and names, publisher credentials, signing identities, repository hosting and key distribution, and Snap Store classic-confinement approval remain external release prerequisites. No packages have been published.

## Microsoft Rust guideline review (2026-10-23)

Reviewed Microsoft's Pragmatic Rust Guidelines 2026.6 and recorded its application-specific decisions in [rust-style.md](rust-style.md). Compiler/Clippy coverage now includes all major stable groups and the recommended restriction lints. Public fallible APIs preserve typed source errors and a backtrace; transformer construction validates settings before processing. Session startup owns worker/child cleanup, bypass rollback handles activation and marker failures, and reload parses configuration outside the output-state lock.

The Divan 0.1.21 benchmark covers ASCII, Unicode with ANSI styles, punctuation, and passthrough with 1-, 64-, and 8192-byte chunks. Before/after comparisons used the same benchmark source and Rust 1.85.1 on macOS ARM64, with separate Cargo target directories. For the 6144-byte punctuation fixture passed in an 8192-byte chunk, sizing output from actual emphasis spans reduces its initial reservation from 55296 to 6144 bytes. Divan reports allocation requests falling from 74.22 KB to 25.07 KB; reallocations and parser buffers remain, and peak live allocation stays approximately 806.9 KB. This is a narrower reservation improvement, not a claim of lower total peak memory or a universal throughput gain.

An isolated release-build comparison tested the system allocator against mimalloc 0.1.52 on the same pre-review source. Both produced identical output. With stdin piped, stdout discarded, isolated HOME/XDG configuration, four warmups, 20 measured runs, and alternating binary order, median child CPU time was 38.46 ms versus 42.46 ms for a 458752-byte ASCII fixture, and 42.46 ms versus 42.21 ms for a 581632-byte Unicode/ANSI fixture. The binary retains the system allocator; these single-host results do not establish behavior on other platforms.

New CI checks run cargo-hack 0.6.45 on Rust 1.85.1, cargo-udeps 0.1.61 on nightly-2026-10-22, and two exact Miri regressions for split UTF-8/SGR and malformed bytes. Native tests cover process, terminal, and signal behavior. The benchmark's allocation profiler belongs only to its executable; the library does not configure a global allocator.
