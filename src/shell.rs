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
/// # Panics
///
/// Panics if the shell normalization logic returns a name without a template.
pub fn init(shell: &str) -> Result<String, Error> {
    let shell = normalize_shell(shell)?;
    let template = match shell {
        "bash" => include_str!("../integrations/bash.sh"),
        "zsh" => include_str!("../integrations/zsh.sh"),
        "fish" => include_str!("../integrations/fish.fish"),
        _ => unreachable!("normalize_shell returned an unsupported shell"),
    };
    Ok(template.to_owned())
}

/// Reduces a shell path to its supported shell name.
///
/// # Errors
///
/// Returns an error when the basename is not `bash`, `zsh`, or `fish`.
pub(crate) fn normalize_shell(shell: &str) -> Result<&'static str, Error> {
    match shell.rsplit('/').next().unwrap_or(shell) {
        "bash" => Ok("bash"),
        "zsh" => Ok("zsh"),
        "fish" => Ok("fish"),
        other => Err(Error::invalid_input(format!(
            "unsupported shell `{other}`; expected bash, zsh, or fish"
        ))),
    }
}
