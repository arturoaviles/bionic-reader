# Shell integration

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

Homebrew users can enter the shipped asset directory with
`cd "$(brew --prefix bionic-reader)/share/bionic-reader"`. For a global npm
installation, use `cd "$(npm root --global)/@arturoaviles/bionic-reader/share/bionic-reader"`.
Yarn installs into a project; automatic shell startup requires a stable
`bionic-reader` command on PATH, so use a global installation route for that setup.

These instructions assume your framework is already installed. Run the copy commands from the directory containing the shipped `integrations` folder, in the framework's shell. For Linux system packages, first run `cd /usr/share/bionic-reader`; for an extracted archive, use its `share/bionic-reader` directory. Wheels also ship that directory under their installation prefix. A source checkout can use the repository root. Copy the adapter for your framework, make the indicated configuration edit, and open a new terminal. The adapters call the same initializer as the standalone setup; preserve your existing plugins and use one activation route.

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

All seven adapters are exercised through their pinned upstream loaders. See [adapter compatibility details](https://github.com/arturoaviles/bionic-reader/blob/main/integrations/adapters/README.md).

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
