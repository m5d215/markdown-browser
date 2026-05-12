//! Pure rendering layer.
//!
//! Converts a parsed markdown document into a sequence of styled lines plus
//! supporting metadata (heading anchors), independent of any output device
//! (stdout ANSI escape, ratatui widget, snapshot test, etc.). Sinks live
//! above this module and consume the same line stream.

use comrak::nodes::AstNode;

pub mod anchor;
pub mod block;
pub mod image;
pub mod inline;
pub mod parse;
pub mod style;
pub mod table;
pub mod theme;
pub mod width;

pub use anchor::Anchor;
pub use image::{MediaRenderer, TextOnlyRenderer};
pub use style::{Color, Style, StyledLine, StyledSpan};
pub use theme::Theme;

#[derive(Debug, Default, Clone)]
pub struct RenderOutput {
    pub lines: Vec<StyledLine>,
    pub anchors: Vec<Anchor>,
}

/// Render a parsed markdown document.
pub fn render_document<'a>(root: &'a AstNode<'a>) -> RenderOutput {
    let theme = Theme::default();
    let media = TextOnlyRenderer;
    let ctx = block::RenderContext {
        theme: &theme,
        media: &media,
    };
    block::render_document(root, &ctx)
}
