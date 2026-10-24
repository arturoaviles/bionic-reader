//! Configuration types and loading for bionic reading behavior.
//!
//! [`Settings::load`] resolves built-in defaults, an optional TOML file, and explicit
//! overrides in that order. Command exclusions from the file and overrides are appended to
//! the built-in exclusions and deduplicated.
//!
//! # Examples
//!
//! ```
//! use bionic_reader::settings::Settings;
//!
//! let settings = Settings { every: 2, ..Settings::new() };
//! settings.validate().expect("the supplied settings are valid");
//! assert_eq!(settings.every, 2);
//! ```

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::Error;

/// Controls when bionic reading emphasis is enabled.
///
/// These three variants are the complete style policy: automatic terminal
/// detection, unconditional emphasis, or passthrough.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Enable emphasis when output is a terminal.
    #[default]
    Auto,
    /// Enable emphasis regardless of the output destination.
    Always,
    /// Disable added emphasis; configured format and input-control processing still apply.
    Never,
}

impl Style {
    /// Resolves terminal styling, honoring nonempty `NO_COLOR` and `TERM=dumb` in auto mode.
    /// An explicit `Always` enables styling even when these environment hints are set.
    #[must_use]
    pub fn enabled(self, is_terminal: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => {
                is_terminal
                    && env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
                    && env::var_os("TERM").is_none_or(|value| value != "dumb")
            }
        }
    }
}

/// Built-in fixation preferences, applied before explicit numeric settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Emphasize fewer, longer words with a shorter prefix.
    Subtle,
    /// Emphasize half of each eligible word.
    Balanced,
    /// Emphasize a longer prefix of every eligible word.
    Strong,
}

/// The terminal attribute used to distinguish selected word prefixes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Emphasis {
    /// Use bold intensity, restoring the original bold and faint attributes.
    #[default]
    Bold,
    /// Use underlining without changing text colors or intensity.
    Underline,
    /// Use reverse video without selecting a brighter foreground color.
    Reverse,
}

/// How input bytes should be interpreted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputFormat {
    /// Ordinary text with the configured terminal-control policy.
    #[default]
    Text,
    /// `CommonMark` source with protected syntax and code spans.
    Markdown,
    /// Trusted terminal output whose existing controls must be preserved.
    Terminal,
}

/// The representation emitted by document rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Terminal emphasis, subject to the style policy.
    #[default]
    Ansi,
    /// Original text without terminal escape sequences or added emphasis.
    Plain,
    /// Markdown source with emphasis syntax around selected prefixes.
    Markdown,
    /// Escaped HTML with semantic emphasis elements.
    Html,
}

/// Policy for control sequences supplied by a document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlPolicy {
    /// Permit presentation styling and safe hyperlinks; remove active controls.
    #[default]
    Safe,
    /// Preserve terminal control bytes for trusted sources.
    Preserve,
}

/// An inclusive word-length threshold with its own emphasis fraction.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LengthRule {
    /// Largest eligible letter-grapheme count covered by this rule.
    pub max_length: usize,
    /// Fraction of the selected word to emphasize, from zero through one.
    pub fraction: f64,
}

/// Settings that control bionic reading emphasis and command passthrough.
///
/// Use struct update syntax with [`Default`] to override selected fields.
/// Validate manually constructed settings before passing them to a transformer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Optional named preference from which numeric settings were initialized.
    pub profile: Option<Profile>,
    /// Increasing inclusive length thresholds; unmatched words use `fraction`.
    pub length_rules: Vec<LengthRule>,
    /// Terminal attribute used for emphasis.
    pub emphasis: Emphasis,
    /// Input syntax for document processing.
    pub input_format: InputFormat,
    /// Output representation for document processing.
    pub format: OutputFormat,
    /// Treatment of controls in document input; reading sessions retain terminal controls.
    pub controls: ControlPolicy,
    /// Leave Markdown inline and fenced code unchanged.
    pub skip_code: bool,
    /// Flush stalled partial words unchanged after 50 ms instead of waiting for a boundary.
    pub low_latency: bool,
}

/// Optional configuration values that override [`Settings`] defaults and files.
///
/// Unspecified fields leave the resolved settings unchanged. Custom command
/// exclusions are appended to the built-in list and deduplicated.
/// Use struct update syntax with [`Default`] to set only selected values.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
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
    /// Named preference applied before explicit numeric overrides.
    pub profile: Option<Profile>,
    /// Replacement length-specific emphasis rules.
    pub length_rules: Option<Vec<LengthRule>>,
    /// Terminal emphasis attribute override.
    pub emphasis: Option<Emphasis>,
    /// Document input syntax override.
    pub input_format: Option<InputFormat>,
    /// Document output representation override.
    pub format: Option<OutputFormat>,
    /// Input terminal-control policy override.
    pub controls: Option<ControlPolicy>,
    /// Override Markdown code protection.
    pub skip_code: Option<bool>,
    /// Override low-latency streaming fallback.
    pub low_latency: Option<bool>,
}

