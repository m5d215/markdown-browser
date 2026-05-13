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
    /// Fenced code blocks whose info string starts with `mermaid`. Captures
    /// the inclusive logical-line range covered by the block (fences
    /// included) plus the raw source so the embedded preview server can
    /// stream it to a browser when the cursor lands inside the range.
    pub mermaid_blocks: Vec<MermaidBlock>,
}

/// A fenced code block tagged as `mermaid`, captured during render so the
/// preview server can find it by cursor line.
#[derive(Debug, Clone)]
pub struct MermaidBlock {
    /// Inclusive logical-line range covering the full block including fences.
    pub start: usize,
    pub end: usize,
    /// Raw mermaid source (the body between the fences), unmodified.
    pub source: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::parse;
    use comrak::Arena;

    fn render(input: &str) -> RenderOutput {
        let arena = Arena::new();
        let root = parse::parse(&arena, input, false);
        render_document(root)
    }

    #[test]
    fn mermaid_block_is_captured() {
        let input = "before\n\n```mermaid\ngraph TD\nA-->B\n```\n\nafter\n";
        let out = render(input);
        assert_eq!(out.mermaid_blocks.len(), 1);
        let b = &out.mermaid_blocks[0];
        assert_eq!(b.source, "graph TD\nA-->B\n");
        // Range covers the fence top through fence bottom inclusive.
        assert!(b.start < b.end);
        let fence_top = &out.lines[b.start];
        let fence_bottom = &out.lines[b.end];
        let top_text: String = fence_top.spans.iter().map(|s| s.text.as_str()).collect();
        let bot_text: String = fence_bottom.spans.iter().map(|s| s.text.as_str()).collect();
        assert!(top_text.contains("mermaid"));
        assert_eq!(bot_text.trim(), "```");
    }

    #[test]
    fn non_mermaid_code_block_is_ignored() {
        let input = "```rust\nfn main() {}\n```\n";
        let out = render(input);
        assert!(out.mermaid_blocks.is_empty());
    }

    #[test]
    fn multiple_mermaid_blocks_are_captured_in_order() {
        let input =
            "```mermaid\ngraph TD\nA-->B\n```\n\nmiddle\n\n```mermaid\ngraph LR\nC-->D\n```\n";
        let out = render(input);
        assert_eq!(out.mermaid_blocks.len(), 2);
        assert!(out.mermaid_blocks[0].source.contains("A-->B"));
        assert!(out.mermaid_blocks[1].source.contains("C-->D"));
        assert!(out.mermaid_blocks[0].end < out.mermaid_blocks[1].start);
    }

    #[test]
    fn mermaid_lang_match_is_case_insensitive() {
        let input = "```Mermaid\ngraph TD\nA-->B\n```\n";
        let out = render(input);
        assert_eq!(out.mermaid_blocks.len(), 1);
    }
}
