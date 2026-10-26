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

## Repository package installation coverage (2026-10-23)

The README now contains local checkout/artifact installation, verification, and removal steps for Cargo, Homebrew, apt, dnf, pacman, Snap, pip, uv, npm, Yarn, and pnpm. Bionic Reader registry commands are removed. Cargo Git installs use the exact checked-out commit in CI; Homebrew consumes a SHA-256-checked `file://` archive through a local tap; Node's local base package refers only to its host-native local tarball. Linux packages retain the portable GNU build constraints, while host-local archive/wheel modes avoid unnecessary cross-build tooling for local installs.

The package lifecycle checks execute the installed command and CLI/PTY checks before removal. The Node suite includes a user-writable npm global prefix and executable Yarn/pnpm PATH wrappers. Snap checks preserve the launcher symlink's command name and restore the checker PATH inside the isolated shell after snapd's environment reset. Actual Snap build/install execution requires Ubuntu 24.04 and is a mandatory CI job, not a macOS-local claim.

Strengthened framework tests start the user's interactive shell and let the adapter activate a reading session. This exposed a fish bug: `init fish | source` gave the startup code pipe input, preventing terminal activation. The corrected `eval (bionic-reader init fish | string collect)` preserves terminal input and multiline code. All seven pinned upstream frameworks pass the fresh-shell activation and emphasized-output checks locally on macOS ARM64.

## Engineering priorities and coverage follow-up (2026-10-24)

[ADR 0002](adr/0002-document-rendering-and-reader.md) expands the terminal MVP to ICU4X multilingual segmentation, reusable fixation spans and custom segmenters, length rules and profiles, ANSI/plain/Markdown/HTML rendering, safe document controls, deterministic streaming, and an interactive reader with search and adjustable settings. The README documents repository installation for all 11 package-manager routes, seven framework adapters, and manual Bash/Zsh/fish completions. No Bionic Reader registry package has been published.

| Check | Current local result |
| --- | --- |
| Complete Rust 1.85.1 suite, macOS ARM64 | 292 passed, including 152 library tests and 62 document integration tests |
| Complete Rust 1.85.1 suite, Linux ARM64 | 295 passed, including the Linux-only input/output and descriptor-limit failures |
| Real upstream framework startup workflows | All seven passed with both coverage compilers |
| Rustdoc examples and documentation | Five examples passed; documentation built with warnings denied |
| Generated properties | Six properties passed with 4,096 cases each |
| Sanitizer fuzzing with varied fixation settings | The recovered timeout input completed in 212 ms; a fresh CI-seed corpus completed 890 inputs in 62 seconds without a failure |
| Makefile verification | `make verify` passed, including both Rust workspaces, completion drift, and distribution source checks |
| Zsh completion | Real ZLE completion passed for subcommands, options, and option values |
| Linux release streaming memory | 40 MiB input; 11,935,744-byte peak RSS and 7,196,672-byte growth after warmup |
| RustSec lockfile audits | No reported advisories in the 117-dependency application lockfile or 105-dependency fuzz lockfile using the cached advisory database; CI refreshes it |

Terminal workflows cover reader navigation/search/settings, resizing, restoration after signals, shell job control, interrupted bypass recovery, and startup cleanup. Linux tests apply descriptor limits only to child processes and check five failure stages: integration preparation, PTY creation, PTY read/write setup, and Bash startup preparation. They verify diagnostics, terminal restoration, and temporary-file cleanup. Executable tests also exercise `/dev/full`, unreadable stdin, configuration errors, saved-file pipelines, format conversion, and malformed input.

Coverage-driven regressions fixed raw and UTF-8-encoded C1 controls receiving emphasis inside escape sequences, UTF-8 continuation bytes mistaken for C1 terminators, oversized/split control boundaries, malformed intermediate escapes, repeated rescanning of long controls and Unicode suffixes, and alphabetic combining marks without a base letter. Operation-count tests bound parser work independently of machine speed. Markdown frequency now remains repeat-safe when generated strong prefixes divide a word across parser events; a generated property varies frequency, minimum length, and fixation strength.

