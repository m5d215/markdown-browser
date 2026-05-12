//! Pure rendering layer.
//!
//! Converts a parsed markdown document into a sequence of styled lines plus
//! supporting metadata (heading anchors), independent of any output device
//! (stdout ANSI escape, ratatui widget, snapshot test, etc.). Sinks live
//! above this module and consume the same line stream.

use comrak::nodes::AstNode;

pub mod anchor;
pub mod block;
pub mod highlight;
pub mod image;
pub mod inline;
pub mod link;
pub mod parse;
pub mod style;
pub mod table;
pub mod theme;
pub mod width;

pub use anchor::Anchor;
pub use image::{MediaRenderer, TextOnlyRenderer};
pub use link::Link;
pub use style::{Color, Style, StyledLine, StyledSpan};
pub use theme::Theme;

#[derive(Debug, Default, Clone)]
pub struct RenderOutput {
    pub lines: Vec<StyledLine>,
    pub anchors: Vec<Anchor>,
    pub links: Vec<Link>,
    /// Block ranges in emission order. Used by yank-mode expand/shrink to
    /// map a cursor line to the enclosing paragraph / list-item / blockquote
    /// / code block / table.
    pub blocks: Vec<BlockRange>,
}

/// Inclusive line range for a markdown block.
#[derive(Debug, Clone, Copy)]
pub struct BlockRange {
    pub start: usize,
    pub end: usize,
    pub kind: BlockKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// Leaf block: paragraph, heading, code block, table, thematic break,
    /// html block. The "paragraph" stop in the yank expand path.
    Leaf,
    /// Container block: list item or blockquote. The "enclosing container"
    /// stop in the yank expand path.
    Container,
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
