//! Shell initialization snippets for interactive reading sessions.
//!
//! [`init`] returns the configuration code for Bash, Zsh, or fish. When a shell is already
//! managed by a PTY reading session, the code installs command-boundary hooks; otherwise, it
//! launches the session wrapper around the interactive shell.
//!
//! # Examples
//!
//! ```
//! let snippet = bionic_reader::shell::init("bash").expect("Bash is supported");
//! assert!(snippet.contains("BIONIC_READER_SESSION"));
//! ```

use crate::Error;

/// Return the shell code used to opt an interactive shell into a reading session.
///
/// With `BIONIC_READER_SESSION=1`, the snippet installs the command-boundary
/// hooks for a shell already managed by the PTY wrapper. Otherwise it launches
/// one wrapper around the current interactive shell.
///
/// # Errors
///
/// Returns an error when `shell` does not name `bash`, `zsh`, or `fish`.
///
pub fn init(shell: &str) -> Result<String, Error> {
    let (_, template) = shell_template(shell)?;
    Ok(template.to_owned())
}

/// Reduces a shell path to its supported shell name.
///
/// # Errors
///
/// Returns an error when the basename is not `bash`, `zsh`, or `fish`.
pub(crate) fn normalize_shell(shell: &str) -> Result<&'static str, Error> {
    shell_template(shell).map(|(name, _template)| name)
}

// Resolve the name and integration together so they cannot disagree at runtime.
fn shell_template(shell: &str) -> Result<(&'static str, &'static str), Error> {
    match shell.rsplit('/').next().unwrap_or(shell) {
        "bash" => Ok(("bash", include_str!("../integrations/bash.sh"))),
        "zsh" => Ok(("zsh", include_str!("../integrations/zsh.sh"))),
        "fish" => Ok(("fish", include_str!("../integrations/fish.fish"))),
        other => Err(Error::invalid_input(format!(
            "unsupported shell `{other}`; expected bash, zsh, or fish"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unsupported_shell_names_and_paths_are_rejected() {
        for shell in ["", "/bin/", "sh", "/bin/powershell"] {
            let error = super::init(shell).expect_err("unsupported shell must fail");
            assert!(error.is_invalid_input());
            assert!(error.to_string().contains("expected bash, zsh, or fish"));
        }
    }
}
