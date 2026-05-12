//! Block-level rendering. Walks the comrak AST top-down, emitting
//! `StyledLine`s. Each block is responsible for any trailing blank line that
//! separates it from the next sibling.

use comrak::nodes::{AstNode, ListType, NodeValue};

use crate::render::image::MediaRenderer;
use crate::render::inline;
use crate::render::style::{Style, StyledLine, StyledSpan};
use crate::render::table;
use crate::render::theme::Theme;

pub struct RenderContext<'r> {
    pub theme: &'r Theme,
    pub media: &'r dyn MediaRenderer,
}

pub fn render_document<'a>(root: &'a AstNode<'a>, ctx: &RenderContext<'_>) -> Vec<StyledLine> {
    let mut out = Vec::new();
    render_blocks(root, ctx, "", &mut out);
    trim_trailing_blank(&mut out);
    out
}

fn render_blocks<'a>(
    parent: &'a AstNode<'a>,
    ctx: &RenderContext<'_>,
    indent: &str,
    out: &mut Vec<StyledLine>,
) {
    let mut first = true;
    for child in parent.children() {
        if !first {
            out.push(StyledLine::new());
        }
        first = false;
        render_block(child, ctx, indent, out);
    }
}

fn render_block<'a>(
    node: &'a AstNode<'a>,
    ctx: &RenderContext<'_>,
    indent: &str,
    out: &mut Vec<StyledLine>,
) {
    let data = node.data.borrow();
    match &data.value {
        NodeValue::Document => {
            render_blocks(node, ctx, indent, out);
        }

        NodeValue::Heading(h) => {
            let style = ctx.theme.heading(h.level as u32);
            let marker = format!("{} ", "#".repeat(h.level as usize));
            let mut lines = vec![StyledLine::new()];
            lines[0].push_styled(marker, style);
            inline::render_inlines(node, style, ctx.theme, &mut lines);
            prepend_indent(&mut lines, indent);
            out.extend(lines);
        }

        NodeValue::Paragraph => {
            let mut lines = Vec::new();
            inline::render_inlines(node, ctx.theme.paragraph, ctx.theme, &mut lines);
            prepend_indent(&mut lines, indent);
            out.extend(lines);
        }

        NodeValue::BlockQuote => {
            let mut inner = Vec::new();
            render_blocks(node, ctx, "", &mut inner);
            for mut line in inner {
                let marker = StyledSpan::new("│ ", ctx.theme.blockquote_marker);
                if line.is_empty() {
                    let mut new_line = StyledLine::new();
                    new_line.push_span(marker);
                    line = new_line;
                } else {
                    let mut spans = vec![marker];
                    spans.extend(line.spans.into_iter().map(|s| StyledSpan {
                        text: s.text,
                        style: s.style.merge(ctx.theme.blockquote),
                    }));
                    line = StyledLine { spans };
                }
                let mut wrapped = StyledLine::new();
                if !indent.is_empty() {
                    wrapped.push_plain(indent.to_string());
                }
                wrapped.spans.extend(line.spans);
                out.push(wrapped);
            }
        }

        NodeValue::List(list) => {
            let mut counter = list.start.max(1);
            for (idx, item) in node.children().enumerate() {
                if idx > 0 && list.tight == false {
                    out.push(StyledLine::new());
                }
                let marker = match list.list_type {
                    ListType::Bullet => "• ".to_string(),
                    ListType::Ordered => {
                        let s = format!("{counter}. ");
                        counter += 1;
                        s
                    }
                };
                render_list_item(item, ctx, indent, &marker, out);
            }
        }

        NodeValue::Item(_) => {
            // Items are handled by their parent List, but we tolerate
            // bare Item nodes by rendering their children.
            render_blocks(node, ctx, indent, out);
        }

        NodeValue::TaskItem(task) => {
            let done = task.symbol.is_some();
            let marker = if done { "[x] " } else { "[ ] " };
            let marker_style = if done {
                ctx.theme.task_marker_done
            } else {
                ctx.theme.task_marker_todo
            };
            render_task_item(node, ctx, indent, marker, marker_style, out);
        }

        NodeValue::CodeBlock(code) => {
            let fence_text = if code.info.is_empty() {
                "```".to_string()
            } else {
                format!("``` {}", code.info)
            };
            let mut top = StyledLine::new();
            if !indent.is_empty() {
                top.push_plain(indent.to_string());
            }
            top.push_styled(fence_text, ctx.theme.code_fence);
            out.push(top);
            for raw_line in code.literal.split('\n') {
                if raw_line.is_empty() && code.literal.ends_with('\n') {
                    // The split produces a trailing empty piece for a
                    // trailing newline; skip it.
                    continue;
                }
                let mut line = StyledLine::new();
                if !indent.is_empty() {
                    line.push_plain(indent.to_string());
                }
                line.push_styled(raw_line.to_string(), ctx.theme.code_block);
                out.push(line);
            }
            let mut bottom = StyledLine::new();
            if !indent.is_empty() {
                bottom.push_plain(indent.to_string());
            }
            bottom.push_styled("```", ctx.theme.code_fence);
            out.push(bottom);
        }

        NodeValue::HtmlBlock(html) => {
            for raw_line in html.literal.lines() {
                let mut line = StyledLine::new();
                if !indent.is_empty() {
                    line.push_plain(indent.to_string());
                }
                line.push_styled(raw_line.to_string(), ctx.theme.code_inline);
                out.push(line);
            }
        }

        NodeValue::ThematicBreak => {
            let mut line = StyledLine::new();
            if !indent.is_empty() {
                line.push_plain(indent.to_string());
            }
            line.push_styled("─".repeat(40), ctx.theme.thematic_break);
            out.push(line);
        }

        NodeValue::Table(_) => {
            let mut lines = Vec::new();
            table::render_table(node, ctx.theme, &mut lines);
            prepend_indent(&mut lines, indent);
            out.extend(lines);
        }

        // TableRow / TableCell are walked by the table renderer; bare
        // occurrences would be malformed, but ignore them gracefully.
        NodeValue::TableRow(_) | NodeValue::TableCell => {}

        // Anything else (footnotes, etc.) — recurse so content isn't dropped.
        _ => {
            render_blocks(node, ctx, indent, out);
        }
    }
}

