# Installation and updates

Use your package manager directly. GitHub Releases carry binaries, Linux system
packages, Python wheels and Node packages; Homebrew/PyPI/npm publication is
performed after a release is published and its channel is enabled.

Homebrew installs prebuilt binaries from the public tap. Python, Node, and
Cargo packages are published on PyPI, npm, and crates.io. See
[publication setup](RELEASE.md#publishing-package-managers) for maintainer requirements.

## Homebrew

On macOS or GNU/Linux, add and trust the public
[Homebrew tap](https://github.com/arturoaviles/homebrew-tap), then install:

```sh
brew tap arturoaviles/tap
brew trust arturoaviles/tap
brew install bionic-reader
```

Verify the installation:

```sh
brew test bionic-reader
bionic-reader --version
```

The formula selects the matching CPU/OS archive and verifies its digest. Rust
and `--HEAD` are unnecessary for this prebuilt installation. Add and trust the
tap only once; package commands then use just `bionic-reader`.

To update:

```sh
brew update
brew upgrade bionic-reader
```

To remove the installation:

```sh
brew uninstall bionic-reader
brew untap arturoaviles/tap
```

### Build from main

See [Homebrew source installation](BUILD.md#homebrew-from-main).

### Install a release formula locally

You can also install a GitHub release's `bionic-reader.rb` formula and its native
archives using a local tap:

```sh
brew tap-new arturoaviles/bionic-reader-local
curl --fail --location \
  https://github.com/arturoaviles/bionic-reader/releases/latest/download/bionic-reader.rb \
  --output "$(brew --repository arturoaviles/bionic-reader-local)/Formula/bionic-reader.rb"
brew install bionic-reader
```

This downloads prebuilt binaries; `--build-from-source` is unnecessary. Local
taps do not update from a remote; download the next formula before reinstalling.

## Linux release packages

Each published release includes stable asset names so the package manager's own
architecture query can choose the file. The version remains in the package
metadata and release tag. No architecture needs to be entered manually.

```sh
# Debian / Ubuntu
curl --fail --location --output bionic-reader.deb \
  "https://github.com/arturoaviles/bionic-reader/releases/latest/download/bionic-reader-linux-$(dpkg --print-architecture).deb"
sudo apt install ./bionic-reader.deb

# Fedora
curl --fail --location --output bionic-reader.rpm \
  "https://github.com/arturoaviles/bionic-reader/releases/latest/download/bionic-reader-linux-$(rpm --eval '%{_arch}').rpm"
sudo dnf install --nogpgcheck ./bionic-reader.rpm

# Arch Linux / compatible ARM64 distributions
curl --fail --location --output bionic-reader.pkg.tar.zst \
  "https://github.com/arturoaviles/bionic-reader/releases/latest/download/bionic-reader-linux-$(uname -m).pkg.tar.zst"
sudo pacman -U ./bionic-reader.pkg.tar.zst
```

These are local package-file installs, not hosted APT/DNF/pacman repositories.
RPM and Arch files are unsigned; verify download checksums/provenance as
[documented here](RELEASE.md#verify-release-provenance). Creating a signed hosted
repository requires an existing publisher key and hosting; the tested builders
remain documented in [CONTRIBUTING.md](../CONTRIBUTING.md#project-repository-candidates).
ARM64 Arch files require an Arch-derived distribution that supports ARM64.

## pip and uv

Install from PyPI:

```sh
# Use pip in an activated virtual environment.
pip install bionic-reader
pip install --upgrade bionic-reader

# Or use uv's isolated tool environment.
uv tool install bionic-reader
uv tool upgrade bionic-reader
```

Both select compatible wheels automatically. No Rust compilation is required for
the six supported wheel platforms. For an unpublished registry channel, download
a matching wheel from GitHub Releases and pass its local filename to
`pip install ./bionic_reader-*.whl` or `uv tool install ./bionic_reader-*.whl`.
Put only the chosen wheel in that directory. Native package selection is normally
the responsibility of pip/uv; do not use a separate Python installer.

## npm, Yarn and pnpm

Install from npm:

```sh
npm install --global @arturoaviles/bionic-reader
pnpm add --global @arturoaviles/bionic-reader

# Yarn 4 uses project dependencies rather than a global-add command.
yarn add @arturoaviles/bionic-reader
yarn bionic-reader --help
```

The registry package selects the matching optional native dependency using
OS/CPU/libc package metadata. The JS launcher forwards arguments, stdio, signals
and exit status; installation has no lifecycle scripts. Node.js 18+ is required;
CI uses Node.js 24.11.1, Yarn 4.18.1 and pnpm 12.14.0. npm/pnpm global installations
need their configured bin directory on PATH. Yarn commands run in the project
where you added the dependency.

Release assets also retain standalone target-specific `.npm.tgz` files. They can
be passed directly to `npm install`, `yarn add` or `pnpm add` when working offline
or before registry publication. See [BUILD.md](BUILD.md) for local packaging.
GitHub Packages is an optional second npm registry; it requires GitHub registry
authentication even for public packages. Public npm is the normal installation
route; do not silently change a user's global registry configuration.

## Cargo

Install from crates.io:

```sh
cargo install bionic-reader --locked
```

Rust 1.85.1+ and a C compiler/linker are required. Cargo selects the latest
published package; keep `--locked` to use its validated dependency lockfile.
See [BUILD.md](BUILD.md) for source installations and local packages.

## Snap

Snap Store publication is not enabled. The [Snap guide](BUILD.md#snap-ubuntu-2404)
contains the repository package workflow.

## Native archives

Download an archive from [GitHub Releases](https://github.com/arturoaviles/bionic-reader/releases),
verify its checksum, and extract it. Put its `bin` directory on PATH, keeping its
`share/bionic-reader` assets for completions, configuration and framework adapters.
GNU builds require glibc 2.28; musl builds/wheels target musl Linux (wheel baseline
1.2). macOS builds target 11.0 but are tested on macOS 15; minimum-version runtime
validation and signing/notarization remain release gates. Windows is unsupported.

## Updates

Use the same manager for updates. Direct Linux package-file installs require
downloading the next release and rerunning the manager command. `cargo install
bionic-reader --locked --force` updates a crates.io installation. For Snap, see
the [build guide](BUILD.md#snap-ubuntu-2404). Installations never activate reading sessions or edit startup files;
see [SHELL.md](SHELL.md) for explicit shell setup.

## Uninstall

Use the route matching your installation:

- Homebrew: `brew uninstall bionic-reader`, then `brew untap arturoaviles/tap`. For a source installation, untap `arturoaviles/bionic-reader` instead.
- Archive: remove the extracted archive directory and the PATH entry you added for its `bin` directory.
- Debian / Ubuntu: `sudo apt remove bionic-reader`.
- Fedora: `sudo dnf remove bionic-reader`.
- Arch Linux: `sudo pacman -R bionic-reader`.
- uv: `uv tool uninstall bionic-reader`.
- pip: `pip uninstall bionic-reader` in its virtual environment.
- npm: `npm uninstall --global @arturoaviles/bionic-reader`.
- Yarn: `yarn remove @arturoaviles/bionic-reader` in the project.
- pnpm: `pnpm remove --global @arturoaviles/bionic-reader`.
- Snap: `sudo snap remove bionic-reader`.
- Cargo/source builds: `cargo uninstall bionic-reader`; other local build routes have removal commands in [BUILD.md](https://github.com/arturoaviles/bionic-reader/blob/main/docs/BUILD.md).

Remove the standalone initialization line or the framework activation entry you added, together with the copied adapter. For Fisher, run `fisher remove "$HOME/.local/share/bionic-reader/fisher"` before deleting its local plugin directory. Remove any manually installed completions.
