//! Inline-link metadata produced alongside `StyledLine`s. The TUI uses these
//! records to highlight the focused link and resolve `Enter` to a navigation
//! action.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Index into `RenderOutput.lines` where the link text begins.
    pub line: usize,
    /// Range over `StyledLine.spans` covered by the link text on that line.
    pub span_range: Range<usize>,
    /// Raw URL/path/anchor as written in the markdown source.
    pub url: String,
}