fn render_list_item<'a>(
    item: &'a AstNode<'a>,
    ctx: &RenderContext<'_>,
    indent: &str,
    marker: &str,
    out: &mut Vec<StyledLine>,
) {
    let marker_width = display_width(marker);
    let child_indent = format!("{indent}{}", " ".repeat(marker_width));

    let mut inner = Vec::new();
    render_blocks(item, ctx, "", &mut inner);

    for (idx, line) in inner.into_iter().enumerate() {
        let mut wrapped = StyledLine::new();
        if !indent.is_empty() {
            wrapped.push_plain(indent.to_string());
        }
        if idx == 0 {
            wrapped.push_styled(marker.to_string(), ctx.theme.list_marker);
        } else {
            wrapped.push_plain(" ".repeat(marker_width));
        }
        wrapped.spans.extend(line.spans);
        out.push(wrapped);
        let _ = &child_indent; // suppress unused-warning in the unlikely case
    }
}

fn render_task_item<'a>(
    item: &'a AstNode<'a>,
    ctx: &RenderContext<'_>,
    indent: &str,
    marker: &str,
    marker_style: Style,
    out: &mut Vec<StyledLine>,
) {
    let marker_width = display_width(marker);

    let mut inner = Vec::new();
    render_blocks(item, ctx, "", &mut inner);

    for (idx, line) in inner.into_iter().enumerate() {
        let mut wrapped = StyledLine::new();
        if !indent.is_empty() {
            wrapped.push_plain(indent.to_string());
        }
        if idx == 0 {
            wrapped.push_styled(marker.to_string(), marker_style);
        } else {
            wrapped.push_plain(" ".repeat(marker_width));
        }
        wrapped.spans.extend(line.spans);
        out.push(wrapped);
    }
}

fn prepend_indent(lines: &mut [StyledLine], indent: &str) {
    if indent.is_empty() {
        return;
    }
    for line in lines.iter_mut() {
        let mut spans = Vec::with_capacity(line.spans.len() + 1);
        spans.push(StyledSpan::plain(indent.to_string()));
        spans.append(&mut line.spans);
        line.spans = spans;
    }
}

fn trim_trailing_blank(out: &mut Vec<StyledLine>) {
    while matches!(out.last(), Some(l) if l.is_empty()) {
        out.pop();
    }
}

fn display_width(s: &str) -> usize {
    // ASCII-only call sites for now; widen when CJK markers appear.
    s.chars().count()
}
