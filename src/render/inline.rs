//! Inline-level rendering. Walks inline AST nodes and emits styled spans
//! into the current line, splitting on hard line breaks.

use comrak::nodes::{AstNode, NodeValue};

use crate::render::link::Link;
use crate::render::style::{Style, StyledLine, StyledSpan};
use crate::render::theme::Theme;

/// Render inline children of `parent`, appending styled spans to `out` and
/// recording any inline links into `links`. New lines are produced when a
/// hard linebreak is encountered.
pub fn render_inlines<'a>(
    parent: &'a AstNode<'a>,
    base: Style,
    theme: &Theme,
    out: &mut Vec<StyledLine>,
    links: &mut Vec<Link>,
) {
    if out.is_empty() {
        out.push(StyledLine::new());
    }
    for child in parent.children() {
        render_inline(child, base, theme, out, links);
    }
}

fn render_inline<'a>(
    node: &'a AstNode<'a>,
    base: Style,
    theme: &Theme,
    out: &mut Vec<StyledLine>,
    links: &mut Vec<Link>,
) {
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
                render_inline(child, style, theme, out, links);
            }
        }

        NodeValue::Strong => {
            let style = base.merge(theme.strong);
            for child in node.children() {
                render_inline(child, style, theme, out, links);
            }
        }

        NodeValue::Strikethrough => {
            let style = base.merge(theme.strikethrough);
            for child in node.children() {
                render_inline(child, style, theme, out, links);
            }
        }

        NodeValue::Link(link) => {
            let text_style = base.merge(theme.link_text);
            let line_before = out.len().saturating_sub(1);
            let span_before = out.last().map(|l| l.spans.len()).unwrap_or(0);

            for child in node.children() {
                render_inline(child, text_style, theme, out, links);
            }

            // Only record links that stayed on the same line. Multi-line
            // links can't host an in-line focus highlight cleanly yet.
            if out.len().saturating_sub(1) == line_before {
                let span_after = out[line_before].spans.len();
                if span_before < span_after {
                    links.push(Link {
                        line: line_before,
                        span_range: span_before..span_after,
                        url: link.url.clone(),
                    });
                }
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
                render_inline(child, alt_style, theme, out, links);
            }
            if !link.url.is_empty() {
                push(out, " (", base.merge(theme.link_url));
                push(out, &link.url, base.merge(theme.link_url));
                push(out, ")", base.merge(theme.link_url));
            }
        }

        NodeValue::HtmlInline(html) => push(out, html, base.merge(theme.code_inline)),

        NodeValue::ShortCode(sc) => push(out, &sc.emoji, base),

        _ => {
            for child in node.children() {
                render_inline(child, base, theme, out, links);
            }
        }
    }
}

fn push(out: &mut Vec<StyledLine>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
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
