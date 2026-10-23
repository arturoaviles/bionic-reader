//! Configuration types and loading for bionic reading behavior.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Controls when bionic reading emphasis is enabled.
///
/// These three variants are the complete style policy: automatic terminal
/// detection, unconditional emphasis, or passthrough.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Enable emphasis when output is a terminal.
    Auto,
    /// Enable emphasis regardless of the output destination.
    Always,
    /// Disable emphasis and pass output through unchanged.
    Never,
}

/// Settings that control bionic reading emphasis and command passthrough.
///
/// Use struct update syntax with [`Default`] to override selected fields.
/// Validate manually constructed settings before passing them to a transformer.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Fraction of each selected word's letters to emphasize, from `0.0` to `1.0`.
    pub fraction: f64,
    /// Word selection frequency: `1` selects every eligible word; `2` selects every other one.
    pub every: usize,
    /// Minimum number of letters a word needs to be eligible for emphasis.
    pub min_word_length: usize,
    /// Policy for enabling emphasis.
    pub style: Style,
    /// Commands whose output should remain in passthrough mode during a session.
    pub exclude_commands: Vec<String>,
}

/// Optional configuration values that override [`Settings`] defaults and files.
///
/// Unspecified fields leave the resolved settings unchanged. Custom command
/// exclusions are appended to the built-in list and deduplicated.
/// Use struct update syntax with [`Default`] to set only selected values.
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    /// Fraction of each selected word's letters to emphasize, from `0.0` to `1.0`.
    pub fraction: Option<f64>,
    /// Word selection frequency: `1` selects every eligible word; `2` selects every other one.
    pub every: Option<usize>,
    /// Minimum number of letters a word needs to be eligible for emphasis.
    pub min_word_length: Option<usize>,
    /// Policy for enabling emphasis.
    pub style: Option<Style>,
    /// Additional command names to keep in passthrough mode during a session.
    pub exclude_commands: Option<Vec<String>>,
}

const DEFAULT_EXCLUDE_COMMANDS: &[&str] = &[
    "vi", "vim", "nvim", "nano", "emacs", "less", "more", "man", "top", "htop", "btop", "fzf",
    "fzf-tmux", "watch", "tmux", "screen", "ssh", "mosh", "sftp", "ftp", "telnet", "sh", "bash",
    "zsh", "fish", "python", "python3", "ipython", "node", "irb", "sqlite3", "psql", "mysql",
];

impl Default for Settings {
    fn default() -> Self {
        Self {
            fraction: 0.5,
            every: 1,
            min_word_length: 2,
            style: Style::Auto,
            exclude_commands: DEFAULT_EXCLUDE_COMMANDS
                .iter()
                .map(|command| (*command).to_owned())
                .collect(),
        }
    }
}

impl Settings {
    /// Loads defaults, an optional configuration file, and explicit overrides.
    ///
    /// Command exclusions from the file and overrides are appended to the
    /// built-in exclusions and deduplicated.
    ///
    /// # Errors
    ///
    /// Returns an error when a configuration file cannot be read or parsed, or
    /// when the resolved settings fail validation.
    pub fn load(explicit: Option<&Path>, overrides: &Overrides) -> Result<Self, String> {
        let mut settings = Self::default();
        let path = match explicit {
            Some(path) => Some(path.to_path_buf()),
            None => default_config_path(),
        };

        if let Some(path) = path {
            match fs::read_to_string(&path) {
                Ok(contents) => {
                    let configured: Settings = toml::from_str(&contents).map_err(|error| {
                        format!("invalid configuration file {}: {error}", path.display())
                    })?;
                    configured.validate().map_err(|error| {
                        format!("invalid configuration file {}: {error}", path.display())
                    })?;
                    settings.fraction = configured.fraction;
                    settings.every = configured.every;
                    settings.min_word_length = configured.min_word_length;
                    settings.style = configured.style;
                    append_unique(&mut settings.exclude_commands, &configured.exclude_commands);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound && explicit.is_none() => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Err(format!(
                        "configuration file {} does not exist",
                        path.display()
                    ));
                }
                Err(error) => {
                    return Err(format!(
                        "could not read configuration file {}: {error}",
                        path.display()
                    ));
                }
            }
        }

        if let Some(value) = overrides.fraction {
            settings.fraction = value;
        }
        if let Some(value) = overrides.every {
            settings.every = value;
        }
        if let Some(value) = overrides.min_word_length {
            settings.min_word_length = value;
        }
        if let Some(value) = overrides.style {
            settings.style = value;
        }
        if let Some(values) = &overrides.exclude_commands {
            append_unique(&mut settings.exclude_commands, values);
        }

        settings.validate()?;
        Ok(settings)
    }

