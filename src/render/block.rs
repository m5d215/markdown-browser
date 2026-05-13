//! Block-level rendering. Walks the comrak AST top-down, emitting styled
//! lines and recording heading anchors as it goes. Each block is responsible
//! for any trailing blank line that separates it from the next sibling.

use comrak::nodes::{AstNode, ListType, NodeValue};

use crate::render::anchor::{Anchor, slugify};
use crate::render::highlight;
use crate::render::image::MediaRenderer;
use crate::render::inline;
use crate::render::style::{Style, StyledLine, StyledSpan};
use crate::render::table;
use crate::render::theme::Theme;
use crate::render::{BlockKind, BlockRange, MermaidBlock, RenderOutput};

pub struct RenderContext<'r> {
    pub theme: &'r Theme,
    pub media: &'r dyn MediaRenderer,
}

pub fn render_document<'a>(root: &'a AstNode<'a>, ctx: &RenderContext<'_>) -> RenderOutput {
    let mut out = RenderOutput::default();
    render_blocks(root, ctx, "", true, &mut out);
    trim_trailing_blank(&mut out.lines);
    out
}

/// Walk every direct child block of `parent`, emitting them in order. When
/// `separate` is true a blank line is inserted between adjacent blocks
/// (the usual document layout). Tight-list item interiors pass `false` to
/// keep their inner paragraphs / nested lists compact, matching CommonMark.
fn render_blocks<'a>(
    parent: &'a AstNode<'a>,
    ctx: &RenderContext<'_>,
    indent: &str,
    separate: bool,
    out: &mut RenderOutput,
) {
    let mut first = true;
    for child in parent.children() {
        if !first && separate {
            out.lines.push(StyledLine::new());
        }
        first = false;
        render_block(child, ctx, indent, out);
    }
}