impl Overrides {
    /// Creates overrides that leave all resolved settings unchanged.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
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
            profile: None,
            length_rules: Vec::new(),
            emphasis: Emphasis::Bold,
            input_format: InputFormat::Text,
            format: OutputFormat::Ansi,
            controls: ControlPolicy::Safe,
            skip_code: true,
            low_latency: false,
            exclude_commands: DEFAULT_EXCLUDE_COMMANDS
                .iter()
                .map(|command| (*command).to_owned())
                .collect(),
        }
    }
}

impl Settings {
    /// Creates settings with the built-in defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads defaults, an optional configuration file, and explicit overrides.
    ///
    /// Command exclusions from the file and overrides are appended to the
    /// built-in exclusions and deduplicated.
    ///
    /// # Errors
    ///
    /// Returns an error when a configuration file cannot be read or parsed, or
    /// when the resolved settings fail validation.
    pub fn load(explicit: Option<&Path>, overrides: &Overrides) -> Result<Self, Error> {
        let mut settings = Self::default();
        let path = match explicit {
            Some(path) => Some(path.to_path_buf()),
            None => default_config_path(),
        };

        if let Some(path) = path {
            match fs::read_to_string(&path) {
                Ok(contents) => {
                    let configured: Overrides = toml::from_str(&contents).map_err(|error| {
                        Error::invalid_input_with_source(
                            format!("invalid configuration file {}", path.display()),
                            error,
                        )
                    })?;
                    settings.apply_overrides(&configured);
                    settings.validate().map_err(|error| {
                        error.context(format!("invalid configuration file {}", path.display()))
                    })?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound && explicit.is_none() => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Err(Error::invalid_input_with_source_hidden(
                        format!("configuration file {} does not exist", path.display()),
                        error,
                    ));
                }
                Err(error) => {
                    return Err(Error::with_source(
                        format!("could not read configuration file {}", path.display()),
                        error,
                    ));
                }
            }
        }

        settings.apply_overrides(overrides);

        settings.validate()?;
        Ok(settings)
    }

    /// Applies a preset before any explicit numeric or length-rule overrides.
    pub fn apply_profile(&mut self, profile: Profile) {
        self.profile = Some(profile);
        (self.fraction, self.every, self.min_word_length) = match profile {
            Profile::Subtle => (0.3, 2, 4),
            Profile::Balanced => (0.5, 1, 2),
            Profile::Strong => (0.7, 1, 2),
        };
    }

    /// Resolves the first inclusive length rule, falling back to the global fraction.
    #[must_use]
    pub fn fraction_for_length(&self, length: usize) -> f64 {
        self.length_rules
            .iter()
            .find(|rule| length <= rule.max_length)
            .map_or(self.fraction, |rule| rule.fraction)
    }

    fn apply_overrides(&mut self, overrides: &Overrides) {
        if let Some(profile) = overrides.profile {
            self.apply_profile(profile);
        }
        if let Some(value) = overrides.fraction {
            self.fraction = value;
        }
        if let Some(value) = overrides.every {
            self.every = value;
        }
        if let Some(value) = overrides.min_word_length {
            self.min_word_length = value;
        }
        if let Some(value) = overrides.style {
            self.style = value;
        }
        if let Some(value) = overrides.emphasis {
            self.emphasis = value;
        }
        if let Some(value) = overrides.input_format {
            self.input_format = value;
        }
        if let Some(value) = overrides.format {
            self.format = value;
        }
        if let Some(value) = overrides.controls {
            self.controls = value;
        }
        if let Some(value) = overrides.skip_code {
            self.skip_code = value;
        }
        if let Some(value) = overrides.low_latency {
            self.low_latency = value;
        }
        if let Some(values) = &overrides.length_rules {
            self.length_rules.clone_from(values);
        }
        if let Some(values) = &overrides.exclude_commands {
            append_unique(&mut self.exclude_commands, values);
        }
    }