    /// Checks that all settings are within their supported ranges.
    ///
    /// # Errors
    ///
    /// Returns an error when a numeric value is out of range or a command
    /// exclusion is empty.
    pub fn validate(&self) -> Result<(), String> {
        if !self.fraction.is_finite() || !(0.0..=1.0).contains(&self.fraction) {
            return Err("fraction must be a finite number between 0 and 1".to_owned());
        }
        if self.every == 0 {
            return Err("every must be a positive integer".to_owned());
        }
        if self.min_word_length == 0 {
            return Err("min_word_length must be a positive integer".to_owned());
        }
        if self
            .exclude_commands
            .iter()
            .any(|command| command.trim().is_empty())
        {
            return Err("exclude_commands must not contain empty command names".to_owned());
        }
        Ok(())
    }
}

fn append_unique(target: &mut Vec<String>, additions: &[String]) {
    let unique_additions = {
        let mut seen: HashSet<&str> = target.iter().map(String::as_str).collect();
        let mut unique: Vec<String> = Vec::new();
        for command in additions {
            if seen.insert(command.as_str()) {
                unique.push(command.clone());
            }
        }
        unique
    };
    target.extend(unique_additions);
}

#[must_use]
fn default_config_path() -> Option<PathBuf> {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
        let config_home = PathBuf::from(config_home);
        if config_home.is_absolute() {
            return Some(config_home.join("bionic-reader/config.toml"));
        }
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/bionic-reader/config.toml"))
}

#[cfg(test)]
mod tests {
    use super::{Overrides, Settings};
    use std::fs;

    #[test]
    fn command_line_override_wins_over_config_file() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let path = temp.path().join("config.toml");
        fs::write(&path, "fraction = 0.25\nevery = 3\n").expect("write temporary configuration");

        let overrides = Overrides {
            fraction: Some(0.75),
            ..Overrides::default()
        };
        let settings = Settings::load(Some(&path), &overrides)
            .expect("load valid configuration with overrides");

        assert_eq!(settings.fraction, 0.75);
        assert_eq!(settings.every, 3);
    }

    #[test]
    fn custom_command_exclusions_append_to_defaults_and_deduplicate() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let path = temp.path().join("config.toml");
        fs::write(&path, "exclude_commands = [\"htop\", \"custom-editor\"]\n")
            .expect("write temporary configuration");

        let overrides = Overrides {
            exclude_commands: Some(vec!["custom-editor".to_owned(), "custom-shell".to_owned()]),
            ..Overrides::default()
        };
        let settings =
            Settings::load(Some(&path), &overrides).expect("load valid command exclusions");

        assert!(settings.exclude_commands.contains(&"vim".to_owned()));
        assert!(settings.exclude_commands.contains(&"htop".to_owned()));
        assert!(
            settings
                .exclude_commands
                .contains(&"custom-editor".to_owned())
        );
        assert!(
            settings
                .exclude_commands
                .contains(&"custom-shell".to_owned())
        );
        assert_eq!(
            settings
                .exclude_commands
                .iter()
                .filter(|command| command.as_str() == "custom-editor")
                .count(),
            1
        );
    }

    #[test]
    fn invalid_explicit_config_reports_its_path() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let path = temp.path().join("broken.toml");
        fs::write(&path, "fraction = 4.0\n").expect("write invalid test configuration");

        let error = Settings::load(Some(&path), &Overrides::default())
            .expect_err("invalid fraction must fail validation");
        assert!(error.contains("fraction must be a finite number between 0 and 1"));
        assert!(error.contains("broken.toml"));
    }
}