fn render_block<'a>(
    node: &'a AstNode<'a>,
    ctx: &RenderContext<'_>,
    indent: &str,
    out: &mut RenderOutput,
) {
    let data = node.data.borrow();
    match &data.value {
        NodeValue::Document => {
            drop(data);
            render_blocks(node, ctx, indent, true, out);
        }

        NodeValue::FrontMatter(literal) => {
            let literal = literal.clone();
            drop(data);
            let start = out.lines.len();
            render_front_matter(&literal, indent, ctx.theme, out);
            push_block(out, start, BlockKind::Leaf);
        }

        NodeValue::Heading(h) => {
            let level = h.level;
            drop(data);

            let plain = collect_plain_text(node);
            let slug = slugify(&plain);
            out.anchors.push(Anchor {
                level,
                text: plain,
                slug,
                line: out.lines.len(),
            });

            let start = out.lines.len();
            let style = ctx.theme.heading(level as u32);
            let marker = format!("{} ", "#".repeat(level as usize));
            let mut lines = vec![StyledLine::new()];
            lines[0].push_styled(marker, style);
            let mut links = Vec::new();
            inline::render_inlines(node, style, ctx.theme, &mut lines, &mut links);
            prepend_indent(&mut lines, &mut links, indent);
            merge_inline(out, lines, links);
            push_block(out, start, BlockKind::Leaf);
        }

        NodeValue::Paragraph => {
            drop(data);
            let start = out.lines.len();
            let mut lines = Vec::new();
            let mut links = Vec::new();
            inline::render_inlines(node, ctx.theme.paragraph, ctx.theme, &mut lines, &mut links);
            prepend_indent(&mut lines, &mut links, indent);
            merge_inline(out, lines, links);
            push_block(out, start, BlockKind::Leaf);
        }

        NodeValue::BlockQuote => {
            drop(data);
            let mut inner = RenderOutput::default();
            render_blocks(node, ctx, "", true, &mut inner);
            let base_line = out.lines.len();
            out.anchors.extend(inner.anchors.into_iter().map(|mut a| {
                a.line += base_line;
                a
            }));
            // Each line gains a leading "│ " span (and an indent span when
            // `indent` is set), so span indices shift right by that count.
            let span_shift = if indent.is_empty() { 1 } else { 2 };
            shift_links(out, inner.links, base_line, span_shift);
            shift_blocks(out, inner.blocks, base_line);
            for mut line in inner.lines {
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
                out.lines.push(wrapped);
            }
            push_block(out, base_line, BlockKind::Container);
        }

        NodeValue::List(list) => {
            let list_type = list.list_type;
            let start = list.start;
            let tight = list.tight;
            drop(data);
            let list_start_line = out.lines.len();
            let mut counter = start.max(1);
            for (idx, item) in node.children().enumerate() {
                if idx > 0 && !tight {
                    out.lines.push(StyledLine::new());
                }
                // Check whether this child is a GFM task item so we can
                // emit the right marker. comrak places TaskItem nodes
                // directly under List, replacing Item in the AST.
                let task = match &item.data.borrow().value {
                    NodeValue::TaskItem(t) => Some(*t),
                    _ => None,
                };
                if let Some(t) = task {
                    let done = t.symbol.is_some();
                    let marker = if done { "[x] " } else { "[ ] " };
                    let style = if done {
                        ctx.theme.task_marker_done
                    } else {
                        ctx.theme.task_marker_todo
                    };
                    render_task_item(item, ctx, indent, marker, style, tight, out);
                } else {
                    let marker = match list_type {
                        ListType::Bullet => "• ".to_string(),
                        ListType::Ordered => {
                            let s = format!("{counter}. ");
                            counter += 1;
                            s
                        }
                    };
                    render_list_item(item, ctx, indent, &marker, tight, out);
                }
            }
            push_block(out, list_start_line, BlockKind::Container);
        }

        NodeValue::Item(_) => {
            drop(data);
            render_blocks(node, ctx, indent, true, out);
        }

        NodeValue::TaskItem(task) => {
            let done = task.symbol.is_some();
            drop(data);
            let marker = if done { "[x] " } else { "[ ] " };
            let marker_style = if done {
                ctx.theme.task_marker_done
            } else {
                ctx.theme.task_marker_todo
            };
            // Stray TaskItem outside a List — default to tight layout.
            render_task_item(node, ctx, indent, marker, marker_style, true, out);
        }

        NodeValue::CodeBlock(code) => {
            let info = code.info.clone();
            let literal = code.literal.clone();
            drop(data);

            let start = out.lines.len();
            let fence_text = if info.is_empty() {
                "```".to_string()
            } else {
                format!("``` {info}")
            };
            let mut top = StyledLine::new();
            if !indent.is_empty() {
                top.push_plain(indent.to_string());
            }
            top.push_styled(fence_text, ctx.theme.code_fence);
            out.lines.push(top);

            let inner_start = out.lines.len();
            let lang = info.split_whitespace().next().unwrap_or("");
            let highlighted = highlight::highlight_code(&literal, lang);
            for mut line in highlighted {
                if !indent.is_empty() {
                    let mut spans = vec![StyledSpan::plain(indent.to_string())];
                    spans.append(&mut line.spans);
                    line.spans = spans;
                }
                out.lines.push(line);
            }
            // Record the fence-less inner range as a separate Leaf so yank
            // expansion stops at "just the code" before grabbing the
            // surrounding ``` lines.
            if out.lines.len() > inner_start {
                out.blocks.push(BlockRange {
                    start: inner_start,
                    end: out.lines.len() - 1,
                    kind: BlockKind::Leaf,
                });
            }

            let mut bottom = StyledLine::new();
            if !indent.is_empty() {
                bottom.push_plain(indent.to_string());
            }
            bottom.push_styled("```", ctx.theme.code_fence);
            out.lines.push(bottom);
            let end = out.lines.len() - 1;
            if lang.eq_ignore_ascii_case("mermaid") {
                out.mermaid_blocks.push(MermaidBlock {
                    start,
                    end,
                    source: literal.clone(),
                });
            }
            push_block(out, start, BlockKind::Leaf);
        }

        NodeValue::HtmlBlock(html) => {
            let literal = html.literal.clone();
            drop(data);
            let start = out.lines.len();
            for raw_line in literal.lines() {
                let mut line = StyledLine::new();
                if !indent.is_empty() {
                    line.push_plain(indent.to_string());
                }
                line.push_styled(raw_line.to_string(), ctx.theme.code_inline);
                out.lines.push(line);
            }
            push_block(out, start, BlockKind::Leaf);
        }

        NodeValue::ThematicBreak => {
            drop(data);
            let start = out.lines.len();
            let mut line = StyledLine::new();
            if !indent.is_empty() {
                line.push_plain(indent.to_string());
            }
            line.push_styled("─".repeat(40), ctx.theme.thematic_break);
            out.lines.push(line);
            push_block(out, start, BlockKind::Leaf);
        }

        NodeValue::Table(_) => {
            drop(data);
            let start = out.lines.len();
            let mut lines = Vec::new();
            let mut links = Vec::new();
            table::render_table(node, ctx.theme, &mut lines, &mut links);
            prepend_indent(&mut lines, &mut links, indent);
            merge_inline(out, lines, links);
            push_block(out, start, BlockKind::Leaf);
        }

        // TableRow / TableCell are walked by the table renderer; bare
        // occurrences would be malformed, but ignore them gracefully.
        NodeValue::TableRow(_) | NodeValue::TableCell => {}

        // Anything else (footnotes, etc.) — recurse so content isn't dropped.
        _ => {
            drop(data);
            render_blocks(node, ctx, indent, true, out);
        }
    }
}

