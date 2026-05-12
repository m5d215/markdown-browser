//! Pluggable rendering for blocks that *could* show a graphic but currently
//! degrade to text. Images (Sixel/Kitty/iTerm2 inline) and mermaid diagrams
//! both flow through this trait. The default `TextOnlyRenderer` is the only
//! implementation today; alternative backends can be slotted in without
//! restructuring callers.
//!
//! See the project README for why graphical rendering is intentionally
//! deferred but kept pluggable.

use crate::render::style::StyledLine;

#[derive(Debug, Clone)]
pub struct ImageRequest<'a> {
    pub alt: &'a str,
    pub url: &'a str,
    pub title: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct FencedBlockRequest<'a> {
    pub language: &'a str,
    pub body: &'a str,
}

pub trait MediaRenderer {
    /// Render an inline/block image. Returning `None` means "fall back to the
    /// caller's default text rendering" (e.g. `alt (url)`).
    fn render_image(&self, _req: &ImageRequest<'_>) -> Option<Vec<StyledLine>> {
        None
    }

    /// Render a fenced code block whose language hints at non-code content
    /// (`mermaid`, `dot`, `plantuml`, ...). Returning `None` means "treat as
    /// a normal code block".
    fn render_fenced(&self, _req: &FencedBlockRequest<'_>) -> Option<Vec<StyledLine>> {
        None
    }
}

/// Default backend: always defers to text rendering. Picks up images and
/// mermaid blocks as ordinary text content.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextOnlyRenderer;

impl MediaRenderer for TextOnlyRenderer {}
