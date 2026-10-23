# Contributing

Contributions should preserve the product decisions in [docs/design.md](docs/design.md), the vocabulary in [GLOSSARY.md](GLOSSARY.md), and the architecture boundary in [docs/adr/0001-session-wide-terminal-transformation.md](docs/adr/0001-session-wide-terminal-transformation.md).

Rust code follows [Google's Rust style guide](https://google.github.io/styleguide/rustguide.html). Read [the project's Rust style policy](docs/rust-style.md) for its application to this Cargo CLI, documented exceptions, and automated enforcement.

## Development checks

Use Rust 1.85.1 for the declared minimum compiler. Run the same checks as CI before submitting changes:

```sh
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo test --locked --all-targets
cargo test --locked --doc
scripts/check-distribution.sh
```

The test suite includes unit, CLI, and PTY/session coverage. Session tests need the shells they exercise. Distribution checks require Bash, Zsh, fish, ShellCheck, Node.js, Python 3, Ruff, and mypy. For dependency advisories, install the pinned `cargo-audit` version listed by CI and run `cargo audit` when its advisory database is reachable. A missing tool is a local environment setup issue; CI installs the required checkers explicitly.

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

Workflow changes are checked with [actionlint](https://github.com/rhysd/actionlint) 1.7.12, including GitHub expression contexts and inline shell scripts. Run `actionlint` locally when editing `.github/workflows/`. Actions are pinned to verified commit SHAs. CI has read-only repository permissions, timeouts, and cancellation of superseded runs. The separate dependency-security workflow runs Cargo audit on dependency changes, weekly after merge, and on manual dispatch.

## Secret scanning

The separate secret-scanning workflow runs Gitleaks 8.30.1 on every pull request, pushes to `main`, weekly after merge, and manual dispatch. It fetches the full Git history, scans all fetched refs, redacts detected values, and fails when a secret is found. The Linux binary is pinned by version and SHA-256; the workflow uses read-only repository permissions.

With the same [Gitleaks release](https://github.com/gitleaks/gitleaks/releases/tag/v8.30.1) installed, reproduce the scan locally:

```sh
gitleaks git --log-opts="--all" --redact=100 --no-banner --exit-code=1 .
```

## Cargo crate validation

Cargo is a first-release distribution route through crates.io. The manifest includes the Rust source, embedded shell integrations, framework adapters, completions, configuration example, license, README, tests, and their support files. Its explicit file list excludes developer state and unrelated distribution tooling.

From a clean checkout, run:

```sh
scripts/check-cargo-package.sh
```

This runs Cargo's verified `package --locked`, checks the archive contents, extracts the `.crate` to a temporary directory, installs it with Cargo, exercises the installed binary and a reading session, and uninstalls it. The archive remains under Cargo's `target/package/` directory (or the target directory selected by `CARGO_TARGET_DIR`). CI covers the same five OS/architecture/toolchain combinations as source installation and runs the full test suite against the packaged source on Linux x86-64. That job also uploads the `.crate` as a workflow artifact.

Before publishing a release, check crates.io name ownership and credentials, then run the final preflight:

```sh
cargo publish --dry-run --locked
```

The dry run does not upload the crate. No workflow publishes it automatically. Source/Git installation works before registry publication; `cargo install bionic-reader --locked` requires an actual crates.io release. See the official [Cargo package](https://doc.rust-lang.org/cargo/commands/cargo-package.html), [install](https://doc.rust-lang.org/cargo/commands/cargo-install.html), and [publish](https://doc.rust-lang.org/cargo/commands/cargo-publish.html) documentation.

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

With no arguments it attempts all targets and stops if the current host lacks the required build support. Build macOS artifacts on matching Intel and Apple Silicon macOS runners. GNU Linux candidates use cargo-zigbuild 0.23.4 with Zig 0.15.1 and are checked for GLIBC symbol versions above 2.28. Musl candidates need a matching Linux host with `musl-gcc`. `SOURCE_DATE_EPOCH` can be set to reproduce archive timestamps; the default comes from the current Git commit. Candidate archives and checksums are written under `dist/candidates/v<VERSION>/`.

`scripts/package-linux.sh TARGET` converts a built GNU candidate into deb, rpm, and Arch Linux packages with nFPM 2.47.0. It includes the README, license, config example, shell integration/framework adapters, and shell completions; it does not run installation scripts or edit user shell configuration. `scripts/package-manager-install-smoke.sh` builds a local GNU candidate, creates a temporary signing key inside each disposable container, generates signed APT/DNF/pacman repositories, and installs and exercises the package in Debian, Fedora, and Arch containers. It needs Docker, cargo-zigbuild 0.23.4, Zig 0.15.1, and nFPM 2.47.0. CI runs this against the current official container images.

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

On a supported macOS or Linux host with a host-native build, `scripts/check-node-package-installs.sh target/debug/bionic-reader` creates local npm tarballs with the optional platform dependency rewritten to a `file:` candidate and installs them offline through npm, Yarn Plug'n'Play, and pnpm. It requires Node.js, npm, and Corepack 0.36.0; the lifecycle scripts remain disabled. Wheel builds exercise pip, `uv pip`, and `uv tool install` only when the wheel target exactly matches the host. These checks validate candidate installation from local files. Registry naming and availability, hosted repository TLS and key distribution, upgrade/uninstall paths, and external store approval remain release gates.

Run `scripts/check-frameworks.sh` to fetch the seven official framework revisions pinned in `integrations/adapters/frameworks.tsv` and exercise their real loaders. This network-dependent suite runs explicitly in CI; it is separate from the offline unit suite.

## Tool versions and references

The wheel build pins [maturin 1.15.0](https://www.maturin.rs/), [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild), Zig, and [nFPM](https://nfpm.goreleaser.com/); CI pins the exact version for each installed tool. The implementation follows the primary package-manager format documentation for [Homebrew formulae](https://docs.brew.sh/Formula-Cookbook), [APT signed repositories](https://wiki.debian.org/DebianRepository/UseThirdParty), [RPM repository metadata](https://rpm-software-management.github.io/createrepo_c/), [pacman repositories](https://wiki.archlinux.org/title/Pacman/Tips_and_tricks#Custom_local_repository), [Snap classic confinement](https://snapcraft.io/docs/reference/administration/reviewing-classic-confinement-snaps/), [Python binary wheels](https://www.maturin.rs/bindings.html), [npm platform packages](https://docs.npmjs.com/cli/v11/configuring-npm/package-json#os), and [Yarn's local file protocol](https://yarnpkg.com/protocol/file).
