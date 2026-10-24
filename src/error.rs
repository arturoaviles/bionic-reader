use std::backtrace::{Backtrace, BacktraceStatus};
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::io;

type BoxedError = Box<dyn StdError + Send + Sync + 'static>;

/// An error returned by a bionic-reader operation.
///
/// The displayed message includes relevant source context. Some errors retain a source only
/// for inspection, preserving an established user-facing message. A captured backtrace is
/// included when Rust backtrace capture is enabled, such as with `RUST_BACKTRACE=1`.
/// Alternate display (`format!("{error:#}")`) omits the backtrace.
///
/// # Examples
///
/// ```
/// use bionic_reader::settings::Settings;
///
/// let error = Settings {
///     every: 0,
///     ..Settings::default()
/// }
/// .validate()
/// .expect_err("zero is not a supported frequency");
///
/// assert!(error.is_invalid_input());
/// assert!(error.to_string().contains("every must be a positive integer"));
/// ```
#[derive(Debug)]
pub struct Error {
    message: String,
    source: Option<BoxedError>,
    backtrace: Backtrace,
    invalid_input: bool,
    display_source: bool,
}

impl Error {
    pub(crate) fn new(message: String) -> Self {
        Self::with_kind(message, None, false, false)
    }

    pub(crate) fn invalid_input(message: String) -> Self {
        Self::with_kind(message, None, true, false)
    }

    pub(crate) fn with_source(message: String, source: impl Into<BoxedError>) -> Self {
        Self::with_kind(message, Some(source.into()), false, true)
    }

    pub(crate) fn with_source_hidden(message: String, source: impl Into<BoxedError>) -> Self {
        Self::with_kind(message, Some(source.into()), false, false)
    }

    pub(crate) fn invalid_input_with_source_hidden(
        message: String,
        source: impl Into<BoxedError>,
    ) -> Self {
        Self::with_kind(message, Some(source.into()), true, false)
    }

    pub(crate) fn invalid_input_with_source(
        message: String,
        source: impl Into<BoxedError>,
    ) -> Self {
        Self::with_kind(message, Some(source.into()), true, true)
    }

    fn with_kind(
        message: String,
        source: Option<BoxedError>,
        invalid_input: bool,
        display_source: bool,
    ) -> Self {
        Self {
            message,
            source,
            backtrace: Backtrace::capture(),
            invalid_input,
            display_source,
        }
    }

