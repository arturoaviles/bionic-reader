//! Deterministic document transformation for applications.
//!
//! Unlike CLI configuration, these entry points never inspect terminal state or user files.

use crate::Error;
use crate::document;
use crate::segmentation::UnicodeSegmenter;
use crate::settings::{
    ControlPolicy, Emphasis, InputFormat, LengthRule, OutputFormat, Overrides, Profile, Settings,
};

use serde::{Deserialize, Serialize};

/// Application options, independent of terminal-style detection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// Whether to add bionic emphasis; conversion and control filtering still apply when false.
    pub enabled: bool,
    /// Select the first eligible word, then every Nth eligible word.
    pub every: Option<usize>,
    /// Optional profile applied before explicit numeric values.
    pub profile: Option<Profile>,
    /// Fraction of each selected word to emphasize.
    pub fraction: Option<f64>,
    /// Smallest letter-grapheme count eligible for emphasis.
    pub min_word_length: Option<usize>,
    /// Increasing inclusive length thresholds overriding the global fraction.
    pub length_rules: Option<Vec<LengthRule>>,
    /// Visual emphasis attribute for formatted output.
    pub emphasis: Option<Emphasis>,
    /// Plain text, Markdown source, or explicitly trusted terminal input.
    pub input_format: Option<InputFormat>,
    /// Terminal-control policy for formatted output.
    pub controls: Option<ControlPolicy>,
    /// Whether Markdown code is protected from additional emphasis.
    pub skip_code: Option<bool>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: true,
            every: None,
            profile: None,
            fraction: None,
            min_word_length: None,
            length_rules: None,
            emphasis: None,
            input_format: None,
            controls: None,
            skip_code: None,
        }
    }
}

impl Options {
    /// Parses explicitly supplied CLI TOML preferences without discovering or reading files.
    ///
    /// Profile values are applied before explicit numeric values. Terminal `auto` style does not
    /// consult the environment; `never` disables added emphasis. Session-only options are ignored.
    ///
    /// # Errors
    ///
    /// Returns an input error for invalid TOML, unknown settings, or invalid option values.
    #[cfg(feature = "config")]
    pub fn parse_config(contents: &str) -> Result<Self, Error> {
        let values: Overrides = toml::from_str(contents).map_err(|error| {
            Error::invalid_input_with_source("invalid configuration".to_owned(), error)
        })?;
        let options = Self {
            enabled: values.style != Some(crate::settings::Style::Never),
            every: values.every,
            profile: values.profile,
            fraction: values.fraction,
            min_word_length: values.min_word_length,
            length_rules: values.length_rules,
            emphasis: values.emphasis,
            input_format: values.input_format,
            controls: values.controls,
            skip_code: values.skip_code,
        };
        options.settings(OutputFormat::Plain)?;
        Ok(options)
    }

    fn settings(&self, format: OutputFormat) -> Result<Settings, Error> {
        let mut settings = Settings {
            format,
            ..Settings::default()
        };
        settings.apply_overrides(&Overrides {
            every: self.every,
            profile: self.profile,
            fraction: self.fraction,
            min_word_length: self.min_word_length,
            length_rules: self.length_rules.clone(),
            emphasis: self.emphasis,
            input_format: self.input_format,
            controls: self.controls,
            skip_code: self.skip_code,
            ..Overrides::default()
        });
        settings.validate()?;
        if u32::try_from(settings.every).is_err()
            || u32::try_from(settings.min_word_length).is_err()
            || settings
                .length_rules
                .iter()
                .any(|rule| u32::try_from(rule.max_length).is_err())
        {
            return Err(Error::invalid_input(
                "library word counts must not exceed 4294967295".to_owned(),
            ));
        }
        Ok(settings)
    }
}

/// A contiguous part of visible text with one bionic reading emphasis state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSegment {
    /// Literal Unicode content, without generated markup or terminal controls.
    pub text: String,
    /// Whether this part receives visual emphasis.
    pub emphasized: bool,
}

/// A reusable multilingual document reader with no per-document frequency state.
#[derive(Debug, Default)]
pub struct Reader {
    segmenter: UnicodeSegmenter,
}

impl Reader {
    /// Creates a reusable reader with the built-in multilingual segmentation data.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Converts a whole Unicode document to the explicitly selected format.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid settings, unsupported format combinations, or input limits.
    #[must_use = "use the rendered document"]
    pub fn render(
        &self,
        text: &str,
        format: OutputFormat,
        options: &Options,
    ) -> Result<String, Error> {
        let settings = options.settings(format)?;
        let output = document::render_with_segmenter(
            text.as_bytes(),
            &settings,
            options.enabled,
            &self.segmenter,
        )?;
        String::from_utf8(output).map_err(|error| {
            Error::invalid_input_with_source("rendered document is not Unicode".to_owned(), error)
        })
    }

    /// Selects text segments for custom presentation, stripping terminal controls.
    ///
    /// Each call starts a new emphasis-frequency sequence. Empty input produces no segments.
    ///
    /// # Errors
    ///
    /// Returns an error when options are invalid.
    #[must_use = "use the text segments"]
    pub fn segments(&self, text: &str, options: &Options) -> Result<Vec<TextSegment>, Error> {
        let settings = options.settings(OutputFormat::Ansi)?;
        document::validate_input(text.as_bytes(), &settings)?;
        let mut filter = document::ControlFilter::plain_text();
        let mut visible = filter.push(text.as_bytes());
        visible.extend(filter.finish());
        let visible = String::from_utf8(visible).map_err(|error| {
            Error::invalid_input_with_source("visible document is not Unicode".to_owned(), error)
        })?;
        let spans =
            document::emphasis_ranges(&visible, &settings, options.enabled, &self.segmenter);
        let mut segments = Vec::new();
        let mut offset = 0;
        for span in spans {
            if offset < span.start {
                segments.push(TextSegment {
                    text: visible[offset..span.start].to_owned(),
                    emphasized: false,
                });
            }
            segments.push(TextSegment {
                text: visible[span.clone()].to_owned(),
                emphasized: true,
            });
            offset = span.end;
        }
        if offset < visible.len() {
            segments.push(TextSegment {
                text: visible[offset..].to_owned(),
                emphasized: false,
            });
        }
        Ok(segments)
    }
}
