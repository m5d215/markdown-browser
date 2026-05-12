//! Display-width helpers. Width is computed from logical text content
//! (post-AST), so ANSI escape sequences never enter the calculation.

use unicode_width::UnicodeWidthStr;

use crate::render::style::StyledSpan;

pub fn span_width(span: &StyledSpan) -> usize {
    UnicodeWidthStr::width(span.text.as_str())
}

pub fn spans_width(spans: &[StyledSpan]) -> usize {
    spans.iter().map(span_width).sum()
}
