//! GFM table rendering. Computes column widths from styled cell content
//! (so ANSI escapes are never counted), then lays out cells inside Unicode
//! box-drawing borders. Alignment honours the table header separator.
//!
//! Wrapping when the table exceeds the terminal width is deferred — that
//! lives in a future ticket and will plug into the same data model.

use std::ops::Range;

use comrak::nodes::{AstNode, NodeValue, TableAlignment};

use crate::render::inline;
use crate::render::link::Link;
use crate::render::style::{Style, StyledLine, StyledSpan};
use crate::render::theme::Theme;
use crate::render::width::spans_width;

#[derive(Debug, Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

impl From<TableAlignment> for Align {
    fn from(a: TableAlignment) -> Self {
        match a {
            TableAlignment::Left | TableAlignment::None => Align::Left,
            TableAlignment::Center => Align::Center,
            TableAlignment::Right => Align::Right,
        }
    }
}

struct Row {
    header: bool,
    cells: Vec<Vec<StyledSpan>>,
    cell_links: Vec<Vec<Link>>,
}

pub fn render_table<'a>(
    table: &'a AstNode<'a>,
    theme: &Theme,
    out_lines: &mut Vec<StyledLine>,
    out_links: &mut Vec<Link>,
) {
    let aligns = match &table.data.borrow().value {
        NodeValue::Table(t) => t
            .alignments
            .iter()
            .copied()
            .map(Align::from)
            .collect::<Vec<_>>(),
        _ => return,
    };

    let mut rows: Vec<Row> = Vec::new();
    for row_node in table.children() {
        let row_data = row_node.data.borrow();
        let NodeValue::TableRow(is_header) = row_data.value else {
            continue;
        };
        let mut cells = Vec::new();
        let mut cell_links = Vec::new();
        for cell_node in row_node.children() {
            let cell_data = cell_node.data.borrow();
            if !matches!(cell_data.value, NodeValue::TableCell) {
                continue;
            }
            drop(cell_data);
            let (spans, links) = collect_cell(cell_node, theme);
            cells.push(spans);
            cell_links.push(links);
        }
        drop(row_data);
        rows.push(Row {
            header: is_header,
            cells,
            cell_links,
        });
    }

    if rows.is_empty() {
        return;
    }

    let col_count = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    if col_count == 0 {
        return;
    }

    // Pad short rows; widen alignment vec if the parser gave fewer.
    for row in rows.iter_mut() {
        while row.cells.len() < col_count {
            row.cells.push(Vec::new());
            row.cell_links.push(Vec::new());
        }
    }
    let mut aligns = aligns;
    while aligns.len() < col_count {
        aligns.push(Align::Left);
    }

    let widths = compute_widths(&rows, col_count);

    let border_style = theme.thematic_break;
    let header_text_style = theme.strong;

    out_lines.push(border_line(&widths, BorderKind::Top, border_style));

    for row in &rows {
        let mut padded_cells: Vec<Vec<StyledSpan>> = Vec::with_capacity(col_count);
        let mut padded_links: Vec<Vec<Link>> = Vec::with_capacity(col_count);
        for (i, cell) in row.cells.iter().enumerate() {
            let extra = if row.header {
                Some(header_text_style)
            } else {
                None
            };
            let (spans, links) = pad_cell(cell, &row.cell_links[i], widths[i], aligns[i], extra);
            padded_cells.push(spans);
            padded_links.push(links);
        }
        let (line, row_links) = content_line(&padded_cells, &padded_links, border_style);
        let row_line_idx = out_lines.len();
        out_lines.push(line);
        for mut link in row_links {
            link.line = row_line_idx;
            out_links.push(link);
        }

        if row.header {
            out_lines.push(border_line(&widths, BorderKind::HeaderSep, border_style));
        }
    }

    out_lines.push(border_line(&widths, BorderKind::Bottom, border_style));
}

