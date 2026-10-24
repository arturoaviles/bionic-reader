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

The priority audit expanded the original terminal MVP according to [ADR 0002](adr/0002-document-rendering-and-reader.md). The implementation now provides ICU4X multilingual word segmentation, grapheme-based fixation, replaceable segmentation and reusable spans, per-length rules and profiles, safe document controls, deterministic default streaming, Markdown-aware source preservation, ANSI/plain/Markdown/HTML output, and an interactive document reader with search and live settings.

The independent review found and the parent fixed: reader signal-setup ordering, C1 sequence residue, nested reference-definition protection, giant-grapheme quadratic work and unbounded retention, repeated rescanning during backward search, reference-catalog reparsing for every Markdown block, and unsafe-link escaping that disabled surrounding Markdown structure. New end-to-end tests also exposed missing inline-code HTML tags, source-layout changes in Markdown-to-ANSI conversion, repeated final reader rows, and omitted inline-code emphasis with code opt-in. Fuzzing found an oversized-control terminator split bug; its minimized behavior is now an ordinary regression. An analogous 64 KiB session-marker boundary is covered and fixed.

| Check | Current local result |
| --- | --- |
| Rust 1.85.1 complete suite, macOS ARM64 | 219 passed, including 111 library tests and 42 document integration tests |
| Actual upstream framework startup workflows | All seven passed with coverage instrumentation |
| Rustdoc examples and warning-free documentation | Five examples passed; documentation build passed |
| Generated properties | Five properties passed with 4,096 cases each |
| End-to-end workflows | Real executable, saved ANSI files, piped input, Markdown reprocessing, HTML export, config/flag precedence, diagnostics, reader PTYs, and interactive shell sessions passed |
| Rust formatting, Clippy, distribution static checks, actionlint, generated completions | Passed |
| Installed CLI and PTY checker | Passed with new format, profile, control-policy, and Markdown checks |
| Linux release streaming memory | 40 MiB transformed; 11,776,000-byte peak RSS and 6,844,416-byte growth after warmup |
| RustSec lockfile audit | No reported advisories in 117 dependencies using the locally cached advisory database; CI refreshes it |

The first measured minimum-compiler baseline was **84.09% line coverage**. After adding behavioral, error-injection, property, and executable workflow tests, the clean Rust 1.85.1 measurement is **90.74% lines**, **85.73% regions**, and **84.72% functions**. These are measured with cargo-llvm-cov 0.9.1; production files are included. The separate nightly-2026-10-22 run, including the real framework startup workflows, measures **89.40% lines**, **87.93% regions**, **85.10% functions**, and **77.61% branches**. Compiler instrumentation produces different denominators, so the two line percentages are not interchangeable.

| Rust source | Rust 1.85.1 line coverage |
| --- | --- |
| `document.rs` | 89.61% |
| `error.rs` | 100.00% |
| `main.rs` | 95.56% |
| `reader.rs` | 90.11% |
| `segmentation.rs` | 100.00% |
| `session.rs` | 84.05% |
| `settings.rs` | 98.85% |
| `shell.rs` | 95.83% |
| `transformer.rs` | 93.39% |

**The requested 100% target remains unmet.** The minimum-compiler report contains 761 uncovered lines. The largest remaining area is session worker/startup and operating-system failure handling, together with terminal error paths and defensive diagnostics. Coverage percentages do not establish that every behavior or operating-system fault has been exercised. CI currently prevents regressions below 85% lines, 80% regions, and 80% functions in its nightly report; these are transitional floors, not the 100% target.

The coverage job uploads HTML, JSON, LCOV, and uncovered-line artifacts. It instruments unit, property, CLI, file-pipeline, reader/session PTY, and seven pinned framework workflows. A separate sanitizer fuzz job runs for 60 seconds with Unicode, ANSI, Markdown, malformed-byte, active-control, and oversized-word seeds. The Linux memory gate uses a release build and measures the executable's own peak with `wait4`, avoiding inherited compiler peaks.

Reader working buffers are bounded, including a 4 KiB oversized-grapheme display fallback; the line-position index still grows with the number of lines. Markdown streaming spools to disk and uses bounded blocks/reference data with documented literal fallbacks. Plain/terminal processing retains malformed ordinary bytes; Unicode-dependent document conversion rejects malformed UTF-8, while the interactive view displays replacement characters. These policies are documented in the README and are not claims of linguistic perfection, speed improvement, constant-size indexes, or unlimited parser blocks.

The coverage-driven follow-up adds real-terminal preview, terminal help, missing-reader-input, absent HOME/config paths, zero-fixation rules, and invalid incremental grapheme-boundary tests. The Unicode segmentation/fixation module now reaches 100% line coverage on Rust 1.85.1; the separate nightly compiler reports 99.72% lines and 90.62% branches for that module. The project-wide target remains open.

Executing CI caught two test assertions that included optional diagnostic backtraces, a Python smoke PTY with zero window dimensions on Linux, and an ambiguous Cargo Git install after adding the separate fuzz package. Assertions now use the documented backtrace-free display, the smoke PTY starts at 24×80 before exec, and the Git install checker selects `bionic-reader` explicitly (as the README already did). All 30 checks passed for [commit 7882400](https://github.com/arturoaviles/bionic-reader/actions/runs/40107227086), including every package-manager lifecycle and seven real framework workflows. Subsequent coverage tests are checked by the same matrix; the PR's current results remain authoritative.
