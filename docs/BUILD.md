# Build Bionic Reader

These instructions are for contributors and users who want to compile Bionic Reader or create local packages. For Homebrew, apt, dnf, pacman, pip, uv, npm, Yarn, pnpm, Cargo installation entry points, see the [README](../README.md#installation). Native package managers select supported architectures automatically. Rust and build tools are required only for source builds.

## Regenerate the embedded Go engine

Go consumers install the module from the registry without these tools. Contributors
changing Rust engine code or the Cargo version regenerate the committed payload
with Rust 1.85.1, Python 3.11+, and the WASI target:

```sh
rustup target add wasm32-wasip1 --toolchain 1.85.1
bash scripts/build-go-engine.sh
bash scripts/build-go-engine.sh --check
go test ./...
go test -race ./...
```

The script builds the private `bindings/go` adapter from locked dependencies,
normalizes source paths, and updates `internal/engine/engine.wasm` and
`engine_version.go`. Commit both generated files with the source change. CI
rebuilds on Linux and requires exact byte equality, then tests Go consumers on
Linux, macOS, and Windows with CGO disabled. Race checks use CGO only for Go's
instrumentation. The module's compiler/runtime requirements are in `go.mod`.

## Build and install from source

### 1. Install the prerequisites

Use macOS or Linux with Git, Rust **1.85.1 or newer**, Cargo, and a native C compiler/linker. On macOS, install Xcode Command Line Tools if they are missing; on Linux, install your distribution's C build tools. Follow the [official Rust installation instructions](https://rust-lang.org/tools/install/) to install Rust and Cargo with rustup. Open a new terminal afterward and verify:

```sh
git --version
rustc --version
cargo --version
```

### 2. Clone the repository

```sh
git clone https://github.com/arturoaviles/bionic-reader.git
cd bionic-reader
```

The repository is public; cloning over HTTPS does not require GitHub authentication.

### 3. Build and install

This is the Cargo route. For another package manager, jump to [repository package installations](#install-with-the-other-package-managers-from-this-repository) after cloning.

Run from the cloned repository:

```sh
cargo install --path . --locked
```

Cargo compiles the native executable and normally installs it in `~/.cargo/bin`. If you use a custom `CARGO_HOME` or installation root, use its `bin` directory in the PATH instructions below. No Bionic Reader package registry is involved.

### 4. Make the executable available to your shell

For **Bash or Zsh**, run:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

If that directory is not already on PATH, also add the line near the top of `~/.bashrc` or `~/.zshrc`, before framework loading or Bionic Reader initialization.

For **fish**, run once:

```fish
fish_add_path $HOME/.cargo/bin
```

### 5. Verify the installation

```sh
bionic-reader --version
bionic-reader --help
bionic-reader -t 'Reading from a source installation.'
```

You can also try the checkout without installing it:

```sh
cargo run --locked -- -t 'Try Bionic Reader before installing.'
```

Cargo installs the executable. To activate automatic reading sessions, follow either [standalone shell setup](SHELL.md#standalone-shell-setup) or [framework setup](SHELL.md#shell-framework-setup). Neither installation nor activation requires publishing a package.

To update a development checkout, pull `main` and rerun `cargo install --path . --locked --force`. For a reproducible version, check out its release tag before building.

## Install with Cargo

Cargo is supported on macOS and Linux with Rust 1.85.1 or newer and the C build tools listed above. The source installation in the steps above uses Cargo already. To install directly from the public Git repository without a separate checkout, run:

```sh
cargo install --git https://github.com/arturoaviles/bionic-reader.git --locked bionic-reader
```

This installs from `main`. To build a particular release, add `--tag vX.Y.Z`. Then follow [the PATH instructions](#4-make-the-executable-available-to-your-shell) and [verify the installation](#5-verify-the-installation). Git installation installs the executable; clone the repository if you also want to copy framework adapters or completion files as described in the [shell guide](SHELL.md#shell-framework-setup). Shell initialization and completion generation are built into the executable.

Use `cargo uninstall bionic-reader` to remove a Cargo installation. Keep `--locked` on installs so Cargo uses the dependencies validated with that version. Maintainers can build and verify the source archive with `cargo package --locked`; see [Cargo crate validation](../CONTRIBUTING.md#cargo-crate-validation).

## Install with the other package managers from this repository

Choose one route below after cloning. The packaging helpers require Python 3.9 or newer; Linux users also need their distribution's Python venv support for the Python build-tool environments. Run build commands from the repository root in Bash or Zsh. These routes build the current checkout and install local files; they do not require a Bionic Reader registry package or GitHub release. Upstream build tools and Rust dependencies still need network access on their first installation.

Set the checkout's version and native target once:

```sh
br_repo="$PWD"
br_target=$(rustc -vV | sed -n 's/^host: //p')
br_version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "bionic-reader"))')
```

### Homebrew

Requires Homebrew and the Rust/C build prerequisites above. On macOS or GNU/Linux, build a host-local archive and register a local tap:

```sh
scripts/build-local-archive.sh
brew tap-new arturoaviles/bionic-reader-local
br_tap=$(brew --repository arturoaviles/bionic-reader-local)
python3 scripts/render-homebrew-formula.py \
  --version "$br_version" --artifacts "$br_repo/dist/local" \
  --local-target "$br_target" --output "$br_tap/Formula/bionic-reader.rb"
brew install --build-from-source arturoaviles/bionic-reader-local/bionic-reader
HOMEBREW_DEVELOPER=1 brew test arturoaviles/bionic-reader-local/bionic-reader
bionic-reader --style always -t 'Reading with a local Homebrew installation.'
```

Create the tap only on the first installation; reuse it when rebuilding. The formula uses the local archive's `file://` URL and SHA-256. Keep the checkout/archive for reinstalls. To remove this installation:

```sh
brew uninstall arturoaviles/bionic-reader-local/bionic-reader
brew untap arturoaviles/bionic-reader-local
```

### Homebrew from main

To build the latest source instead of installing a prebuilt release, use this
repository as a separate tap:

```sh
brew tap arturoaviles/bionic-reader https://github.com/arturoaviles/bionic-reader.git
brew trust arturoaviles/bionic-reader
brew install --HEAD bionic-reader
```

This installs Rust as a build dependency and compiles `main` using the checked-in
`Cargo.lock`. On macOS, Xcode Command Line Tools are also required. Update this
installation with `brew update` followed by `brew upgrade --fetch-HEAD bionic-reader`.
To switch to the prebuilt channel, run `brew uninstall bionic-reader` and
`brew untap arturoaviles/bionic-reader`, then follow the tap installation above.

### apt, dnf, and pacman: build local system packages

On GNU/Linux, install [Go 1.26.4 or newer](https://go.dev/doc/install) and the Rust/C prerequisites first. The following prepares the pinned packaging tools, then builds the checkout into local `.deb`, `.rpm`, and `.pkg.tar.zst` files. The Python environment is for build tools, separate from the installed application:

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install ziglang==0.15.1
export PATH="$(python -c 'from pathlib import Path; import ziglang; print(Path(ziglang.__file__).parent)'):$(go env GOPATH)/bin:$PATH"
rustup toolchain install stable --profile minimal
cargo +stable install cargo-zigbuild --version 0.23.4 --locked
go install -ldflags='-X main.version=2.47.0' github.com/goreleaser/nfpm/v2/cmd/nfpm@v2.47.0
scripts/build-release.sh "$br_target"
br_packages="$br_repo/dist/local/linux-$br_version-$br_target"
scripts/package-linux.sh "$br_target" "$br_packages"
```

Choose the command for your distribution. These paths identify files you just built, without adding a hosted package repository.

### apt (Debian / Ubuntu)

```sh
sudo apt install "$br_packages"/bionic-reader*.deb
bionic-reader --style always -t 'Reading with a local apt installation.'
```

To remove this installation later, run `sudo apt remove bionic-reader`.

### dnf (Fedora)

```sh
sudo dnf install --nogpgcheck "$br_packages"/bionic-reader*.rpm
bionic-reader --style always -t 'Reading with a local dnf installation.'
```

To remove this installation later, run `sudo dnf remove bionic-reader`.

`--nogpgcheck` applies here to the unsigned RPM built from your own checkout. Signed repository installation is tested separately in CI.

### pacman (Arch Linux)

```sh
sudo pacman -U "$br_packages"/bionic-reader*.pkg.tar.zst
bionic-reader --style always -t 'Reading with a local pacman installation.'
```

To remove this installation later, run `sudo pacman -R bionic-reader`.

### Snap (Ubuntu 24.04)

Requires Ubuntu 24.04, an active snapd service, sudo, and the Rust/C prerequisites. Install the pinned **Snapcraft build tool**, then build and install Bionic Reader from the checkout:

```sh
sudo apt update
sudo apt install snapd
sudo systemctl enable --now snapd.socket
sudo snap wait system seed.loaded
# Snapcraft 8.14.5 builder: amd64 revision 17614, arm64 revision 17616.
# On amd64:
sudo snap install snapcraft --classic --revision=17614
sudo snap install core24
# On arm64, use --revision=17616 instead.
export PATH="/snap/bin:$PATH"
scripts/build-snap-candidate.sh "$br_repo/dist/local/snap"
sudo snap install --dangerous --classic "$br_repo"/dist/local/snap/bionic-reader_*.snap
/snap/bin/bionic-reader --style always -t 'Reading with a local Snap installation.'
```

To remove this installation later, run `sudo snap remove bionic-reader`.

`--dangerous` permits the unsigned file you built locally; `--classic` enables the terminal/session behavior. Run the build script as your normal user: it requests sudo only for Snapcraft's host build. No Bionic Reader Store installation or publisher approval is needed for this local route.

### pip

On macOS or Linux, build the wheel from the checkout with Python 3.11 or newer, maturin 1.15.0, and uv 0.13.5 (the build script also runs pip/uv installation checks):

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install maturin==1.15.0 uv==0.13.5
scripts/build-python-wheel.sh --local "$br_target"
python3 -m venv "$HOME/.local/share/bionic-reader/pip-env"
"$HOME/.local/share/bionic-reader/pip-env/bin/python" -m pip install \
  --no-index --no-deps "$br_repo"/dist/local/python/bionic_reader-*.whl
export PATH="$HOME/.local/share/bionic-reader/pip-env/bin:$PATH"
bionic-reader --style always -t 'Reading with a local pip installation.'
```

Add the PATH line to your shell configuration to retain it. Remove this installation with `"$HOME/.local/share/bionic-reader/pip-env/bin/python" -m pip uninstall bionic-reader`.

### uv

Build the local wheel using the pip preparation/build commands above, then install it as a persistent tool:

```sh
uv tool install --no-index "$br_repo"/dist/local/python/bionic_reader-*.whl
uv tool update-shell
# Open a new terminal, or use the installed command directly:
"$(uv tool dir --bin)/bionic-reader" --style always -t 'Reading with a local uv installation.'
```

To remove this installation later, run `uv tool uninstall bionic-reader`.

The wheel contains the native Python library, executable, configuration example, integrations, and completions. See [library usage](LIBRARY.md#python) for imports. Host-local wheels are built for this machine; portable release-wheel builds are a separate maintainer workflow.

### npm

Requires Node.js 22 or newer and npm. Build the shared WebAssembly payload with wasm-bindgen 0.2.129 first (see [library builds](#build-and-validate-library-packages)), then build a host-local archive and create the local Node tarballs:

```sh
scripts/build-local-archive.sh
scripts/package-npm.sh --local
br_node_tarball="$br_repo/dist/local/npm/bionic-reader-base-v$br_version.tgz"
npm install --global --prefix "$HOME/.local" --ignore-scripts --no-audit --no-fund "$br_node_tarball"
export PATH="$HOME/.local/bin:$PATH"
bionic-reader --style always -t 'Reading with a local npm installation.'
```

To remove this installation later, run `npm uninstall --global --prefix "$HOME/.local" @arturoaviles/bionic-reader`.

This uses a user-writable npm prefix. Add its PATH line to your shell configuration. The base tarball points to one matching native tarball using an absolute `file://` dependency. Keep both tarballs in their generated directory; recreate them after moving the checkout or for another machine. No lifecycle install scripts or native downloads are used.

### Yarn

Build the Node tarballs using the npm preparation/build commands above. Install Corepack for the pinned Yarn/pnpm versions (only a build/runtime tool comes from its registry):

```sh
npm install --global --prefix "$HOME/.local" --ignore-scripts --no-audit --no-fund corepack@0.36.0
export PATH="$HOME/.local/bin:$PATH"
corepack enable --install-directory "$HOME/.local/bin"
```

With Yarn 4.18.1, install into a new local tools project; this supports Yarn Plug'n'Play:

```sh
mkdir -p "$HOME/.local/share/bionic-reader/yarn-tools"
cd "$HOME/.local/share/bionic-reader/yarn-tools"
printf '%s\n' '{"name":"bionic-reader-tools","private":true,"packageManager":"yarn@4.18.1"}' > package.json
printf '%s\n' 'nodeLinker: pnp' > .yarnrc.yml
yarn add "@arturoaviles/bionic-reader@file:$br_node_tarball"
yarn run bionic-reader --style always -t 'Reading with a local Yarn installation.'
cd "$br_repo"
```

Run `yarn run bionic-reader` from this project. To make the command available elsewhere and to shell/framework initialization, create a launcher:

```sh
mkdir -p "$HOME/.local/bin"
cat > "$HOME/.local/bin/bionic-reader" <<EOF
#!/bin/sh
exec yarn --cwd "$HOME/.local/share/bionic-reader/yarn-tools" run bionic-reader "\$@"
EOF
chmod +x "$HOME/.local/bin/bionic-reader"
export PATH="$HOME/.local/bin:$PATH"
```

Keep the tools project and tarball directory. Add the PATH line to your shell configuration. Remove the package later with `yarn --cwd "$HOME/.local/share/bionic-reader/yarn-tools" remove @arturoaviles/bionic-reader`, then remove the launcher you created. Choose one package-manager launcher at a time.

### pnpm

Build the Node tarballs using the npm preparation/build commands above and prepare Corepack as shown under Yarn. With pnpm 12.14.0, install into a new local tools project:

```sh
mkdir -p "$HOME/.local/share/bionic-reader/pnpm-tools"
cd "$HOME/.local/share/bionic-reader/pnpm-tools"
printf '%s\n' '{"name":"bionic-reader-tools","private":true,"packageManager":"pnpm@12.14.0"}' > package.json
pnpm add "$br_node_tarball"
pnpm exec bionic-reader --style always -t 'Reading with a local pnpm installation.'
cd "$br_repo"
```

Run `pnpm exec bionic-reader` from this project. For general use and shell/framework initialization, create a launcher:

```sh
mkdir -p "$HOME/.local/bin"
cat > "$HOME/.local/bin/bionic-reader" <<EOF
#!/bin/sh
exec pnpm --dir "$HOME/.local/share/bionic-reader/pnpm-tools" exec bionic-reader "\$@"
EOF
chmod +x "$HOME/.local/bin/bionic-reader"
export PATH="$HOME/.local/bin:$PATH"
```

Keep the tools project and tarball directory. Add the PATH line to your shell configuration. Remove the package later with `pnpm --dir "$HOME/.local/share/bionic-reader/pnpm-tools" remove @arturoaviles/bionic-reader`, then remove the launcher you created. It shares its command name with the other installation routes.

To update an unpublished build, pull the chosen Git branch, rebuild its local artifact, and remove/reinstall using the corresponding commands above. Installation does not activate shell integration; follow the shell/framework instructions in the [shell guide](SHELL.md#standalone-shell-setup) after putting your chosen executable or launcher on PATH.

## Build and validate library packages

The Vite examples require Node 22.12 or newer. Build tools are pinned separately
from runtime minimums. Use maturin 1.15.0 for Python, wasm-bindgen 0.2.129 for
JavaScript, and the checked-in Cargo lockfile.
Portable Linux wheels use Zig 0.15.1 so their runtime libraries match GNU or
musl targets; host-local GNU wheels can use the native toolchain.
Install the `wasm32-unknown-unknown` Rust target, put the pinned wasm-bindgen CLI
on PATH (or set `WASM_BINDGEN`), then run:

```sh
scripts/build-library-wasm.sh
br_version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "bionic-reader"))')
python3 scripts/package-library-candidate.py --version "$br_version" --output dist/library-api.tgz
npm ci --ignore-scripts --no-audit --no-fund
npm run check:node -- dist/library-api.tgz
npm run check:example -- browser dist/library-api.tgz
npm run check:example -- react dist/library-api.tgz
npm run check:example -- next dist/library-api.tgz
```

The API-only tarball is an internal test candidate: it omits native CLI optional
dependencies so Windows can test the public JavaScript API before publication.
Release npm packages retain the native CLI dependencies. Build the shared Wasm
payload before running `scripts/package-npm.sh --local`; see [npm packaging](#npm).
For browser checks, install Playwright's Chromium first (`npx playwright install
chromium`), or set `BIONIC_BROWSER=chromium BIONIC_BROWSER_CHANNEL=chrome` to
use installed Chrome. The example checkers create isolated consumers and clean
their own servers and directories.
Set `BIONIC_REACT_VERSION=18.3.1` and `BIONIC_REACT_TYPES=18.3.31` for the React 18
consumer checks. See CI for pinned React 19 and framework versions.

To use the checkout directly as a Rust dependency, adjust this path in the
consumer application's `Cargo.toml`:

```toml
[dependencies]
bionic-reader = { path = "../bionic-reader", default-features = false, features = ["config"] }
```

## Development and portable candidates

See [CONTRIBUTING.md](../CONTRIBUTING.md) for checks, pinned packaging tools, and portable candidate builds. Host-local packages above are intended for the build machine. See [RELEASE.md](RELEASE.md) for versioning and GitHub Release preparation.