    /// Checks that all settings are within their supported ranges.
    ///
    /// # Errors
    ///
    /// Returns an error when a numeric value is out of range or a command
    /// exclusion is empty.
    pub fn validate(&self) -> Result<(), Error> {
        if !self.fraction.is_finite() || !(0.0..=1.0).contains(&self.fraction) {
            return Err(Error::invalid_input(
                "fraction must be a finite number between 0 and 1".to_owned(),
            ));
        }
        if self.every == 0 {
            return Err(Error::invalid_input(
                "every must be a positive integer".to_owned(),
            ));
        }
        if self.min_word_length == 0 {
            return Err(Error::invalid_input(
                "min_word_length must be a positive integer".to_owned(),
            ));
        }
        let mut previous = 0;
        for rule in &self.length_rules {
            if rule.max_length <= previous
                || !rule.fraction.is_finite()
                || !(0.0..=1.0).contains(&rule.fraction)
            {
                return Err(Error::invalid_input(
                    "length_rules need strictly increasing positive max_length values and finite fractions between 0 and 1".to_owned()));
            }
            previous = rule.max_length;
        }
        if self
            .exclude_commands
            .iter()
            .any(|command| command.trim().is_empty())
        {
            return Err(Error::invalid_input(
                "exclude_commands must not contain empty command names".to_owned(),
            ));
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
    use std::error::Error as StdError;
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

        assert_eq!(settings.fraction.to_bits(), 0.75_f64.to_bits());
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
        let message = error.to_string();
        assert!(message.contains("fraction must be a finite number between 0 and 1"));
        assert!(message.contains("broken.toml"));
    }

    #[test]
    fn invalid_toml_preserves_its_source_and_input_classification() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let path = temp.path().join("broken.toml");
        fs::write(&path, "fraction = [").expect("write invalid TOML");

        let error = Settings::load(Some(&path), &Overrides::default())
            .expect_err("invalid TOML must fail to load");

        assert!(error.is_invalid_input());
        assert!(error.source().is_some());
        assert!(error.to_string().contains("invalid configuration file"));
    }

    #[test]
    fn missing_explicit_config_keeps_its_message_and_io_source() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let path = temp.path().join("missing.toml");

        let error = Settings::load(Some(&path), &Overrides::default())
            .expect_err("missing explicit configuration must fail");

        let expected = format!("configuration file {} does not exist", path.display());
        assert!(error.to_string().starts_with(&expected));
        assert!(error.is_invalid_input());
        assert_eq!(error.io_kind(), Some(std::io::ErrorKind::NotFound));
        assert!(error.source().is_some());
    }
    #[test]
    fn all_presentation_overrides_are_applied_and_length_thresholds_are_inclusive() {
        use super::{
            ControlPolicy, Emphasis, InputFormat, LengthRule, OutputFormat, Profile, Style,
        };
        let overrides = Overrides {
            profile: Some(Profile::Balanced),
            fraction: Some(0.75),
            every: Some(3),
            min_word_length: Some(4),
            style: Some(Style::Always),
            emphasis: Some(Emphasis::Underline),
            input_format: Some(InputFormat::Markdown),
            format: Some(OutputFormat::Html),
            controls: Some(ControlPolicy::Preserve),
            skip_code: Some(false),
            low_latency: Some(true),
            length_rules: Some(vec![LengthRule {
                max_length: 4,
                fraction: 0.25,
            }]),
            exclude_commands: Some(vec!["custom-reader".to_owned()]),
        };
        let mut settings = Settings::new();
        settings.apply_overrides(&overrides);
        settings.validate().expect("all overrides are valid");
        assert_eq!(settings.every, 3);
        assert_eq!(settings.min_word_length, 4);
        assert_eq!(settings.style, Style::Always);
        assert_eq!(settings.emphasis, Emphasis::Underline);
        assert_eq!(settings.input_format, InputFormat::Markdown);
        assert_eq!(settings.format, OutputFormat::Html);
        assert_eq!(settings.controls, ControlPolicy::Preserve);
        assert!(!settings.skip_code);
        assert!(settings.low_latency);
        assert_eq!(
            settings.fraction_for_length(4).to_bits(),
            0.25_f64.to_bits()
        );
        assert_eq!(
            settings.fraction_for_length(5).to_bits(),
            0.75_f64.to_bits()
        );
        assert!(
            settings
                .exclude_commands
                .iter()
                .any(|command| command == "custom-reader")
        );
        let empty = Overrides::new();
        assert!(empty.fraction.is_none());
    }

    #[test]
    fn unreadable_configuration_and_empty_exclusions_are_rejected() {
        let directory = tempfile::tempdir().expect("create configuration directory");
        let error = Settings::load(Some(directory.path()), &Overrides::new())
            .expect_err("a directory is not a config file");
        assert!(
            error
                .to_string()
                .contains("could not read configuration file")
        );
        let settings = Settings {
            exclude_commands: vec![" ".to_owned()],
            ..Settings::new()
        };
        assert!(
            settings
                .validate()
                .expect_err("empty exclusion is invalid")
                .is_invalid_input()
        );
    }
}