fn render_list_item<'a>(
    item: &'a AstNode<'a>,
    ctx: &RenderContext<'_>,
    indent: &str,
    marker: &str,
    tight: bool,
    out: &mut RenderOutput,
) {
    let marker_width = display_width(marker);

    let mut inner = RenderOutput::default();
    render_blocks(item, ctx, "", !tight, &mut inner);

    let base_line = out.lines.len();
    out.anchors.extend(inner.anchors.into_iter().map(|mut a| {
        a.line += base_line;
        a
    }));
    // Each line is prefixed with marker/padding (always) and an indent span
    // when `indent` is set. Shift link span ranges accordingly.
    let span_shift = if indent.is_empty() { 1 } else { 2 };
    shift_links(out, inner.links, base_line, span_shift);
    shift_blocks(out, inner.blocks, base_line);

    for (idx, line) in inner.lines.into_iter().enumerate() {
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
        out.lines.push(wrapped);
    }
    push_block(out, base_line, BlockKind::Container);
}

fn render_task_item<'a>(
    item: &'a AstNode<'a>,
    ctx: &RenderContext<'_>,
    indent: &str,
    marker: &str,
    marker_style: Style,
    tight: bool,
    out: &mut RenderOutput,
) {
    let marker_width = display_width(marker);

    let mut inner = RenderOutput::default();
    render_blocks(item, ctx, "", !tight, &mut inner);

    let base_line = out.lines.len();
    out.anchors.extend(inner.anchors.into_iter().map(|mut a| {
        a.line += base_line;
        a
    }));
    let span_shift = if indent.is_empty() { 1 } else { 2 };
    shift_links(out, inner.links, base_line, span_shift);
    shift_blocks(out, inner.blocks, base_line);

    for (idx, line) in inner.lines.into_iter().enumerate() {
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
        out.lines.push(wrapped);
    }
    push_block(out, base_line, BlockKind::Container);
}

fn merge_inline(
    out: &mut RenderOutput,
    lines: Vec<StyledLine>,
    links: Vec<crate::render::link::Link>,
) {
    let base_line = out.lines.len();
    out.lines.extend(lines);
    out.links.extend(links.into_iter().map(|mut l| {
        l.line += base_line;
        l
    }));
}

/// Re-base every link in `links` for a wrapper that prepended `span_shift`
/// spans (e.g. indent + marker) to every line and shifted lines by
/// `base_line` rows.
fn shift_links(
    out: &mut RenderOutput,
    links: Vec<crate::render::link::Link>,
    base_line: usize,
    span_shift: usize,
) {
    out.links.extend(links.into_iter().map(|mut l| {
        l.line += base_line;
        l.span_range.start += span_shift;
        l.span_range.end += span_shift;
        l
    }));
}

fn shift_blocks(out: &mut RenderOutput, blocks: Vec<BlockRange>, base_line: usize) {
    out.blocks.extend(blocks.into_iter().map(|mut b| {
        b.start += base_line;
        b.end += base_line;
        b
    }));
}

