//! Bionic reading for application documents, byte streams, and terminal sessions.
//!
//! [`reading`] provides deterministic whole-string transformations and text segments.
//! Optional native and CLI features add filesystem, shell, and PTY session modules. [`Error`] preserves source errors and supports I/O error classification for callers.

mod error;

#[doc(inline)]
pub use error::Error;

pub mod document;
#[cfg(feature = "cli")]
pub mod reader;
pub mod reading;
pub mod segmentation;
#[cfg(feature = "cli")]
pub mod session;
pub mod settings;
#[cfg(feature = "cli")]
pub mod shell;
pub mod transformer;
