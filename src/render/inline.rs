//! Inline-level rendering. Walks inline AST nodes and emits styled spans
//! into the current line, splitting on hard line breaks.

use comrak::nodes::{AstNode, NodeValue};

use crate::render::style::{Style, StyledLine, StyledSpan};
use crate::render::theme::Theme;

/// Render inline children of `parent`, appending styled spans to `out`. New
/// lines are produced when a hard linebreak is encountered.
pub fn render_inlines<'a>(
    parent: &'a AstNode<'a>,
    base: Style,
    theme: &Theme,
    out: &mut Vec<StyledLine>,
) {
    if out.is_empty() {
        out.push(StyledLine::new());
    }
    for child in parent.children() {
        render_inline(child, base, theme, out);
    }
}

fn render_inline<'a>(node: &'a AstNode<'a>, base: Style, theme: &Theme, out: &mut Vec<StyledLine>) {
    let data = node.data.borrow();
    match &data.value {
        NodeValue::Text(text) => push(out, text, base),

        NodeValue::SoftBreak => push(out, " ", base),

        NodeValue::LineBreak => out.push(StyledLine::new()),

        NodeValue::Code(code) => {
            push(out, &code.literal, base.merge(theme.code_inline));
        }

        NodeValue::Emph => {
            let style = base.merge(theme.emph);
            for child in node.children() {
                render_inline(child, style, theme, out);
            }
        }

        NodeValue::Strong => {
            let style = base.merge(theme.strong);
            for child in node.children() {
                render_inline(child, style, theme, out);
            }
        }

        NodeValue::Strikethrough => {
            let style = base.merge(theme.strikethrough);
            for child in node.children() {
                render_inline(child, style, theme, out);
            }
        }

        NodeValue::Link(link) => {
            let text_style = base.merge(theme.link_text);
            for child in node.children() {
                render_inline(child, text_style, theme, out);
            }
            if !link.url.is_empty() {
                push(out, " (", base.merge(theme.link_url));
                push(out, &link.url, base.merge(theme.link_url));
                push(out, ")", base.merge(theme.link_url));
            }
        }

        NodeValue::Image(link) => {
            let alt_style = base.merge(theme.image_alt);
            push(out, "🖼 ", alt_style);
            for child in node.children() {
                render_inline(child, alt_style, theme, out);
            }
            if !link.url.is_empty() {
                push(out, " (", base.merge(theme.link_url));
                push(out, &link.url, base.merge(theme.link_url));
                push(out, ")", base.merge(theme.link_url));
            }
        }

        NodeValue::HtmlInline(html) => push(out, html, base.merge(theme.code_inline)),

        // Fall back to walking children for anything else (e.g. footnote refs).
        _ => {
            for child in node.children() {
                render_inline(child, base, theme, out);
            }
        }
    }
}

fn push(out: &mut Vec<StyledLine>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    // Inline text occasionally contains embedded newlines (e.g. raw HTML).
    let mut first = true;
    for piece in text.split('\n') {
        if !first {
            out.push(StyledLine::new());
        }
        first = false;
        if piece.is_empty() {
            continue;
        }
        let line = out.last_mut().expect("at least one line exists");
        line.push_span(StyledSpan::new(piece, style));
    }
}