/// Record a block spanning `[start, out.lines.len() - 1]`. No-op if the
/// block produced zero lines (a degenerate case).
fn push_block(out: &mut RenderOutput, start: usize, kind: BlockKind) {
    if out.lines.len() > start {
        out.blocks.push(BlockRange {
            start,
            end: out.lines.len() - 1,
            kind,
        });
    }
}

fn prepend_indent(lines: &mut [StyledLine], links: &mut [crate::render::link::Link], indent: &str) {
    if indent.is_empty() {
        return;
    }
    for line in lines.iter_mut() {
        let mut spans = Vec::with_capacity(line.spans.len() + 1);
        spans.push(StyledSpan::plain(indent.to_string()));
        spans.append(&mut line.spans);
        line.spans = spans;
    }
    for link in links.iter_mut() {
        link.span_range.start += 1;
        link.span_range.end += 1;
    }
}

fn trim_trailing_blank(out: &mut Vec<StyledLine>) {
    while matches!(out.last(), Some(l) if l.is_empty()) {
        out.pop();
    }
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}

/// Flatten a heading's inline children into plain text — used for the TOC and
/// anchor slugs. Code spans contribute their literal text; emphasis et al.
/// are unwrapped.
fn collect_plain_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut buf = String::new();
    walk_plain(node, &mut buf);
    buf.trim().to_string()
}

fn walk_plain<'a>(node: &'a AstNode<'a>, out: &mut String) {
    let data = node.data.borrow();
    match &data.value {
        NodeValue::Text(t) => out.push_str(t),
        NodeValue::Code(c) => out.push_str(&c.literal),
        NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
        _ => {
            drop(data);
            for child in node.children() {
                walk_plain(child, out);
            }
        }
    }
}

/// Render a YAML / TOML front matter block as a styled panel. `literal`
/// is the raw text comrak hands us, which includes the opening and
/// closing delimiter lines.
fn render_front_matter(literal: &str, indent: &str, theme: &Theme, out: &mut RenderOutput) {
    let trimmed = literal.trim_end_matches('\n');
    let mut iter = trimmed.lines();
    let first = iter.next().unwrap_or("").trim();
    let format_label = if first == "+++" { "TOML" } else { "YAML" };

    let mut content: Vec<&str> = iter.collect();
    if let Some(last) = content.last()
        && matches!(last.trim(), "---" | "+++")
    {
        content.pop();
    }

    let mut top = StyledLine::new();
    if !indent.is_empty() {
        top.push_plain(indent.to_string());
    }
    top.push_styled(
        format!("┌─ Front matter ({format_label}) "),
        theme.front_matter_border,
    );
    top.push_styled("─".repeat(20), theme.front_matter_border);
    out.lines.push(top);

    for line in &content {
        let mut row = StyledLine::new();
        if !indent.is_empty() {
            row.push_plain(indent.to_string());
        }
        row.push_styled("│ ", theme.front_matter_border);
        if let Some((key, sep, value)) = split_front_matter_kv(line) {
            row.push_styled(key.to_string(), theme.front_matter_key);
            row.push_styled(sep.to_string(), theme.front_matter_separator);
            if !value.is_empty() {
                row.push_styled(value.to_string(), theme.front_matter_value);
            }
        } else {
            row.push_styled((*line).to_string(), theme.front_matter_value);
        }
        out.lines.push(row);
    }

    let mut bot = StyledLine::new();
    if !indent.is_empty() {
        bot.push_plain(indent.to_string());
    }
    bot.push_styled("└".to_string(), theme.front_matter_border);
    bot.push_styled("─".repeat(40), theme.front_matter_border);
    out.lines.push(bot);
}

/// Heuristic key/value splitter for front matter rows. Recognizes TOML
/// (`key = value`) and YAML (`key: value` / bare `key:`) shapes; nested
/// content keeps its leading whitespace as part of the key so indentation
/// survives.
fn split_front_matter_kv(line: &str) -> Option<(&str, &str, &str)> {
    if let Some(idx) = line.find(" = ") {
        return Some((&line[..idx], " = ", &line[idx + 3..]));
    }
    if let Some(idx) = line.find(": ") {
        return Some((&line[..idx], ": ", &line[idx + 2..]));
    }
    if let Some(stripped) = line.strip_suffix(':') {
        return Some((stripped, ":", ""));
    }
    None
}
