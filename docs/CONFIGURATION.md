# Configuration

Settings are read from `$XDG_CONFIG_HOME/bionic-reader/config.toml` when `XDG_CONFIG_HOME` is an absolute path; otherwise they use `~/.config/bionic-reader/config.toml`. Create that file with your preferred editor, using settings such as:

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