CI subsequently found a malformed UTF-8 suffix after long combining-mark runs that differed between bytewise and whole-input rendering, then a raw-byte mutation that caused repeated suffix scans. The fix distinguishes incomplete scalars from definitively invalid bytes and emits complete raw tokens through the parser's complete-input boundary. Incomplete UTF-8 remains buffered. The 70,007-byte mismatch and 32,822-byte timeout reproducers run as unit tests with chunk-equality and linear scan-work bounds and seed the sanitizer fuzzer. Ubuntu's global Zsh startup also prompted before the isolated completion configuration; the completion test now skips global startup files while loading its own real ZLE configuration. Additional tests cover concurrent session controls and shutdown, the no-terminal diagnostic, Markdown entity spelling, code opt-in escaping, invalid document UTF-8, and partial output followed by a read failure.

The property configuration previously fixed the count at 256 even when a larger `PROPTEST_CASES` was requested. It now honors that variable, and the current six-property run actually used 4,096 cases per property. CI sets the same count.

The first measured minimum-compiler baseline was **84.09% line coverage**. The clean final Rust 1.85.1 measurement is **93.57% lines**, **88.46% regions**, and **88.66% functions**. The separate nightly-2026-10-22 measurement is **92.86% lines**, **91.73% regions**, **89.01% functions**, and **80.58% branches**. Both include all seven real framework startup workflows and use cargo-llvm-cov 0.9.1. Different compiler instrumentation produces different denominators; these percentages are not interchangeable. No production module is omitted.

| Rust source | Rust 1.85.1 line coverage |
| --- | --- |
| `document.rs` | 91.49% |
| `error.rs` | 100.00% |
| `main.rs` | 96.40% |
| `reader.rs` | 94.53% |
| `segmentation.rs` | 98.77% |
| `session.rs` | 87.95% |
| `settings.rs` | 99.42% |
| `shell.rs` | 100.00% |
| `transformer.rs` | 95.79% |

**The requested 100% target remains unmet.** The MSRV report has 660 uncovered lines; nightly has 676 uncovered lines and 348 uncovered branches. Most remaining paths concern operating-system/worker failures and defensive parser diagnostics. Inline unit-test code is also represented in the compiler's source-file measurements. Passing tests does not establish that every fault or branch has been exercised.

CI publishes the actual measurements and the open 100% target in its job summary, uploads HTML/JSON/LCOV/uncovered-line artifacts, and enforces transitional floors of 92% lines, 91% regions, 88% functions, and 80% branches. These floors are regression gates, not completion of the target. The fuzzer has a ten-second per-input timeout and uploads failure inputs; malformed-byte, multilingual, C1/ANSI, Markdown, combining-mark, and oversized-control seeds reproduce known edge cases.

Reader view buffers are bounded, including a 4 KiB oversized-grapheme display fallback; the line-position index grows with the number of lines. Markdown streaming spools to disk and uses bounded blocks/reference data with documented literal fallbacks. Plain/terminal processing retains malformed ordinary bytes outside removed controls; Unicode document conversion rejects malformed UTF-8, while the reader displays replacement characters. These policies do not promise perfect linguistic analysis or a reading-speed improvement.

Use `make help` for common commands and [CONTRIBUTING.md](../CONTRIBUTING.md) for tool versions, coverage reproduction, fuzzing, and installation checks. The Makefile's core checks are exercised by CI. Both application and fuzz-workspace lockfiles are audited. [The current PR matrix](https://github.com/arturoaviles/bionic-reader/pull/2/checks) records results for each pushed revision; newer changes do not inherit earlier passing results. Registry publication, hosted repository distribution, minimum-OS validation, upgrades, and Snap Store approval remain release gates.

## Targeted CI assurance (2026-10-26)

The PR matrix and all 11 installation routes are retained. New workflow checks
cover spelling, YAML duplicates, local Markdown anchors/links and workflow
security. Dependency policy covers both locked graphs and all supported targets;
Dependency Review and Rust/Python/JavaScript CodeQL are configured for every PR.
RustSec runs daily, external links and mutation/performance reports weekly, and
ASan fuzzing nightly for 15 minutes alongside the existing 60-second PR pass.

Local validation on macOS ARM64 with Rust 1.85.1 passed `make verify`, including
all existing Rust suites, five Rustdoc examples, 4,096-case properties, completion
drift, distribution checks and the new offline checks. Seven CI gate/checksum
regressions passed. Freshly fetched application/fuzz graphs passed cargo-deny;
expected duplicate-version and unused-license-allowance warnings remain. The
approved NCSA exception is limited to libfuzzer-sys 0.4.10. Deliberately invalid
license/source/link/YAML/workflow inputs were rejected; the exception also rejects
NCSA for libfuzzer-sys 0.4.11. The rollout helper refused the old passing commit
because the new check contexts had not registered, leaving rules unchanged.

