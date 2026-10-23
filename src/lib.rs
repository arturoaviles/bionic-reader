//! Bionic reading for byte streams and interactive terminal sessions.
//!
//! The crate exposes configuration, shell initialization, byte transformation, and PTY session
//! modules. [`Error`] preserves source errors and supports I/O error classification for callers.

mod error;

#[doc(inline)]
pub use error::Error;

pub mod session;
pub mod settings;
pub mod shell;
pub mod transformer;
