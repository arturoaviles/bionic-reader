# Bionic Reader

Bionic Reader is a local Rust command-line program that emphasizes the beginnings of words in text streams and opt-in interactive shell sessions. It transforms text on your machine; it does not send text to a service. It preserves the input text and layout and does not promise a reading-speed improvement.

**Install from this repository for now.** Bionic Reader has not been submitted to Homebrew, PyPI, npm, apt, dnf, pacman, Snap, or any other package registry. The package-generation scripts produce local candidates; registry install commands are not available yet.

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

Until [implementation PR #2](https://github.com/arturoaviles/bionic-reader/pull/2) is merged, select its branch:

```sh
git switch feat/bionic-reader
```

Skip that branch-selection step once the implementation is merged into `main`.

### 3. Build and install

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

To update later, pull the branch you installed and rerun `cargo install --path . --locked --force`. After PR #2 merges, switch your checkout to `main` before pulling updates.

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

The default `--style auto` adds terminal emphasis only when standard output is a terminal. Redirected output stays plain; `--style always` emits terminal styling even when redirected, and `--style never` disables styling. Existing ANSI formatting is preserved. With no `-t`, an interactive standard input shows help instead of waiting for text.

Tune word selection with `--fraction`, `--every`, and `--min-word-length`:

```sh
bionic-reader -t 'Read the first part of each word.' --fraction 0.4 --every 2
```

`fraction` is a finite value from `0` to `1`; `every` and `min-word-length` must be positive integers. The default fraction is `0.5`, default frequency is `1`, and default minimum length is `2`. Latin-script letters are emphasized; other scripts, digits, and emoji remain unchanged. Internal apostrophes stay within words, while punctuation such as hyphens separates word segments.

## Configuration

Settings are read from `$XDG_CONFIG_HOME/bionic-reader/config.toml`, or `~/.config/bionic-reader/config.toml` when `XDG_CONFIG_HOME` is unset or empty. Create that file with your preferred editor, using settings such as:

```toml
fraction = 0.5
every = 1
min_word_length = 2
style = "auto"
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
bionic-reader init fish | source
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

Generate definitions for your shell:

```sh
bionic-reader completions bash
bionic-reader completions zsh
bionic-reader completions fish
```

The generated files are also in `completions/`. Source `completions/bionic-reader.bash` in Bash, place `_bionic-reader` on Zsh's `fpath` before `compinit`, or copy `bionic-reader.fish` to your fish configuration's `completions` directory. Cargo's source installation installs only the executable; it does not install these files or edit shell configuration.

## Platforms and candidate packaging

The candidate build matrix targets macOS 11 or newer and Linux x86-64/ARM64 with glibc 2.28 or newer, plus x86-64/ARM64 musl Linux. CI currently runs native package-manager install smokes on Linux x86-64 GNU; the other listed targets receive candidate builds, not matching-host installation coverage. Release support requires artifact-level tests on matching hosts.

Scripts under `scripts/` produce local candidate artifacts for Homebrew, apt, dnf, pacman, classic Snap, pip/uv wheels, and npm/Yarn/pnpm tarballs. They do not publish to registries or create publisher keys. Linux repository-generation scripts sign package/index metadata using an existing key explicitly supplied to the script; they never generate a publisher identity. The installation smokes use only a temporary key inside disposable test containers. External registry installation, repository hosting and key distribution, upgrade/uninstall validation, and Snap classic-confinement approval remain release gates.

See [CONTRIBUTING.md](CONTRIBUTING.md) for candidate-build commands, required tools, and checks, and [the verification record](docs/verification.md) for executed tests and remaining release gates. The implementation and local candidate tooling are available in this checkout; no public package availability is claimed.

## Uninstall

Remove the executable installed by Cargo:

```sh
cargo uninstall bionic-reader
```

Remove the standalone initialization line or the framework activation entry you added, together with the copied adapter. For Fisher, run `fisher remove "$HOME/.local/share/bionic-reader/fisher"` before deleting the local plugin directory. Remove any manually installed completions. Your other shell configuration remains in place.

## License

MIT. See [LICENSE](LICENSE).
