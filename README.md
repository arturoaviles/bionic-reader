# Bionic Reader

Read text with emphasis on the beginnings of words.

[![CI](https://github.com/arturoaviles/bionic-reader/actions/workflows/ci.yml/badge.svg)](https://github.com/arturoaviles/bionic-reader/actions/workflows/ci.yml)
[![MIT license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/arturoaviles/bionic-reader/blob/main/LICENSE)

Bionic Reader works with text streams, Markdown documents, and interactive shell
sessions. It runs locally and keeps your words intact. Try different emphasis
settings to find what feels comfortable; no reading-speed improvement is promised.

> **Bio**nic **Rea**der **empha**sizes **wo**rd **begin**nings.

## Installation

**The first release and package-manager publications are pending.** Cargo's Git
installation works now. The commands below describe the release packages and
registry channels once published; [installation details](https://github.com/arturoaviles/bionic-reader/blob/main/docs/INSTALL.md)
include direct-download alternatives and platform requirements.

### Homebrew

```sh
brew install arturoaviles/tap/bionic-reader
```

### Linux

```sh
# Debian / Ubuntu
curl -fL "https://github.com/arturoaviles/bionic-reader/releases/latest/download/bionic-reader-linux-$(dpkg --print-architecture).deb" -o bionic-reader.deb
sudo apt install ./bionic-reader.deb

# Fedora
curl -fL "https://github.com/arturoaviles/bionic-reader/releases/latest/download/bionic-reader-linux-$(rpm --eval '%{_arch}').rpm" -o bionic-reader.rpm
sudo dnf install --nogpgcheck ./bionic-reader.rpm

# Arch Linux
curl -fL "https://github.com/arturoaviles/bionic-reader/releases/latest/download/bionic-reader-linux-$(uname -m).pkg.tar.zst" -o bionic-reader.pkg.tar.zst
sudo pacman -U ./bionic-reader.pkg.tar.zst
```

### Python and Node.js

```sh
# pip (in a virtual environment) or uv
pip install bionic-reader
uv tool install bionic-reader

# npm, Yarn, or pnpm
npm install --global @arturoaviles/bionic-reader
yarn add @arturoaviles/bionic-reader
pnpm add --global @arturoaviles/bionic-reader
```

### Cargo and Snap

```sh
cargo install --git https://github.com/arturoaviles/bionic-reader.git --locked bionic-reader

# Snap currently installs a repository-built file; see the build guide.
sudo snap install --dangerous --classic ./dist/local/snap/bionic-reader_*.snap
```

Yarn installs into the current project; run it with `yarn bionic-reader`.
Package managers select the CPU architecture automatically. Linux release
filenames use the native architecture query shown above. Snap build instructions
live in [BUILD.md](https://github.com/arturoaviles/bionic-reader/blob/main/docs/BUILD.md#snap-ubuntu-2404).
You can also download native binaries from [GitHub Releases](https://github.com/arturoaviles/bionic-reader/releases).

## The reader

```sh
bionic-reader read article.txt
cat article.txt | bionic-reader read
```

Scroll with the arrow keys or `j`/`k`, search with `/`, and press `q` to leave.
Use `?` for all shortcuts. Adjust emphasis while you read with `+`/`-`, or try
settings interactively with `bionic-reader preview`.

## The CLI

```sh
bionic-reader -t 'Make long passages easier to follow.'
cat article.txt | bionic-reader
bionic-reader --input-format markdown --style always < article.md

# Change the reading style.
bionic-reader --profile subtle -t 'A lighter touch.'
bionic-reader --fraction 0.6 --every 2 -t 'Choose your own emphasis.'

# Export HTML or Markdown.
bionic-reader --format html < article.txt > article.html
bionic-reader --format markdown < article.txt > emphasized.md
```

See `bionic-reader --help` and the [usage guide](https://github.com/arturoaviles/bionic-reader/blob/main/docs/USAGE.md)
for formats, profiles, safe terminal controls, and reader shortcuts.

## Shell integration

Start a reading session with `bionic-reader session`. Known editors, pagers, and
full-screen applications use passthrough. Inside a session, use `bionic-reader
off`, `on`, or `toggle` to control emphasis, and `exit` to leave.

For automatic sessions, add one initializer to your shell configuration:

```sh
# ~/.bashrc
eval "$(bionic-reader init bash)"
# ~/.zshrc
eval "$(bionic-reader init zsh)"
```

Bash, Zsh, fish, shell completions, and seven framework adapters are covered in
the [shell guide](https://github.com/arturoaviles/bionic-reader/blob/main/docs/SHELL.md).

## Configuration

Save preferences in `~/.config/bionic-reader/config.toml`, or beneath an absolute
`XDG_CONFIG_HOME`:

```toml
profile = "balanced"
fraction = 0.5
emphasis = "bold"
```

Command-line settings override the file. See the [configuration guide](https://github.com/arturoaviles/bionic-reader/blob/main/docs/CONFIGURATION.md)
for all options and precedence.

## Contributing

See [CONTRIBUTING.md](https://github.com/arturoaviles/bionic-reader/blob/main/CONTRIBUTING.md)
for development checks, [BUILD.md](https://github.com/arturoaviles/bionic-reader/blob/main/docs/BUILD.md)
for source builds, and [RELEASE.md](https://github.com/arturoaviles/bionic-reader/blob/main/docs/RELEASE.md)
for release and package publication.

## License

[MIT](https://github.com/arturoaviles/bionic-reader/blob/main/LICENSE).
