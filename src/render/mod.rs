//! Pure rendering layer.
//!
//! Converts a parsed markdown document into a sequence of styled lines,
//! independent of any output device (stdout ANSI escape, ratatui widget,
//! snapshot test, etc.). Sinks live above this module and consume the
//! same line stream.

pub mod block;
pub mod image;
pub mod inline;
pub mod style;

pub use style::{Style, StyledLine, StyledSpan};