    /// Returns the captured backtrace, which may be disabled by the environment.
    #[must_use = "inspect the captured diagnostic backtrace"]
    pub fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }

    /// Returns the source I/O error kind when this error wraps an I/O error.
    #[must_use]
    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        let mut source = self.source();
        while let Some(error) = source {
            if let Some(error) = error.downcast_ref::<io::Error>() {
                return Some(error.kind());
            }
            source = error.source();
        }
        None
    }

    /// Whether this error was caused by invalid caller or configuration input.
    #[must_use]
    pub fn is_invalid_input(&self) -> bool {
        self.invalid_input
    }

    pub(crate) fn context(mut self, message: String) -> Self {
        if self.message.is_empty() {
            self.message = message;
        } else {
            self.message = format!("{message}: {}", self.message);
        }
        self
    }

    fn format(&self, formatter: &mut Formatter<'_>, include_backtrace: bool) -> fmt::Result {
        formatter.write_str(&self.message)?;
        if let (true, Some(source)) = (self.display_source, &self.source) {
            formatter.write_str(": ")?;
            if let Some(error) = source.downcast_ref::<Self>() {
                error.format(formatter, false)?;
            } else {
                Display::fmt(source, formatter)?;
            }
        }
        if include_backtrace {
            formatter.write_str("\n")?;
            Display::fmt(&self.backtrace, formatter)?;
        }
        Ok(())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.format(
            f,
            !f.alternate() && self.backtrace.status() == BacktraceStatus::Captured,
        )
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn StdError + 'static))
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use std::backtrace::{Backtrace, BacktraceStatus};
    use std::error::Error as StdError;
    use std::io;

    #[test]
    fn source_and_io_kind_are_preserved() {
        let error = Error::with_source(
            "could not write terminal output".to_owned(),
            io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"),
        );

        assert_eq!(error.io_kind(), Some(io::ErrorKind::BrokenPipe));
        assert_eq!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<io::Error>())
                .map(io::Error::kind),
            Some(io::ErrorKind::BrokenPipe)
        );
        assert!(
            error
                .to_string()
                .starts_with("could not write terminal output: closed pipe")
        );
    }

    #[test]
    fn io_kind_follows_nested_error_sources() {
        let inner = Error::with_source(
            "inner operation".to_owned(),
            io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"),
        );
        let outer = Error::with_source("outer operation".to_owned(), inner);
        assert_eq!(outer.io_kind(), Some(io::ErrorKind::BrokenPipe));
    }

    #[test]
    fn nested_errors_display_only_the_outer_backtrace() {
        let inner = Error {
            message: "inner operation".to_owned(),
            source: None,
            backtrace: Backtrace::force_capture(),
            invalid_input: false,
            display_source: false,
        };
        let mut outer = Error::with_source("outer operation".to_owned(), inner);
        outer.backtrace = Backtrace::force_capture();
        assert_eq!(
            outer.to_string(),
            format!("outer operation: inner operation\n{}", outer.backtrace())
        );
        assert_eq!(format!("{outer:#}"), "outer operation: inner operation");
    }

    #[test]
    fn context_flattens_messages_and_keeps_the_source() {
        let error = Error::with_source(
            "invalid configuration".to_owned(),
            io::Error::new(io::ErrorKind::PermissionDenied, "access denied"),
        )
        .context("could not load settings".to_owned());

        assert!(
            error
                .to_string()
                .starts_with("could not load settings: invalid configuration: access denied")
        );
        assert_eq!(error.io_kind(), Some(io::ErrorKind::PermissionDenied));
    }

    #[test]
    fn backtrace_is_exposed_and_displayed_only_when_captured() {
        let error = Error {
            message: "operation failed".to_owned(),
            source: None,
            backtrace: Backtrace::disabled(),
            invalid_input: false,
            display_source: false,
        };
        assert_eq!(error.backtrace().status(), BacktraceStatus::Disabled);
        assert_eq!(error.to_string(), "operation failed");

        let captured = Error {
            message: "operation failed".to_owned(),
            source: None,
            backtrace: Backtrace::force_capture(),
            invalid_input: false,
            display_source: false,
        };
        let rendered = captured.to_string();
        assert!(rendered.starts_with("operation failed\n"));
        assert!(rendered.contains(&captured.backtrace().to_string()));
    }

    #[test]
    fn invalid_input_classification_survives_context() {
        let error = Error::invalid_input("unsupported shell".to_owned())
            .context("could not initialize shell".to_owned());

        assert!(error.is_invalid_input());
        assert!(
            error
                .to_string()
                .starts_with("could not initialize shell: unsupported shell")
        );
    }

    #[test]
    fn hidden_source_is_still_inspectable() {
        let error = Error::invalid_input_with_source_hidden(
            "configuration file does not exist".to_owned(),
            io::Error::new(io::ErrorKind::NotFound, "missing"),
        );

        assert!(error.is_invalid_input());
        assert_eq!(error.io_kind(), Some(io::ErrorKind::NotFound));
        assert!(
            error
                .to_string()
                .starts_with("configuration file does not exist")
        );
    }
    #[test]
    fn empty_context_and_hidden_operating_system_sources_keep_diagnostics_precise() {
        let error = super::Error::with_source_hidden(
            String::new(),
            std::io::Error::other("private detail"),
        )
        .context("public explanation".to_owned());
        assert_eq!(error.to_string(), "public explanation");
        assert_eq!(error.io_kind(), Some(std::io::ErrorKind::Other));
        assert!(std::error::Error::source(&error).is_some());
    }
}
