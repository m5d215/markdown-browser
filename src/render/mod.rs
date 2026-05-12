//! Pure rendering layer.
//!
//! Converts a parsed markdown document into a sequence of styled lines,
//! independent of any output device (stdout ANSI escape, ratatui widget,
//! snapshot test, etc.). Sinks live above this module and consume the
//! same line stream.

use comrak::nodes::AstNode;

pub mod block;
pub mod image;
pub mod inline;
pub mod style;
pub mod theme;

pub use image::{MediaRenderer, TextOnlyRenderer};
pub use style::{Color, Style, StyledLine, StyledSpan};
pub use theme::Theme;

/// Render a parsed markdown document into a sequence of styled lines.
pub fn render_document<'a>(root: &'a AstNode<'a>) -> Vec<StyledLine> {
    let theme = Theme::default();
    let media = TextOnlyRenderer;
    let ctx = block::RenderContext {
        theme: &theme,
        media: &media,
    };
    block::render_document(root, &ctx)
}
