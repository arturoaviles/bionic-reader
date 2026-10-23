# Contributing

Contributions should preserve the product decisions in [docs/design.md](docs/design.md), the vocabulary in [GLOSSARY.md](GLOSSARY.md), and the architecture boundary in [docs/adr/0001-session-wide-terminal-transformation.md](docs/adr/0001-session-wide-terminal-transformation.md).

## Development checks

Use Rust 1.85.1 for the declared minimum compiler. Run the same checks as CI before submitting changes:

```sh
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
scripts/check-distribution.sh
```

The test suite includes unit, CLI, and PTY/session coverage. Session tests need the shells they exercise. Distribution checks require Bash, Zsh, fish, ShellCheck, Node.js, Python 3, Ruff, and mypy. For dependency advisories, install the pinned `cargo-audit` version listed by CI and run `cargo audit` when its advisory database is reachable. A missing tool is a local environment setup issue; CI installs the required checkers explicitly.

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
