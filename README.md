# Bionic Reader

Bionic Reader is a local Rust command-line program that emphasizes the beginnings of words in text streams and opt-in interactive shell sessions. It transforms text on your machine; it does not send text to a service. It preserves the input text and layout and does not promise a reading-speed improvement.

The project is not published yet. The repository contains local candidate-build and package-generation scripts. CI installs local signed Linux repositories in disposable Debian, Fedora, and Arch containers and tests local npm, Yarn Plug'n'Play, and pnpm installs; these checks do not publish anything. Hosting, external registry installation, signing-key provisioning for real users, upgrade testing, and other release work remain. The Snap recipe uses classic confinement and remains a devel candidate pending Snap Store approval.

## Build from source

Install Rust 1.85.1 or newer, then build and install the executable from this checkout:

```sh
cargo install --path . --locked
bionic-reader --help
```

You can also run it without installing it using `cargo run --locked -- --help`.

## Transform text

Pass one string with `-t` / `--text`, or pipe text on standard input:

```sh
bionic-reader -t 'Bionic reading keeps the original words.'
cat article.txt | bionic-reader
printf 'A stream can be transformed.\n' | bionic-reader --style always
```

The default `--style auto` adds terminal emphasis only when standard output is a terminal. Redirected output stays plain; `--style always` emits terminal styling even when redirected, and `--style never` disables styling. Existing ANSI formatting is preserved. With no `-t`, an interactive standard input shows help instead of waiting for text.

Tune word selection with `--fraction`, `--every`, and `--min-word-length`:

```sh
bionic-reader -t 'Read the first part of each word.' --fraction 0.4 --every 2
```

`fraction` is a finite value from `0` to `1`; `every` and `min-word-length` must be positive integers. The default fraction is `0.5`, default frequency is `1`, and default minimum length is `2`. Latin-script letters are emphasized; other scripts, digits, and emoji remain unchanged. Internal apostrophes stay within words, while punctuation such as hyphens separates word segments.

## Configuration

Settings are read from `$XDG_CONFIG_HOME/bionic-reader/config.toml`, or `~/.config/bionic-reader/config.toml` when `XDG_CONFIG_HOME` is unset. Select a file explicitly with `--config`:

```toml
fraction = 0.5
every = 1
min_word_length = 2
style = "auto"
exclude_commands = ["my-pager"]
```

Command-line values override file values, which override built-in defaults. Command exclusions in the file and `--exclude NAME` flags add to the built-in safety list; they do not remove its entries. The shipped defaults include common editors, pagers, terminal multiplexers, remote shells, and full-screen tools. Invalid settings are reported with their file path.

## Interactive shell sessions

Start a session manually from an interactive terminal:

```sh
bionic-reader session
# Or select a shell executable explicitly:
bionic-reader session --shell /bin/bash
```

The program supports Bash, Zsh, and fish session initialization. You can opt into automatic startup by adding the printed snippet to your shell configuration yourself:

```sh
# Bash or Zsh
eval "$(bionic-reader init bash)"  # use `zsh` for Zsh

# fish
bionic-reader init fish | source
```

Generate shell completion output with `bionic-reader completions bash`, `bionic-reader completions zsh`, or `bionic-reader completions fish`. The generated files are checked into `completions/`; CI verifies that they match the current CLI. Package candidates install them into conventional completion directories where the package format allows it.

Installation does not modify shell startup files. Set `BIONIC_READER_DISABLE=1` to bypass automatic startup for a shell invocation. Within an active session, `bionic-reader on`, `off`, `toggle`, and `reload` control the current session. `reload` applies the current configuration at the next command boundary. Use `bionic-reader bypass -- COMMAND [ARG ...]` to run a command without transforming its output.

Session transformation is best-effort at shell command boundaries. Known editors, pagers, full-screen applications, and configured exclusions use passthrough. Application-generated prompts, nested interactive programs, and background output cannot always be classified reliably; add a command to `exclude_commands` or use `bypass` when its terminal behavior should remain untouched.

Adapters for Bash-it, Oh My Bash, Oh My Zsh, Prezto, Zinit, Fisher, and Oh My Fish are included. See the [framework installation instructions](integrations/adapters/README.md). Tests exercise all seven managers' pinned upstream loaders, including Bash 3.2 and fish terminal metadata.

## Platforms and candidate packaging

The candidate build matrix targets macOS 11 or newer and Linux x86-64/ARM64 with glibc 2.28 or newer, plus x86-64/ARM64 musl Linux. CI currently runs native package-manager install smokes on Linux x86-64 GNU; the other listed targets receive candidate builds, not matching-host installation coverage. Release support requires artifact-level tests on matching hosts.

Scripts under `scripts/` produce local candidate artifacts for Homebrew, apt, dnf, pacman, classic Snap, pip/uv wheels, and npm/Yarn/pnpm tarballs. They do not publish to registries or create publisher keys. Linux repository-generation scripts sign package/index metadata using an existing key explicitly supplied to the script; they never generate a publisher identity. The installation smokes use only a temporary key inside disposable test containers. External registry installation, repository hosting and key distribution, upgrade/uninstall validation, and Snap classic-confinement approval remain release gates.

See [CONTRIBUTING.md](CONTRIBUTING.md) for candidate-build commands, required tools, and checks, and [the verification record](docs/verification.md) for executed tests and remaining release gates. The implementation and local candidate tooling are available in this checkout; no public package availability is claimed.

To remove a source installation, delete the installed executable (or run `cargo uninstall bionic-reader` if Cargo installed it) and remove the initialization line you added to your shell configuration. For candidate installations, use the installing manager's normal uninstall command: `brew uninstall`, `apt remove`, `dnf remove`, `pacman -R`, `snap remove`, `pip uninstall`, `uv tool uninstall`, `npm uninstall`, `yarn remove`, or `pnpm remove`, with the installed package name. Remove copied framework adapters using the instructions above; package removal never rewrites shell configuration.

## License

MIT. See [LICENSE](LICENSE).
