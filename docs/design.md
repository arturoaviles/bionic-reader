# Bionic Reader design

Status: specification published as [GitHub issue #1](https://github.com/arturoaviles/bionic-reader/issues/1). The Rust CLI, reading sessions, shell adapters, tests, and distribution candidate tooling are implemented. The [source repository](https://github.com/arturoaviles/bionic-reader) is public. Package publication remains gated on the release prerequisites below. This document records agreed product decisions and synthesized implementation defaults; use the published specification as the implementation reference.

## Product

- Build a Rust CLI named `bionic-reader` that performs local word-prefix emphasis, with no external text-processing service.
- Accept explicit text with `bionic-reader -t "<TEXT>"` and stdin with `cat <file> | bionic-reader`.
- Use terminal bold by default, with adjustable emphasis length and frequency.
- Default to emphasizing the first half of every eligible Latin-script word, rounding up and leaving single-letter words unchanged. Expose `--fraction 0.5`, `--every 1`, and `--min-word-length 2`; count accented letters correctly.
- Internal apostrophes remain part of a word. Hyphens, underscores, digits, and other punctuation separate word segments. Preserve all original characters; leave digits and emoji unchanged and keep combining marks attached to their letters.
- Preserve text and layout, including mixed prose, source code, logs, URLs, and existing colors. Readable prose is the primary target; do not claim a guaranteed reading-speed improvement.
- Emphasize Latin-script words, including accented text. Preserve other scripts unchanged in v1.

## Output and shell integration

- Support `--style auto|always|never`, defaulting to `auto`: add emphasis on terminal stdout and add no emphasis when stdout is redirected, unless explicitly overridden. Both redirected `auto` and `never` preserve existing input formatting; they do not strip ANSI sequences.
- Include an opt-in reading session using a pseudo-terminal in addition to explicit text and stdin filtering.
- Emphasize command output while preserving shell prompts, editing, and typed input through shell integration boundaries. Bypass known interactive commands and full-screen applications such as Vim, top, and fzf, and use passthrough when terminal behavior is uncertain. Provide configurable command exclusions and an explicit bypass. Application-generated prompts, nested interactive programs, and background output cannot always be classified reliably; do not promise universal detection.
- Integrate with Bash, Zsh, and fish configuration files.
- Include macOS system Bash 3.2 as well as modern Bash, using a compatibility path for its older prompt and execution hooks. Preserve existing hooks and command exit status; test the legacy path explicitly.
- Provide and test adapters for Bash-it, Oh My Bash, Oh My Zsh, Prezto, Zinit, Fisher, and Oh My Fish. Each adapter activates the shared integration for its shell.
- Support manual startup with `bionic-reader session` and generated Bash/Zsh/fish initialization snippets for automatic startup in interactive terminals. Adding the snippet is the opt-in; package installation does not edit shell configuration.
- Provide `bionic-reader on`, `off`, `toggle`, and `reload` scoped to the current reading session. Reload explicitly applies configuration-file changes.
- Provide `bionic-reader bypass -- <command>` to suspend transformation for a command and restore the prior state afterward.
- Document the automatic-start escape hatch as `BIONIC_READER_DISABLE=1`. The initializer must be interactive-only and idempotent, and must not create nested reading sessions.

## Implementation defaults for final review

- Count Latin letters with attached combining marks as visible letter units without normalizing or rewriting input. Internal straight and curly apostrophes do not add to the letter count. Apply the same segmentation to prose, code, logs, paths, and URLs; there is no language-specific syntax rewriting.
- `--fraction` accepts a finite value from 0 through 1. `--every` and `--min-word-length` accept positive integers. A zero fraction adds no emphasis.
- With `--every N`, select the first eligible word and then every Nth eligible word. Short words and unsupported scripts do not advance the eligible-word count. Reset the count for each filter invocation and each marked shell command.
- Preserve whitespace and trailing-newline presence exactly. Do not append a newline to explicit text unless it was present in the input.
- Existing SGR styling and hyperlink payloads must survive transformation. Restore the prior emphasis state after each emphasized prefix rather than clearing unrelated styles. Terminal control sequences are not words.
- In sessions, use shell execution boundaries and known interactive-command exclusions. Treat unsafe or unrecognized terminal activity as passthrough until a trustworthy shell boundary. Background output can only be classified by the interval in which it arrives.
- Controls affect only the current reading session and report a clear error outside one. Failed reloads keep the last valid settings. Apply successful reloads at the next command boundary.
- Preserve the exit status and terminal interaction of bypassed commands, restoring the previous reading state on completion or interruption.

## Streaming

- Process input incrementally with bounded buffering rather than waiting for an entire file or stream.
- Pass malformed bytes through unchanged. Leave oversized or stalled partial words unemphasized instead of buffering or delaying output indefinitely.
- Use initial bounds of 64 KiB for a pending word or control sequence and a 50 ms flush deadline for partial output. When a word falls back to passthrough, keep the remainder of that word unchanged. Validate these bounds in the terminal prototype before treating them as release defaults.

## Input and configuration

- `-t` supplies the complete input without reading stdin, even when stdin is piped. Explicit empty text is valid.
- Without `-t`, read stdin when redirected. When stdin is an interactive terminal, show help instead of waiting for text.
- Store user settings in TOML at `$XDG_CONFIG_HOME/bionic-reader/config.toml`, falling back to `~/.config/bionic-reader/config.toml`.
- Allow an explicit file with `--config <path>`.
- Command-line flags override file settings, which override built-in defaults. Report invalid configuration clearly.

## Release coverage

- Support macOS and Linux on x86-64 and ARM64. Windows is deferred.
- Target macOS 11+, Linux glibc 2.28+, and musl-based Linux on both architectures. Package managers may impose stricter host requirements.
- Every requested package manager is required at the first public release, on its applicable operating systems: Homebrew, apt, dnf, pacman, Snap, pip, uv, npm, Yarn, and pnpm.
- Project-maintained distribution channels are acceptable: a Homebrew tap, project package repositories, Snap Store, PyPI, and npm. Inclusion in operating-system official repositories is not a release requirement.
- Snap must preserve reading-session functionality through classic confinement. First public release waits for [Snap Store classic-confinement approval](https://snapcraft.io/docs/reference/administration/reviewing-classic-confinement-snaps/). If approval is refused, revisit the release requirement rather than silently reducing Snap functionality.
- The source repository is public and available for cloning and source installation. Public package availability remains subject to the release prerequisites.
- Use the MIT license with copyright attribution to `arturoaviles and contributors`.
- The executable is always `bionic-reader`. Prefer that package name wherever registrable; use `@arturoaviles/bionic-reader` as the npm fallback and `bionic-reader-cli` on registries without namespaces. Registry-name availability is provisional until reserved.

## Implementation and distribution approach

- Separate the Rust text transformer, ANSI-aware renderer, CLI/configuration handling, and PTY/session integration so filter behavior can be tested without a shell.
- Prove the session boundary and passthrough behavior in Bash 3.2, modern Bash, Zsh, and fish before expanding framework adapters. Preserve prompts and existing hooks, propagate resizing and signals, maintain job control, and restore terminal state on normal exit, errors, and handled signals.
- Build macOS x86-64/ARM64 binaries and Linux GNU/musl x86-64/ARM64 binaries. Audit actual deployment targets and linked-library requirements.
- Distribute one versioned Rust implementation through native packages and thin Python/JavaScript packaging. Python and JavaScript packages must not reimplement text processing.
- Deliver a Homebrew tap, signed apt/dnf/pacman project repositories, a classic Snap, platform-specific PyPI wheels usable by pip and uv, and npm packages usable by npm, Yarn, and pnpm.
- Prefer npm platform-specific binary dependencies over install-time compilation or downloads. Verify launcher behavior and modern Yarn execution modes; do not assume all Yarn versions provide global installation.
- Include shell initialization, framework adapters, completions, configuration examples, and uninstall instructions with the applicable packages. Removing integration must not require reconstructing overwritten user configuration.
- Prepare registry ownership, name reservations, signing keys, repository hosting, and publisher credentials before release. Validate artifacts before publishing; Snap approval remains an external release blocker.

## Acceptance checks

- Exact `-t` and stdin behavior, empty input, non-TTY defaults, forced and disabled emphasis, configuration precedence, and clear invalid-setting errors.
- Golden cases for accented and decomposed Latin text, apostrophes, hyphens, punctuation, digits, emoji, unsupported scripts, source code, paths, URLs, and whitespace preservation.
- Streaming tests across arbitrary byte boundaries, including split UTF-8 and ANSI sequences, malformed bytes, slow partial words, oversized words, large input, and broken pipes without panics. Equivalent chunking without a timeout must produce equivalent output.
- Existing colors, bold/dim state, hyperlinks, and unknown controls survive without style leakage or altered control payloads.
- PTY tests cover shell prompts, completion and input editing, multiline commands, full-screen bypass, known interactive commands, Ctrl-C, Ctrl-Z/foreground/background jobs, resizing, EOF, and child exit status.
- Verify all seven framework adapters compose with existing hooks and initialize once. Missing or inconsistent integration falls back to passthrough.
- Verify session on/off/toggle/reload, invalid reload retention, per-command bypass restoration, recursion prevention, the disable escape hatch, and isolation between simultaneous sessions.
- Installation and uninstall smoke tests for every requested package-manager route on its supported platforms. Exercise `-t`, stdin, shell initialization, session behavior, and version reporting from installed artifacts; verify Python and Node wrappers retain native behavior.

## Published specification

The product decisions are confirmed through Q1–Q27. The conversation and design were synthesized into [GitHub issue #1](https://github.com/arturoaviles/bionic-reader/issues/1) using `to-spec`, including user stories, implementation decisions, testing decisions, scope boundaries, and release prerequisites. The implementation exercises the buffer and flush bounds through streaming and PTY tests. Package publication, registry reservation, signing identities, repository hosting, and Snap approval remain release work; local candidates do not establish public package availability.
