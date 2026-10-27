# Bionic Reader

Read text with emphasis on the beginnings of words.

[![CI](https://github.com/arturoaviles/bionic-reader/actions/workflows/ci.yml/badge.svg)](https://github.com/arturoaviles/bionic-reader/actions/workflows/ci.yml)
[![MIT license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/arturoaviles/bionic-reader/blob/main/LICENSE)

Bionic Reader works with text streams, Markdown documents, and interactive shell
sessions. It runs locally and keeps your words intact. Try different emphasis
settings to find what feels comfortable; no reading-speed improvement is promised.

> **Bio**nic **Rea**der **empha**sizes **wo**rd **begin**nings.

## Installation

Install with your package manager using the commands below. Homebrew installs
prebuilt binaries; Cargo installs from crates.io. See
[installation details](https://github.com/arturoaviles/bionic-reader/blob/main/docs/INSTALL.md)
for channel availability, direct-download alternatives, and platform requirements.

### Homebrew

```sh
brew tap arturoaviles/tap
brew trust arturoaviles/tap
brew install bionic-reader
```

Homebrew selects the matching prebuilt binary. See the
[Homebrew guide](https://github.com/arturoaviles/bionic-reader/blob/main/docs/INSTALL.md#homebrew)
for updates, removal, and platform requirements.

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

### Cargo

```sh
cargo install bionic-reader --locked
```

Yarn installs into the current project; run it with `yarn bionic-reader`.
Package managers select the CPU architecture automatically. Linux release
filenames use the native architecture query shown above.
You can also download native binaries from [GitHub Releases](https://github.com/arturoaviles/bionic-reader/releases).

## Application libraries

Install packages from [PyPI](https://pypi.org/project/bionic-reader/),
[npm](https://www.npmjs.com/package/@arturoaviles/bionic-reader), or
[crates.io](https://crates.io/crates/bionic-reader). The library APIs below share
the CLI's local transformation engine. See the [library guide](docs/LIBRARY.md)
for complete options, errors, and platform requirements.

Earlier CLI-only releases do not include the callable APIs shown below. The
registry installation commands select the latest available release.

### Python

The library API requires Python 3.11+. Install into your application's virtual
environment:

```sh
python -m pip install bionic-reader
```

```python
from bionic_reader import Reader

reader = Reader()
print(reader.render("Hello world", format="html", profile="balanced"))
print(reader.segments("Hello world"))
```

### JavaScript

The library API requires Node 22+. Install into your application:

```sh
npm install @arturoaviles/bionic-reader
```

Initialize the bundled WebAssembly engine once:

```js
import { createReader } from '@arturoaviles/bionic-reader';

const reader = await createReader();
console.log(reader.render('Hello world', { format: 'html', profile: 'balanced' }));
console.log(reader.segments('Hello world'));
```

CommonJS uses `const { createReader } = require('@arturoaviles/bionic-reader')`
and awaits initialization in an async function. Browser bundlers select the
browser loader. React applications can use `BionicReaderProvider` and
`BionicText` from `@arturoaviles/bionic-reader/react`; see the
[React example](docs/LIBRARY.md#react) for initialization and hydration.

### Rust

Requires Rust 1.85.1+. Add the crate to your application:

```sh
cargo add bionic-reader
```

```rust
use bionic_reader::reading::{Options, Reader};
use bionic_reader::settings::OutputFormat;

fn main() -> Result<(), bionic_reader::Error> {
    let reader = Reader::new();
    let options = Options::default();
    println!("{}", reader.render("Hello world", OutputFormat::Html, &options)?);
    println!("{:?}", reader.segments("Hello world", &options)?);
    Ok(())
}
```

Formatted output requires an explicit `ansi`, `plain`, `markdown`, or `html`
format. Text segments contain literal `text` and an `emphasized` flag for custom
presentation. Calls use explicit settings and deterministic defaults; they do
not implicitly read CLI configuration or terminal state.

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
