# Framework adapters

Each adapter activates the same shell initializer as `bionic-reader init`.
The executable must already be on `PATH`. Local native archives, Homebrew,
Linux system packages, Snap, Python wheels, and npm-family packages carry these
directories under `share/bionic-reader/integrations/adapters`. Cargo source
installation installs only the executable; a source checkout uses this
directory directly. Copy only the adapter for your framework. Installation of
the executable never edits shell configuration.

| Framework | Adapter directory | Activation |
| --- | --- | --- |
| Bash-it | `bash-it/bionic-reader` | Copy `bionic-reader.plugin.bash` to `$BASH_IT_CUSTOM/plugins/` (normally `~/.bash_it/custom/plugins/`). Bash-it loads custom plugin files. |
| Oh My Bash | `oh-my-bash/bionic-reader` | Copy the directory to `$OSH_CUSTOM/plugins/bionic-reader` and add `bionic-reader` to your existing `plugins` array before loading Oh My Bash. |
| Oh My Zsh | `oh-my-zsh/bionic-reader` | Copy the directory to `$ZSH_CUSTOM/plugins/bionic-reader` and add `bionic-reader` to your existing `plugins` array before loading Oh My Zsh. |
| Prezto | `prezto/bionic-reader` | Copy to a user module directory, include its parent in `zstyle ':prezto:load' pmodule-dirs`, and append `bionic-reader` to your existing `pmodule` list in `.zpreztorc`. |
| Zinit | `zinit/bionic-reader` | Copy to a local plugin directory and add `zinit light /absolute/path/to/bionic-reader` after loading Zinit. |
| Fisher | `fisher/bionic-reader` | Run `fisher install /absolute/path/to/this/directory`; Fisher installs its `conf.d/bionic-reader.fish` file. |
| Oh My Fish | `oh-my-fish/bionic-reader` | Copy to `$OMF_CONFIG/pkg/bionic-reader` (normally `~/.config/omf/pkg/bionic-reader`). Oh My Fish loads its `init.fish`. |

Preserve the other entries in your framework configuration. Avoid adding both
a standalone automatic-start snippet and a framework adapter; repeated loading
is guarded, but one activation route is easier to maintain. Set
`BIONIC_READER_DISABLE=1` before starting a shell to skip automatic activation.

To uninstall integration, remove the copied adapter and its activation entry.
For Fisher, use `fisher remove /absolute/path/to/the/installed/plugin`. No
original configuration needs to be reconstructed.

`tests/frameworks.rs` exercises the seven managers' actual loaders in isolated
PTYs by starting fresh interactive shells. They verify automatic activation of
exactly one reading session, initialization before activation and inside that
session, and emphasized command output. `frameworks.tsv` pins the official upstream revisions. Run
`scripts/check-frameworks.sh` to fetch those revisions into a temporary directory
and run the tests. These network-dependent tests are separate from the offline
unit suite and run explicitly in CI. They establish compatibility with the
pinned revisions, not every past or future framework version.