A native ARM64 archive/wheel/SBOM bundle passed complete-manifest SHA-256
verification and pip/uv pip/uv tool installation, CLI/PTY use and removal. The
CycloneDX 1.5 binary SBOM contains 85 components and excludes divan/proptest.
Divan and hyperfine reports completed; the local binary measured 4,820,368 bytes
and startup averaged approximately 1.9 ms over 30 runs. These are single-host
advisory measurements, not regression thresholds. A five-minute segmentation
mutation smoke completed its baseline, found survivors and mutant timeouts,
and preserved its partial outcomes when the campaign budget expired. The full
45-minute-per-module weekly reports still require their first scheduled run.

The current main ruleset was snapshotted. Required-check enforcement and CodeQL
alert blocking are prepared with a guarded rollout helper; activation requires
successful new PR contexts, matching workflows on main, a successful main CI run
including candidate attestations, and a fresh ruleset snapshot. See [the rollout
record and commands](ci/README.md). Main-push attestation verification, fork and
Dependabot execution, the complete remote matrix and initial scheduled reports
must be recorded for the new revision before claiming rollout completion.
The new offline glibc 2.28 smoke also passed against the checksum-verified
same-source Linux ARM64 candidate from GitHub run 40131670249. The container
requires an explicitly executable temporary extraction mount; the workflow now
sets it. Native x86-64 execution remains part of the remote matrix. macOS 11
runtime validation and registry/release publication remain release work.

The first new PR run (40278345339) registered the new checks and exposed three
integration defects: Linux lychee archives use a directory prefix, rust-cache's
concurrent compiler probes could race rustup's automatic component installation,
and Cargo's unanchored `README.md` include pattern also matched the new CI README.
The fixes extract the exact verified archive member, install declared components
before cache probes/select nightly explicitly, and anchor root Cargo package
support files. Dependency Review, both dependency policies/audits, Gitleaks and
all three CodeQL languages passed on that first revision. Subsequent matrix
results must be checked at the latest commit.

The corrected implementation at `9e3a4545c67b340213307a198d16db50541dcd53`
passed the [complete PR CI run](https://github.com/arturoaviles/bionic-reader/actions/runs/40278944373).
The PR reported **39 successful checks**, with only the main-push attestation job
intentionally skipped. All six native bundles, native x86-64/ARM64 glibc 2.28
execution, source/Git/package installation, all 11 installation routes, coverage
floors, Miri, PR fuzzing, workflow/document checks and CI gate passed. Separate
RustSec/policy/Dependency Review, Gitleaks, all three CodeQL languages and
Dependabot configuration validation also passed; no open CodeQL alerts were
reported for the PR. These results identify that exact implementation commit;
subsequent revisions still require their own full checks.

The failing first run also exercised the real aggregate gate: it rejected the
failed Rust/candidate/package/lint/source matrices and the resulting skipped
runtime job. After the fixes, the same gate passed. The refreshed local Cargo
archive passed its content audit, installation, CLI/PTY use and uninstall. An
additional 60-second settings mutation smoke tested 15 mutants (nine caught,
six surviving) and successfully stopped both workers at its budget while retaining
logs and outcomes. These partial mutation reports remain advisory; the full
scheduled campaigns and main-push attestations await the main rollout described
in [the CI rollout instructions](ci/README.md).

## Live CI rollout

PR #2 was merged after its final matrix passed. Main candidate provenance/SBOM
verification passed for all six targets, and the guarded ruleset activation
succeeded. The first assurance reports and effective merge-policy details are
recorded in [the 2026-10-26 rollout record](ci/rollout-2026-10-26.md).

A follow-up regression closes the gate-consistency scanner's handling of valid
job IDs containing digits, underscores or capitals. Before the fix, an omitted
`integration_v2` job escaped the dependency guard; the corrected scanner rejects
it. Eight gate/checksum/toolchain tests, Ruff and strict typing checks passed
locally. The follow-up itself must pass the newly required PR checks.
