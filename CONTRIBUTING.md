# Contributing

Contributions should preserve the product decisions in [docs/design.md](docs/design.md), the vocabulary in [GLOSSARY.md](GLOSSARY.md), and the architecture boundaries in [ADR 0001](docs/adr/0001-session-wide-terminal-transformation.md) and [ADR 0002](docs/adr/0002-document-rendering-and-reader.md). ADR 0002 expands the original terminal MVP to multilingual documents and the interactive reader.

Rust code follows [Google's Rust style guide](https://google.github.io/styleguide/rustguide.html) and [Microsoft's Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/guidelines/index.html). Read [the project's Rust style policy](docs/rust-style.md) for its application to this Cargo CLI, documented exceptions, and automated enforcement.

The [library guide](docs/LIBRARY.md) covers the shared engine, language bindings, installed-package checks, and SSR examples. The workspace default member is the root crate; use explicit binding packages when checking adapters.

## Development checks

Use Rust 1.85.1 for the declared minimum compiler. The root Makefile works with GNU Make 3.81 or newer, including macOS's system Make. Start with:

```sh
make help
make verify
make test-e2e
make run ARGS='--style always -t "Reading documentation"'
make release
```

`make verify` runs formatting, type checking, Clippy, all Rust tests, Rustdoc tests/documentation, completion drift checks, distribution source checks, workflow/YAML/security lint, tracked spelling, local Markdown links/anchors, and CI gate/toolchain consistency. Formatting, type checking, and linting include the separate fuzz workspace. `make test-e2e` runs executable, reader, reading-session, terminal, and completion workflows. `make test-unit` runs library and CLI unit tests. `scripts/check-quality.sh` invokes `make verify` and also audits both lockfiles if cargo-audit is installed.

`make install` installs this checkout into Cargo's configured installation root; `make uninstall` removes that Cargo installation. `make smoke` checks the built executable and a real terminal session. Use `make completions` after changing CLI arguments to regenerate all three checked-in completion files, then run `make completions-check`.

The Makefile honors `CARGO_TARGET_DIR`, including for completion and smoke-test binary paths. Override `CARGO`, `PYTHON`, or `NIGHTLY` when needed; `ARGS` supplies arguments to `make run`. Nightly defaults to CI's `nightly-2026-10-22`. Make targets do not install development tools or publish packages. `make frameworks` fetches upstream framework checkouts; `make audit` can update the advisory database.

The equivalent individual Rust checks are:

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path fuzz/Cargo.toml --all -- --check
cargo check --locked --workspace --all-targets
cargo check --locked --manifest-path fuzz/Cargo.toml --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo test --locked --all-targets
cargo test --locked --doc
scripts/check-completions.sh "${CARGO_TARGET_DIR:-target}/debug/bionic-reader"
scripts/check-distribution.sh
```

The test suite includes unit, property, CLI, document, and PTY/reader/session coverage. Terminal tests need Bash, Zsh, and fish; Node.js is needed for launcher tests. Distribution checks also require ShellCheck, Python 3, Ruff, mypy, and Ruby. Checking the fuzz workspace requires a C++ compiler. For dependency advisories, install the pinned `cargo-audit` version listed by CI and run `make audit` when its advisory database is reachable. A missing tool is a local environment setup issue; CI installs the required checkers explicitly. See [CI tools and rollout](docs/ci/README.md) for new checker versions, installation, policy exceptions, candidate verification and merge enforcement.

CI runs each generated property with `PROPTEST_CASES=4096`. The local Proptest default is 256; use `PROPTEST_CASES=4096 make test` for the same case count. The test configuration honors this environment variable.

## Coverage and fuzzing

Install the pinned nightly coverage tool, then generate line/branch coverage and HTML:

```sh
rustup toolchain install nightly-2026-10-22 --profile minimal --component llvm-tools-preview
cargo +nightly-2026-10-22 install cargo-llvm-cov --version 0.9.1 --locked
make coverage
```

Open `target/coverage/html/index.html`, or the equivalent path under `CARGO_TARGET_DIR`. This instruments the default test suite; the seven network-dependent framework tests are ignored until explicitly requested. To include those real startup workflows as CI does, use the same instrumented build directory throughout:

```sh
(
  export RUSTUP_TOOLCHAIN=nightly-2026-10-22
  export CARGO_TARGET_DIR="$PWD/target/coverage-build"
  export CARGO_LLVM_COV_TARGET_DIR="$CARGO_TARGET_DIR"
  cargo llvm-cov clean --workspace
  cargo llvm-cov --branch --locked --all-targets --no-report
  eval "$(cargo llvm-cov show-env --branch --sh)"
  scripts/check-frameworks.sh
  cargo llvm-cov report --show-missing-lines
  cargo llvm-cov report --html --output-dir target/coverage
)
```

Clean the instrumented workspace after source changes before taking a fresh measurement; stale object files can distort the report. CI uploads HTML, JSON, LCOV, and uncovered-line artifacts and compares the measurements with the requested 100% target. Current results and remaining gaps are recorded in [docs/verification.md](docs/verification.md). Passing the transitional coverage floors does not mean the target has been reached.

For sanitizer fuzzing, install `cargo +nightly-2026-10-22 install cargo-fuzz --version 0.13.2 --locked`, then run `make fuzz`. It uses a 60-second campaign, a 128 KiB maximum input, and a ten-second per-input timeout, matching CI. See [fuzz/README.md](fuzz/README.md) for reproduction and regression-case instructions. `make bench` runs the transformer benchmarks. Local and CI fuzz runs share `scripts/seed-fuzz-corpus.py`; a separate nightly ASan campaign runs for 15 minutes.

## Additional Rust verification and benchmarks

CI pins cargo-hack 0.6.45, cargo-udeps 0.1.61, and nightly-2026-10-22 for the checks that require nightly. The project's source, tests, and benchmarks still support Rust 1.85.1. Reproduce these checks after installing the corresponding tools:

```sh
cargo +1.85.1 hack check --locked --feature-powerset --all-targets
cargo +nightly-2026-10-22 udeps --locked --all-targets --all-features
cargo +nightly-2026-10-22 miri test --locked --lib transformer::tests::chunk_boundaries_inside_utf8_and_sgr_do_not_change_output -- --exact
cargo +nightly-2026-10-22 miri test --locked --lib transformer::tests::malformed_bytes_are_preserved_while_surrounding_words_transform -- --exact
cargo bench --locked --bench transformer
```

Install Miri and rust-src with `rustup toolchain install nightly-2026-10-22 --profile minimal --component miri --component rust-src`. Install the pinned maintenance tools with `cargo +nightly-2026-10-22 install cargo-hack --version 0.6.45 --locked` and the equivalent command for cargo-udeps 0.1.61. The Miri commands interpret specific pure byte-stream cases; real terminal/process operations remain covered by native integration tests.

The Divan benchmark reports bytes per second and allocations for ASCII, Unicode with existing ANSI styles, punctuation, and passthrough at three chunk sizes. Its profiling allocator applies only to the benchmark executable. Compare changes on the same idle machine with the same compiler and settings; allocation profiling affects timings. CI compiles and smoke-executes all benchmark cases through `cargo test --all-targets`. Weekly reports measure throughput, allocations, startup and binary size without timing regression thresholds.

## Build, install, and use checks

The `source-install` CI job installs the checkout with Cargo into a temporary root, exercises the installed binary, and uninstalls it. It covers Linux and macOS on x86-64 and ARM64 with Rust 1.85.1, plus current stable Rust on Linux x86-64. Each entry also packages a `.crate`, installs its extracted source outside the checkout, runs the installed CLI/PTY checks, and uninstalls it. The six-target candidate build matrix also extracts each native archive and exercises its executable on the matching host architecture.

To reproduce the installation check without changing your normal Cargo installation:

```sh
install_root=$(mktemp -d)
cargo install --path . --locked --root "$install_root"
python3 scripts/check-installed-cli.py "$install_root/bin/bionic-reader"
cargo uninstall --root "$install_root" bionic-reader
rm -rf "$install_root"
```

The checker requires Python 3.9 or newer and `/bin/bash`. It isolates HOME and XDG configuration and verifies text/stdin behavior, ANSI output, configuration precedence, shell initialization and completions, session controls, and a real PTY reading session. Pass the actual installed binary path; the checker does not substitute a build-tree executable.

Workflow changes are checked with [actionlint](https://github.com/rhysd/actionlint) 1.7.12, including GitHub expression contexts and inline shell scripts. Run `actionlint` locally when editing `.github/workflows/`. Actions are pinned to verified commit SHAs. Build and PR jobs use read-only repository permissions, timeouts, and cancellation of superseded runs. CodeQL has only the additional security-events permission. Main-push candidate attestation has a separate job with narrowly scoped write permissions. The separate dependency-security workflow audits both lockfiles on every PR and main push, daily, and on manual dispatch. It also enforces dependency policy. Dependency Review runs on every PR; CodeQL analyzes Rust, Python, and JavaScript/TypeScript on PRs, main pushes, and weekly.

## Secret scanning

The separate secret-scanning workflow runs Gitleaks 8.30.1 on every pull request, pushes to `main`, weekly after merge, and manual dispatch. It fetches the full Git history, scans all fetched refs, redacts detected values, and fails when a secret is found. The Linux binary is pinned by version and SHA-256; the workflow uses read-only repository permissions.

With the same [Gitleaks release](https://github.com/gitleaks/gitleaks/releases/tag/v8.30.1) installed, reproduce the scan locally:

```sh
gitleaks git --log-opts="--all" --redact=100 --no-banner --exit-code=1 .
```

## Cargo crate validation

Bionic Reader is published on crates.io. Contributors can also validate installations from this checkout, its Git source, or an extracted local `.crate`. The manifest includes the Rust source, embedded shell integrations, framework adapters, completions, configuration example, license, README, tests, and their support files. Its explicit file list excludes developer state and unrelated distribution tooling. The development Makefile belongs to the Git checkout; packaged-source tests run with Cargo directly.

From a clean checkout, run:

```sh
scripts/check-cargo-package.sh
```

This runs Cargo's verified `package --locked`, checks the archive contents, extracts the `.crate` to a temporary directory, installs it with Cargo, exercises the installed binary and a reading session, and uninstalls it. The archive remains under Cargo's `target/package/` directory (or the target directory selected by `CARGO_TARGET_DIR`). CI covers the same five OS/architecture/toolchain combinations as source installation and runs the full test suite against the packaged source on Linux x86-64. That job also uploads the `.crate` as a workflow artifact.

Before publishing to crates.io, check crates.io name ownership and credentials, then run the final preflight:

```sh
cargo publish --dry-run --locked
```

The dry run does not upload the crate. No workflow publishes it automatically. Use the [build guide](docs/BUILD.md) for source/Git installation before registry publication. See the official [Cargo package](https://doc.rust-lang.org/cargo/commands/cargo-package.html), [install](https://doc.rust-lang.org/cargo/commands/cargo-install.html), and [publish](https://doc.rust-lang.org/cargo/commands/cargo-publish.html) documentation.

## Candidate binaries

`scripts/build-release.sh` builds six local archive candidates:

```sh
scripts/build-release.sh x86_64-apple-darwin
scripts/build-release.sh aarch64-apple-darwin
scripts/build-release.sh x86_64-unknown-linux-gnu
scripts/build-release.sh aarch64-unknown-linux-gnu
scripts/build-release.sh x86_64-unknown-linux-musl
scripts/build-release.sh aarch64-unknown-linux-musl
```

With no arguments it attempts all targets and stops if the current host lacks the required build support. Build macOS artifacts on matching Intel and Apple Silicon macOS runners. GNU Linux candidates use cargo-zigbuild 0.23.4 with Zig 0.15.1 and are checked for GLIBC symbol versions above 2.28. Musl CLI candidates need a matching Linux host with `musl-gcc`; their Python extensions use Zig 0.15.1 to avoid linking glibc runtime libraries. `SOURCE_DATE_EPOCH` can be set to reproduce archive timestamps; the default comes from the current Git commit. Candidate archives and checksums are written under `dist/candidates/v<VERSION>/`.

`scripts/package-linux.sh TARGET` converts a built GNU candidate into deb, rpm, and Arch Linux packages with nFPM 2.47.0. It includes the README, license, config example, shell integration/framework adapters, and shell completions; it does not run installation scripts or edit user shell configuration. `scripts/package-manager-install-smoke.sh` builds a local GNU candidate, creates a temporary signing key inside each disposable container, generates signed APT/DNF/pacman repositories, and installs and exercises the package in Debian, Fedora, and Arch containers. It needs Docker, cargo-zigbuild 0.23.4, Zig 0.15.1, and nFPM 2.47.0. CI passes `PACKAGE_INPUT_DIR` pointing to the checksum-verified GNU x86-64 target bundle, so the containers install the exact direct-download package files. Without that variable, the script builds local candidates first. CI runs this against the current official container images.

## Project repository candidates

Repository scripts create local signed repositories from package artifacts. They require a secret key already present in the caller's GPG keyring. Pass its key ID as an argument; the scripts do not create or import signing identities. They require an empty destination directory and emit repository configuration/key files alongside signed metadata.

```sh
scripts/create-apt-repo.sh deb-packages repo-apt https://packages.example.invalid/apt KEY_ID stable
scripts/create-dnf-repo.sh rpm-packages repo-dnf https://packages.example.invalid/rpm KEY_ID
scripts/create-pacman-repo.sh arch-packages repo-pacman https://packages.example.invalid/arch KEY_ID
```

These examples use the reserved `.invalid` domain and do not install anything. APT also needs the exported archive key installed at `/usr/share/keyrings/bionic-reader-archive-keyring.gpg` before using the generated `.sources` file. DNF and pacman users must trust the project key through their normal system package-manager key configuration before enabling the repository. Hosting, TLS, key distribution, upgrade/uninstall tests, and verification on real supported distributions are release gates.

## Other candidate packages

The scripts create candidate inputs only; no registry publishing is performed:

```sh
python3 scripts/render-homebrew-formula.py --version VERSION --artifacts dist/candidates/vVERSION --output bionic-reader.rb
scripts/build-python-wheel.sh TARGET
scripts/package-npm.sh
scripts/build-snap-candidate.sh
```

Homebrew rendering requires the four macOS and GNU Linux archives. Python wheel builds pin maturin 1.15.0; GNU wheels target manylinux 2.28 using Zig, and musl wheels target musllinux 1.2. Wheel installation smoke checks run only when the target exactly matches the build host. npm packages are split into a small launcher and six optional native platform packages. The wrapper passes arguments, stdio, environment, signals, and exit status through to the native executable; install scripts and native downloads are not used. The Snap candidate is classic-confined and marked `devel`; Store approval is an external prerequisite to a public Snap release.

On a supported macOS or Linux host with a host-native build, `scripts/check-node-package-installs.sh target/debug/bionic-reader` creates local npm tarballs with the optional platform dependency rewritten to a `file:` candidate and installs them offline through npm, Yarn Plug'n'Play, and pnpm. It requires Node.js, npm, and Corepack 0.36.0; the lifecycle scripts remain disabled. Wheel builds exercise pip, `uv pip`, and `uv tool install` only when the wheel target exactly matches the host. These checks validate candidate installation and removal using local files. Registry naming and availability, hosted repository TLS and key distribution, upgrade paths, and external store approval remain release gates.

Run `scripts/check-frameworks.sh` to fetch the seven official framework revisions pinned in `integrations/adapters/frameworks.tsv` and exercise their real loaders. This network-dependent suite runs explicitly in CI; it is separate from the offline unit suite.

## Tool versions and references

The wheel build pins [maturin 1.15.0](https://www.maturin.rs/), [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild), Zig, and [nFPM](https://nfpm.goreleaser.com/); CI pins the exact version for each installed tool. The implementation follows the primary package-manager format documentation for [Homebrew formulae](https://docs.brew.sh/Formula-Cookbook), [APT signed repositories](https://wiki.debian.org/DebianRepository/UseThirdParty), [RPM repository metadata](https://rpm-software-management.github.io/createrepo_c/), [pacman repositories](https://wiki.archlinux.org/title/Pacman/Tips_and_tricks#Custom_local_repository), [Snap classic confinement](https://snapcraft.io/docs/reference/administration/reviewing-classic-confinement-snaps/), [Python binary wheels](https://www.maturin.rs/bindings.html), [npm platform packages](https://docs.npmjs.com/cli/v11/configuring-npm/package-json#os), and [Yarn's local file protocol](https://yarnpkg.com/protocol/file).

## Repository-only installation coverage

The [build guide](docs/BUILD.md) documents a current-checkout route for every requested package manager. No Bionic Reader registry package or GitHub release is consumed by these tests.

- Cargo checks both `--path` installation and Git-source installation from the checkout's exact commit (`scripts/check-cargo-git-install.sh`), plus extracted `.crate` installation.
- `scripts/build-local-archive.sh` builds the exact host target and respects `CARGO_TARGET_DIR`. Its archives are for installation on the build host, while `build-release.sh` retains portable release constraints.
- `scripts/check-homebrew-install.sh` consumes a host-local archive through a temporary local tap, checks formula installation, `brew test`, installed assets and CLI/PTY behavior, uninstall, and tap cleanup. With `--source`, it instead installs `Formula/bionic-reader.rb` using `--HEAD` from public `main`, as documented in the installation guide; it refuses to replace an existing `bionic-reader` Homebrew installation. The explicit tap URL lets this repository serve as the tap without a separate `homebrew-tap` repository. CI covers both routes on GNU/Linux and macOS on x86-64 and ARM64.
- Linux container checks install from signed local repositories, remove the package, install the same built `.deb`/`.rpm`/Arch archive directly as shown in the build guide, check installed assets and CLI/PTY behavior, and remove it again.
- `scripts/build-python-wheel.sh --local TARGET` builds a wheel for the exact host without Zig on GNU/Linux. It validates the wheel assets and tests pip, uv pip, and uv tool install/use/uninstall. CI exercises this route on matching macOS and GNU/Linux hosts in addition to the portable candidate builds.
- `scripts/package-npm.sh --local` emits a base tarball and one native tarball from a host archive. Its only native dependency is a local file URL. The installation checker consumes these public artifacts, installs offline with npm/Yarn Plug'n'Play/pnpm, checks ANSI output and stdin, then removes the packages. CI covers Linux and macOS.
- The Snap job runs on Ubuntu 24.04 with Snapcraft 8.14.5 (amd64 revision 17614). It builds the tracked checkout, installs the unsigned file using `--dangerous --classic`, checks the actual `/snap/bin/bionic-reader` launcher with CLI/PTY tests, then removes the Snap. Snapcraft itself runs elevated for its destructive build in this disposable runner; run the repository script as the normal runner user.
- The seven pinned framework-loader tests start fresh interactive shells, require automatic activation of exactly one reading session, and check emphasized command output. In fish, initialization uses `eval (bionic-reader init fish | string collect)` so a sourced pipeline does not replace the terminal input needed by the session.

To run the local routes, follow the commands in [docs/BUILD.md](docs/BUILD.md). Homebrew and Snap checks make actual package-manager installations and remove their own test installations; their helpers refuse to replace existing test formula/Snap installations.

## CI policy and scheduled reports

The local `rust-toolchain.toml` selects Rust 1.85.1 with rustfmt and Clippy. Use explicit `cargo +stable` or `cargo +nightly-2026-10-22` for other compilers; `rustup default` does not override the project file. `RUSTUP_TOOLCHAIN` is appropriate when a script invokes Cargo internally.

```sh
make workflow-check docs-check ci-policy-check # fast offline checks
make ci-negative-check                       # deliberate invalid inputs
make deny                                    # both dependency graphs
make sbom TARGET=aarch64-apple-darwin
make performance-report                      # advisory measurements
make mutation-report MODULE=segmentation      # two workers, 45 minutes
```

Weekly mutation campaigns cover settings, segmentation and the transformer. Surviving mutants and mutant timeouts remain advisory; failing baselines, invalid output and tool errors fail. Scheduled diagnostics have 14-day retention. Existing coverage floors (92% lines, 91% regions, 88% functions, 80% branches), memory limits and scan-work assertions remain mandatory. The 100% coverage target remains open.

Dependabot proposes weekly Cargo (application and fuzz), Actions and Python updates, grouping patch/minor changes while keeping majors separate. Merging remains manual. Native Windows CLI distribution and registry publication remain separate work. JavaScript API checks include Windows. GitHub Release preparation is documented in [docs/RELEASE.md](docs/RELEASE.md).

## Release and registry packages

[README.md](README.md#installation) contains native package-manager entry points;
[INSTALL.md](docs/INSTALL.md) covers platform requirements and alternatives, and
[BUILD.md](docs/BUILD.md) contains compilation and local packaging steps.

`scripts/package-release-node.py ARCHIVE --output DIRECTORY` creates a
self-contained native Node tarball. Run
`NODE_PACKAGE_MODE=standalone scripts/check-node-package-installs.sh BINARY`
for npm/Yarn Plug'n'Play/pnpm installation, CLI/PTY and removal checks. The
original split-package local candidate mode remains the default. Use
`NODE_PACKAGE_MODE=registry` to test the generated registry format against a
temporary loopback npm registry through all three managers; the fixture and
manager caches are cleaned up on exit.

Release preparation uses `scripts/package-registries.py` to derive seven npm
registry tarballs from verified standalone payloads, preserving the exact native
bytes and shared JS launcher. `scripts/stage-publication.py` revalidates a public
release, its current tag, complete checksum manifest and original main CI
provenance before staging isolated Homebrew/PyPI/npm publication artifacts.

The separate publishing workflow runs after publication and has independent,
opt-in jobs with scoped credentials. Setup and retry instructions are in
[RELEASE.md](docs/RELEASE.md#publishing-package-managers). No Python dispatcher is
part of the user installation flow.