fn collect_cell<'a>(cell: &'a AstNode<'a>, theme: &Theme) -> (Vec<StyledSpan>, Vec<Link>) {
    let mut tmp: Vec<StyledLine> = vec![StyledLine::new()];
    let mut tmp_links = Vec::new();
    inline::render_inlines(cell, Style::new(), theme, &mut tmp, &mut tmp_links);
    // Flatten multi-line cells onto a single line — we don't wrap inside
    // a cell yet. Links that stayed on the first line keep their indices;
    // any link that crossed a soft-break is dropped since its span_range
    // would no longer match the flattened layout.
    let mut spans = Vec::new();
    let mut first = true;
    for line in tmp {
        if !first {
            spans.push(StyledSpan::plain(" "));
        }
        first = false;
        spans.extend(line.spans);
    }
    let links = tmp_links.into_iter().filter(|l| l.line == 0).collect();
    (spans, links)
}

fn compute_widths(rows: &[Row], col_count: usize) -> Vec<usize> {
    let mut widths = vec![0usize; col_count];
    for row in rows {
        for (i, cell) in row.cells.iter().enumerate() {
            let w = spans_width(cell);
            if w > widths[i] {
                widths[i] = w;
            }
        }
    }
    for w in widths.iter_mut() {
        if *w < 1 {
            *w = 1;
        }
    }
    widths
}

fn pad_cell(
    cell: &[StyledSpan],
    cell_links: &[Link],
    target_width: usize,
    align: Align,
    extra_style: Option<Style>,
) -> (Vec<StyledSpan>, Vec<Link>) {
    let w = spans_width(cell);
    let pad = target_width.saturating_sub(w);
    let (left, right) = match align {
        Align::Left => (0, pad),
        Align::Right => (pad, 0),
        Align::Center => {
            let l = pad / 2;
            (l, pad - l)
        }
    };
    let mut out = Vec::with_capacity(cell.len() + 2);
    let mut left_added = 0usize;
    if left > 0 {
        out.push(StyledSpan::plain(" ".repeat(left)));
        left_added = 1;
    }
    if let Some(extra) = extra_style {
        out.extend(cell.iter().map(|s| StyledSpan {
            text: s.text.clone(),
            style: s.style.merge(extra),
        }));
    } else {
        out.extend(cell.iter().cloned());
    }
    if right > 0 {
        out.push(StyledSpan::plain(" ".repeat(right)));
    }
    let shifted: Vec<Link> = cell_links
        .iter()
        .map(|l| Link {
            line: l.line,
            span_range: Range {
                start: l.span_range.start + left_added,
                end: l.span_range.end + left_added,
            },
            url: l.url.clone(),
        })
        .collect();
    (out, shifted)
}

#[derive(Debug, Clone, Copy)]
enum BorderKind {
    Top,
    HeaderSep,
    Bottom,
}

impl BorderKind {
    fn glyphs(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            BorderKind::Top => ("┌", "┬", "┐", "─"),
            BorderKind::HeaderSep => ("├", "┼", "┤", "─"),
            BorderKind::Bottom => ("└", "┴", "┘", "─"),
        }
    }
}

fn border_line(widths: &[usize], kind: BorderKind, style: Style) -> StyledLine {
    let (l, j, r, f) = kind.glyphs();
    let mut buf = String::new();
    buf.push_str(l);
    for (i, w) in widths.iter().enumerate() {
        for _ in 0..(*w + 2) {
            buf.push_str(f);
        }
        if i + 1 < widths.len() {
            buf.push_str(j);
        }
    }
    buf.push_str(r);
    let mut line = StyledLine::new();
    line.push_styled(buf, style);
    line
}

fn content_line(
    cells: &[Vec<StyledSpan>],
    cells_links: &[Vec<Link>],
    border_style: Style,
) -> (StyledLine, Vec<Link>) {
    let mut line = StyledLine::new();
    let mut links = Vec::new();
    line.push_styled("│", border_style);
    for (i, cell) in cells.iter().enumerate() {
        line.push_plain(" ");
        let cell_span_start = line.spans.len();
        for span in cell {
            line.push_span(span.clone());
        }
        for link in &cells_links[i] {
            links.push(Link {
                line: 0,
                span_range: Range {
                    start: link.span_range.start + cell_span_start,
                    end: link.span_range.end + cell_span_start,
                },
                url: link.url.clone(),
            });
        }
        line.push_plain(" ");
        line.push_styled("│", border_style);
    }
    (line, links)
}
