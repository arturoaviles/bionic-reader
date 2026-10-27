# Bionic Reader

Bionic Reader is a local Rust command-line program that emphasizes the beginnings of words in text streams and opt-in interactive shell sessions. It transforms text on your machine; it does not send text to a service. It preserves the input text and layout and does not promise a reading-speed improvement.

**Install from this GitHub repository.** Use [Homebrew](#install-with-homebrew) to build directly from `main`, or use the checkout and local packages below. Bionic Reader is unpublished; no Bionic Reader registry package or GitHub release is required.

## Install with Homebrew

On macOS or GNU/Linux with Homebrew installed, use this repository as the tap:

```sh
brew tap arturoaviles/bionic-reader https://github.com/arturoaviles/bionic-reader.git
brew install --HEAD arturoaviles/bionic-reader/bionic-reader
brew test arturoaviles/bionic-reader/bionic-reader
bionic-reader --version
```

Homebrew installs Rust as a build dependency and compiles the current `main` source with the checked-in `Cargo.lock`. On macOS, Xcode Command Line Tools are also required. `--HEAD` is required until a stable release is published. The explicit repository URL is required because this repository does not use Homebrew's default `homebrew-` naming convention.

If `brew install arturoaviles/tap/bionic-reader` fails with `Repository not found`, use the commands above. That older command points to the unavailable `arturoaviles/homebrew-tap` repository.

To update or remove the installation:

```sh
brew update
brew upgrade --fetch-HEAD arturoaviles/bionic-reader/bionic-reader
# To uninstall:
brew uninstall arturoaviles/bionic-reader/bionic-reader
brew untap arturoaviles/bionic-reader
```

Then follow [standalone shell setup](#standalone-shell-setup) or [framework setup](#shell-framework-setup) to activate reading sessions. To install a local checkout instead, use the [local Homebrew archive route](#homebrew) below.

## How to install

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

Cargo installs the executable. To activate automatic reading sessions, follow either [standalone shell setup](#standalone-shell-setup) or [framework setup](#shell-framework-setup) below. Neither installation nor activation requires publishing a package.

To update later, pull `main` and rerun `cargo install --path . --locked --force`.

## Install with Cargo

Cargo is supported on macOS and Linux with Rust 1.85.1 or newer and the C build tools listed above. The source installation in the steps above uses Cargo already. To install directly from the public Git repository without a separate checkout, run:

```sh
cargo install --git https://github.com/arturoaviles/bionic-reader.git --locked bionic-reader
```

This installs from `main`. Then follow [the PATH instructions](#4-make-the-executable-available-to-your-shell) and [verify the installation](#5-verify-the-installation). Git installation installs the executable; clone the repository if you also want to copy framework adapters or completion files as described below. Shell initialization and completion generation are built into the executable.

Use `cargo uninstall bionic-reader` to remove a Cargo installation. Keep `--locked` on installs so Cargo uses the dependencies validated with that version. Maintainers can build and verify the source archive with `cargo package --locked`; see [Cargo crate validation](CONTRIBUTING.md#cargo-crate-validation).

## Install with the other package managers from this repository

Choose one route below after cloning the repository above. The packaging helpers require Python 3.9 or newer; Linux users also need their distribution's Python venv support for the Python build-tool environments. Run build commands from the repository root in Bash or Zsh. These routes build the current checkout and install local files; they do not require a Bionic Reader registry package or GitHub release. Upstream build tools and Rust dependencies still need network access on their first installation.

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

On macOS or Linux, build the wheel from the checkout with Python 3.9 or newer, maturin 1.15.0, and uv 0.13.5 (the build script also runs pip/uv installation checks):

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
uv tool install --no-index --no-deps "$br_repo"/dist/local/python/bionic_reader-*.whl
uv tool update-shell
# Open a new terminal, or use the installed command directly:
"$(uv tool dir --bin)/bionic-reader" --style always -t 'Reading with a local uv installation.'
```

To remove this installation later, run `uv tool uninstall bionic-reader`.

The wheel contains the executable, configuration example, integrations, and completions. Host-local wheels are built for this machine; portable release-wheel builds are a separate maintainer workflow.

### npm

Requires Node.js 24.11.1 or newer and npm. Build a host-local archive, then create the local Node tarballs:

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

To update an unpublished build, pull the chosen Git branch, rebuild its local artifact, and remove/reinstall using the corresponding commands above. Installation does not activate shell integration; follow the shell/framework instructions below after putting your chosen executable or launcher on PATH.

## Usage examples

Pass one string with `-t` / `--text`, or pipe text on standard input:

```sh
bionic-reader -t 'Bionic reading keeps the original words.'
cat article.txt | bionic-reader
printf 'A stream can be transformed.\n' | bionic-reader --style always

# Emphasize 40% of every second eligible word of at least four letters.
bionic-reader -t 'Read the first part of selected words.' --fraction 0.4 --every 2 --min-word-length 4

# Keep ANSI emphasis when saving output or sending it through another pipe.
bionic-reader --style always -t 'Save the emphasized text.' > reading.ansi

# Preserve the input without adding emphasis.
printf 'Original text\n' | bionic-reader --style never
```

The default `--style auto` adds emphasis only on terminal stdout, honoring nonempty `NO_COLOR` and `TERM=dumb`. `--style always` explicitly overrides these hints; `--style never` adds no emphasis. For ANSI output, the default safe policy keeps recognized presentation controls and removes other terminal controls. Use `--controls preserve` or `--input-format terminal` only for trusted ANSI input; plain output always removes controls. With no `-t`, an interactive standard input shows help instead of waiting for text.

Tune word selection with `--fraction`, `--every`, and `--min-word-length`:

```sh
bionic-reader -t 'Read the first part of each word.' --fraction 0.4 --every 2
```

`fraction` is a finite value from `0` to `1`; `every` and `min-word-length` must be positive integers. The default fraction is `0.5`, default frequency is `1`, and default minimum length is `2`. Word boundaries use ICU4X Unicode word segmentation and its automatic complex-script data. Emphasis boundaries use extended grapheme clusters, so accents and emoji sequences stay intact. Only letter graphemes count toward fixation length; digits and emoji receive no added emphasis. Internal apostrophes stay within words, while hyphens separate segments. Automatic segmentation is not a promise of perfect linguistic analysis for every language; the library accepts custom segmenters.

## Document formats and safe input

```sh
# Preserve Markdown headings, lists, links, and code while adding terminal emphasis.
bionic-reader --input-format markdown --style always < article.md

# Export Markdown emphasis or escaped HTML. Code is protected by default.
bionic-reader --input-format markdown --format markdown < article.md > emphasized.md
bionic-reader --input-format markdown --format html < article.md > article.html

# Remove terminal escapes without adding emphasis; preserve the remaining source text.
bionic-reader --format plain < colored-output.txt > plain.txt

# Explicitly preserve all controls in trusted terminal output.
command-producing-terminal-output | bionic-reader --input-format terminal
```

`--format ansi|plain|markdown|html` selects a representation; `--style` controls
whether emphasis is added. Markdown/HTML exports add emphasis even when stdout
is redirected, unless `--style never` is selected. Markdown always uses
CommonMark `**strong**` syntax, regardless of `--emphasis`. HTML uses strong or
underline markup; reverse emphasis emits a `bionic-reverse` class that requires
CSS in the consuming page. Plain output strips terminal controls and adds no
markup. Markdown source processing protects code, link destinations, escapes,
and entities. For Markdown input, `--skip-code=false` allows code emphasis in
ANSI or HTML output; Markdown-to-Markdown output rejects that setting because
CommonMark code cannot contain emphasis without changing its literal content.
HTML output escapes raw HTML
and removes unsafe link schemes. When reprocessing exported Markdown, select
`--input-format markdown`; ordinary text input treats markup as literal text.

ANSI document input defaults to `--controls safe`, which retains SGR styling
and safe OSC 8 hyperlinks while removing active controls such as clipboard
commands, cursor movement, DCS, and unsafe hyperlinks. Tabs, line endings, and
ordinary content are retained. Use `--controls preserve` or
`--input-format terminal` with ANSI output only for trusted terminal bytes.
Plain output always strips controls; reading sessions preserve terminal
controls because shell applications require them. ANSI/plain processing
preserves malformed UTF-8 bytes outside removed controls; Markdown and HTML
conversion report invalid UTF-8 as an error.

Normal filtering is deterministic across input chunking and delays. It waits for
a complete word or passes oversized words through at the 64 KiB pending bound.
For live streams, `--low-latency` flushes a stalled partial word unchanged after
50 ms; its remaining bytes stay unchanged until the next boundary. Reading
sessions retain this latency behavior. This explicit mode can change which words
receive emphasis depending on input timing.

Markdown streaming spools input to disk and parses bounded blocks. A block or
reference-definition catalog exceeding 1 MiB uses an unstyled literal fallback,
with Markdown/HTML escaping as appropriate; document size itself is unlimited
by the streaming API. The in-memory Markdown-input API accepts up to 8 MiB; use
`render_stream` for larger Markdown documents.

## Fixation profiles and preview

```sh
bionic-reader --profile subtle -t 'A lighter reading preference.'
bionic-reader --profile balanced --fraction 0.6 -t 'Explicit values override profiles.'
bionic-reader --profile strong --emphasis underline -t 'Emphasis without brighter text.'

# Short words get 25%, medium words 50%, and longer words the fallback 70%.
bionic-reader --length-rule 4:0.25 --length-rule 8:0.5 --fraction 0.7 -t 'Adjust emphasis by word length.'

# Open a live preview; use --print for a noninteractive preview.
bionic-reader preview
bionic-reader preview --text 'Reading complex documentation becomes easier.' --print --style always
```

Profiles initialize `(fraction, every, minimum length)` to subtle `(0.3, 2, 4)`,
balanced `(0.5, 1, 2)`, or strong `(0.7, 1, 2)`. Explicit numeric flags override
profile values. Length-rule thresholds are inclusive and must increase; longer
words use `fraction`. `--min-word-length 4` skips words of three letters or fewer.
`--emphasis bold|underline|reverse` selects a terminal attribute, preserving
existing colors and restoring pre-existing intensity/underline/reverse state.

## Interactive document reader

```sh
bionic-reader read article.txt
cat article.txt | bionic-reader read
```

The reader spools input to a temporary file and uses the controlling terminal for
keyboard input, so piped documents work. It strips active controls before display,
wraps on grapheme boundaries, responds to resizing, and restores terminal settings
on exit, errors, and handled termination signals. Invalid UTF-8 is shown with replacement characters. A grapheme exceeding 4 KiB is displayed as one replacement cell while its full source range remains intact on disk. A line-position index is kept
in memory; document text and search scans stay on disk.

| Key | Action |
| --- | --- |
| Arrow keys / `j`, `k` | Scroll |
| Page Up / Page Down | Move a page |
| Home / End / `g`, `G` | First / last position |
| `/`, then Enter | Search |
| `n`, `N` | Next / previous match |
| `+`, `-` | Increase / decrease fixation fraction |
| `[` / `]` | Adjust emphasis frequency |
| `m` / `M` | Adjust minimum word length |
| `p` | Cycle profiles |
| `s` | Show or hide the settings panel |
| `?` | Help |
| `q`, Escape, Ctrl-C | Exit |

Use `--input-format markdown read article.md` to protect code and link destinations in the reader.

Reader adjustments apply to the current view and do not overwrite the config
file. The reader requires a controlling terminal; exported formats work in
noninteractive pipelines.

## Configuration

Settings are read from `$XDG_CONFIG_HOME/bionic-reader/config.toml`, or `~/.config/bionic-reader/config.toml` when `XDG_CONFIG_HOME` is unset or empty. Create that file with your preferred editor, using settings such as:

```toml
profile = "balanced"
fraction = 0.5
every = 1
min_word_length = 2
style = "auto"
emphasis = "bold"
input_format = "text"
format = "ansi"
controls = "safe"
skip_code = true
low_latency = false
length_rules = [{ max_length = 4, fraction = 0.25 }, { max_length = 8, fraction = 0.5 }]
exclude_commands = ["my-pager"]
```

To try a separate file, save the example as `reading.toml` and select it explicitly:

```sh
bionic-reader --config ./reading.toml -t 'Use settings from this file.'
bionic-reader --config ./reading.toml --fraction 0.7 -t 'Override the configured fraction.'
```

Command-line values override file values, which override built-in defaults. Command exclusions in the file and `--exclude NAME` flags add to the built-in safety list; they do not remove its entries. The shipped defaults include common editors, pagers, terminal multiplexers, remote shells, and full-screen tools. Invalid settings are reported with their file path.

## Interactive shell sessions

Start a session manually from an interactive terminal:

```sh
bionic-reader session
# Or select a shell executable explicitly:
bionic-reader session --shell /bin/bash

# Select Zsh or fish, or add a passthrough exclusion for this session.
bionic-reader session --shell zsh
bionic-reader session --shell fish --exclude my-interactive-tool
```

Inside a reading session, run commands normally and adjust presentation as needed:

```sh
printf 'Command output receives emphasis.\n'
bionic-reader off
printf 'This command output stays unchanged.\n'
bionic-reader on
bionic-reader toggle
bionic-reader on
bionic-reader reload                 # Apply config-file edits at the next command boundary.
bionic-reader bypass -- cat article.txt
```

The controls above require an active reading session. Use `exit` to leave a session; a manually started session returns to its calling shell. Known editors, pagers, full-screen applications, and configured exclusions use passthrough. Application-generated prompts, nested interactive programs, and background output cannot always be classified reliably; add an exclusion or use `bypass` when their terminal interaction should remain untouched.

## Standalone shell setup

Choose either this setup or one framework adapter below. Ensure `bionic-reader` is on PATH before the activation line. Installation does not modify startup files; add the appropriate line yourself, then open a new terminal.

### Bash

Add to `~/.bashrc`:

```bash
eval "$(bionic-reader init bash)"
```

If your login shell reads only `~/.bash_profile` (common on macOS), ensure that file sources `~/.bashrc`:

```bash
if [ -r "$HOME/.bashrc" ]; then
  . "$HOME/.bashrc"
fi
```

### Zsh

Add to `~/.zshrc`:

```zsh
eval "$(bionic-reader init zsh)"
```

### fish

Add to `~/.config/fish/config.fish`, or `$XDG_CONFIG_HOME/fish/config.fish` if you set `XDG_CONFIG_HOME`:

```fish
eval (bionic-reader init fish | string collect)
```

### Temporarily skip automatic startup

Launch the shell you need with the escape hatch set:

```sh
env BIONIC_READER_DISABLE=1 bash
env BIONIC_READER_DISABLE=1 zsh
env BIONIC_READER_DISABLE=1 fish
```

## Shell framework setup

These instructions assume your framework is already installed. Run the copy commands **from the cloned repository**, in the framework's shell. Copy the adapter for your framework, make the indicated configuration edit, and open a new terminal. The adapters call the same initializer as the standalone setup; preserve your existing plugins and use one activation route.

### Bash-it

In Bash, copy the custom plugin:

```bash
br_custom="${BASH_IT_CUSTOM:-${BASH_IT:-$HOME/.bash_it}/custom}"
mkdir -p "$br_custom/plugins"
cp integrations/adapters/bash-it/bionic-reader/bionic-reader.plugin.bash "$br_custom/plugins/"
```

Bash-it loads custom plugin files automatically. Keep your existing Bash-it startup in `~/.bashrc`; no extra `eval` line is needed. If your Bash-it installation uses a different custom directory, set `BASH_IT_CUSTOM` to that directory before loading Bash-it.

### Oh My Bash

In Bash, copy the plugin:

```bash
br_custom="${OSH_CUSTOM:-${OSH:-$HOME/.oh-my-bash}/custom}"
mkdir -p "$br_custom/plugins/bionic-reader"
cp integrations/adapters/oh-my-bash/bionic-reader/bionic-reader.plugin.sh "$br_custom/plugins/bionic-reader/"
```

In `~/.bashrc`, add this **after** your existing `plugins=(...)` assignment and **before** the line that sources `oh-my-bash.sh`:

```bash
plugins+=(bionic-reader)
```

### Oh My Zsh

In Zsh, copy the plugin:

```zsh
br_custom="${ZSH_CUSTOM:-${ZSH:-$HOME/.oh-my-zsh}/custom}"
mkdir -p "$br_custom/plugins/bionic-reader"
cp integrations/adapters/oh-my-zsh/bionic-reader/bionic-reader.plugin.zsh "$br_custom/plugins/bionic-reader/"
```

In `~/.zshrc`, add this **after** your existing `plugins=(...)` assignment and **before** the line that sources `oh-my-zsh.sh`:

```zsh
plugins+=(bionic-reader)
```

### Prezto

In Zsh, copy the user module:

```zsh
mkdir -p "$HOME/.zprezto-contrib/bionic-reader"
cp integrations/adapters/prezto/bionic-reader/init.zsh "$HOME/.zprezto-contrib/bionic-reader/"
```

Append this to `~/.zpreztorc`, after your existing module configuration. It adds the directory and module while keeping the other entries:

```zsh
typeset -aU br_module_dirs br_modules
zstyle -a ':prezto:load' pmodule-dirs br_module_dirs || br_module_dirs=()
zstyle -a ':prezto:load' pmodule br_modules || br_modules=()
br_module_dirs+=("$HOME/.zprezto-contrib")
br_modules+=(bionic-reader)
zstyle ':prezto:load' pmodule-dirs "${br_module_dirs[@]}"
zstyle ':prezto:load' pmodule "${br_modules[@]}"
unset br_module_dirs br_modules
```

### Zinit

In Zsh, copy the local plugin:

```zsh
mkdir -p "$HOME/.local/share/bionic-reader/zinit"
cp integrations/adapters/zinit/bionic-reader/init.zsh "$HOME/.local/share/bionic-reader/zinit/"
```

In `~/.zshrc`, add after your Zinit initialization:

```zsh
zinit light "$HOME/.local/share/bionic-reader/zinit"
```

### Fisher

In fish, copy the plugin to a persistent local directory and install it through Fisher:

```fish
mkdir -p "$HOME/.local/share/bionic-reader/fisher/conf.d"
cp integrations/adapters/fisher/bionic-reader/conf.d/bionic-reader.fish "$HOME/.local/share/bionic-reader/fisher/conf.d/"
fisher install "$HOME/.local/share/bionic-reader/fisher"
```

Fisher installs its startup file into fish's `conf.d` directory. Keep the local plugin directory for future Fisher updates; no extra line in `config.fish` is needed.

### Oh My Fish

In fish, copy the package to your Oh My Fish configuration directory:

```fish
set -l br_omf_dir "$HOME/.config/omf"
if set -q OMF_CONFIG
    set br_omf_dir "$OMF_CONFIG"
end
mkdir -p "$br_omf_dir/pkg/bionic-reader"
cp integrations/adapters/oh-my-fish/bionic-reader/init.fish "$br_omf_dir/pkg/bionic-reader/"
```

Keep the existing Oh My Fish initialization in `config.fish`. Its package loader sources the adapter's `init.fish`; no extra Bionic Reader initialization line is needed.

All seven adapters are exercised through their pinned upstream loaders. See [adapter compatibility details](integrations/adapters/README.md).

## Shell completions

The CLI generates Bash, Zsh, and fish completions. To install them manually
from any executable, write the generated definitions to your user completion
directories:

```sh
mkdir -p "$HOME/.local/share/bash-completion/completions"
bionic-reader completions bash > "$HOME/.local/share/bash-completion/completions/bionic-reader"
```

```zsh
mkdir -p "$HOME/.zfunc"
bionic-reader completions zsh > "$HOME/.zfunc/_bionic-reader"
```

```fish
mkdir -p "$__fish_config_dir/completions"
bionic-reader completions fish > "$__fish_config_dir/completions/bionic-reader.fish"
```

Add the directory to `fpath` in `~/.zshrc` **before** `compinit` or before
loading a framework that runs it:

```zsh
fpath=("$HOME/.zfunc" $fpath)
```

If your startup file does not already initialize Zsh completion, add:

```zsh
autoload -Uz compinit
compinit
```

Bash must have its completion loader enabled to discover the user completion
directory. Alternatively, add `source "$HOME/.local/share/bash-completion/completions/bionic-reader"`
to `~/.bashrc`. The fish command uses its active configuration directory, including
a custom `XDG_CONFIG_HOME`. The checked-in files under `completions/` can also be sourced or
copied when working from a checkout. Homebrew and Linux system packages install
completions in their shell-specific locations; other binary packages include
the files under `share/bionic-reader/completions`. Cargo source installation
installs only the executable and does not edit shell configuration.

## Platforms and candidate packaging

The candidate build matrix targets macOS 11 or newer and Linux x86-64/ARM64 with glibc 2.28 or newer, plus x86-64/ARM64 musl Linux. CI installs and exercises native archives on all six target architectures. Local Homebrew installs cover macOS and GNU/Linux on x86-64 and ARM64; system-package installs cover Linux x86-64; pip/uv installs cover matching macOS and GNU/Linux hosts; npm/Yarn/pnpm cover Linux and macOS; local Snap installation covers Ubuntu 24.04 amd64. Minimum operating-system versions remain release validation gates.

Scripts under `scripts/` produce local candidate artifacts for Cargo/crates.io, Homebrew, apt, dnf, pacman, classic Snap, pip/uv wheels, and npm/Yarn/pnpm tarballs. Cargo candidates contain Rust source and compile during installation; other routes use the native binary or their existing packaging wrappers. They do not publish to registries or create publisher keys. Linux repository-generation scripts sign package/index metadata using an existing key explicitly supplied to the script; they never generate a publisher identity. The installation smokes use only a temporary key inside disposable test containers. Public registry availability, repository hosting and key distribution, upgrade validation, and Snap Store classic-confinement approval remain release gates. Local installation and removal are tested independently of publication.

See [CONTRIBUTING.md](CONTRIBUTING.md) for candidate-build commands, required tools, and checks, and [the verification record](docs/verification.md) for executed tests and remaining release gates. The implementation and local candidate tooling are available in this checkout; no public package availability is claimed.

## Uninstall

Remove the executable installed by Cargo:

```sh
cargo uninstall bionic-reader
```

Remove the standalone initialization line or the framework activation entry you added, together with the copied adapter. For Fisher, run `fisher remove "$HOME/.local/share/bionic-reader/fisher"` before deleting the local plugin directory. Remove any manually installed completions. Your other shell configuration remains in place.

## License

MIT. See [LICENSE](LICENSE).

## Tests and measured coverage

`cargo test --locked --all-targets` runs unit tests, generated properties, CLI/file-pipeline end-to-end tests, and real PTY tests for reader and shell sessions. `scripts/check-frameworks.sh` additionally fetches seven pinned upstream frameworks and verifies their real startup paths. CI also compiles, installs, runs, and removes repository-built packages through every documented package-manager route. See [CONTRIBUTING.md](CONTRIBUTING.md) for common Makefile commands and local coverage and fuzz workflows.

The coverage job uploads HTML, JSON, LCOV, and an uncovered-line report as the `coverage-reports` artifact. Current measurements and remaining gaps are recorded in [the verification report](docs/verification.md). Production files are included in the report; line, region, function, and branch measurements are separate, and passing end-to-end tests does not imply complete coverage.
