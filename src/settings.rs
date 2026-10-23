use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub fraction: f64,
    pub every: usize,
    pub min_word_length: usize,
    pub style: Style,
    pub exclude_commands: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub fraction: Option<f64>,
    pub every: Option<usize>,
    pub min_word_length: Option<usize>,
    pub style: Option<Style>,
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
                    append_unique(&mut settings.exclude_commands, configured.exclude_commands);
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound && explicit.is_none() => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
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
            append_unique(&mut settings.exclude_commands, values.clone());
        }

        settings.validate()?;
        Ok(settings)
    }

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

fn append_unique(target: &mut Vec<String>, additions: Vec<String>) {
    let mut seen: HashSet<String> = target.iter().cloned().collect();
    for command in additions {
        if seen.insert(command.clone()) {
            target.push(command);
        }
    }
}

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
    use super::*;

    #[test]
    fn command_line_override_wins_over_config_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        fs::write(&path, "fraction = 0.25\nevery = 3\n").unwrap();

        let overrides = Overrides {
            fraction: Some(0.75),
            ..Overrides::default()
        };
        let settings = Settings::load(Some(&path), &overrides).unwrap();

        assert_eq!(settings.fraction, 0.75);
        assert_eq!(settings.every, 3);
    }

    #[test]
    fn custom_command_exclusions_append_to_defaults_and_deduplicate() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        fs::write(&path, "exclude_commands = [\"htop\", \"custom-editor\"]\n").unwrap();

        let overrides = Overrides {
            exclude_commands: Some(vec!["custom-editor".to_owned(), "custom-shell".to_owned()]),
            ..Overrides::default()
        };
        let settings = Settings::load(Some(&path), &overrides).unwrap();

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
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("broken.toml");
        fs::write(&path, "fraction = 4.0\n").unwrap();

        let error = Settings::load(Some(&path), &Overrides::default()).unwrap_err();
        assert!(error.contains("fraction must be a finite number between 0 and 1"));
        assert!(error.contains("broken.toml"));
    }
}
