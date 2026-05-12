//! GFM table rendering. Computes column widths from styled cell content
//! (so ANSI escapes are never counted), then lays out cells inside Unicode
//! box-drawing borders. Alignment honours the table header separator.
//!
//! Wrapping when the table exceeds the terminal width is deferred — that
//! lives in a future ticket and will plug into the same data model.

use comrak::nodes::{AstNode, NodeValue, TableAlignment};

use crate::render::inline;
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
}

pub fn render_table<'a>(table: &'a AstNode<'a>, theme: &Theme, out: &mut Vec<StyledLine>) {
    let aligns = match &table.data.borrow().value {
        NodeValue::Table(t) => t.alignments.iter().copied().map(Align::from).collect::<Vec<_>>(),
        _ => return,
    };

    let mut rows: Vec<Row> = Vec::new();
    for row_node in table.children() {
        let row_data = row_node.data.borrow();
        let NodeValue::TableRow(is_header) = row_data.value else {
            continue;
        };
        let mut cells = Vec::new();
        for cell_node in row_node.children() {
            let cell_data = cell_node.data.borrow();
            if !matches!(cell_data.value, NodeValue::TableCell) {
                continue;
            }
            drop(cell_data);
            cells.push(collect_cell(cell_node, theme));
        }
        drop(row_data);
        rows.push(Row {
            header: is_header,
            cells,
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
        }
    }
    let mut aligns = aligns;
    while aligns.len() < col_count {
        aligns.push(Align::Left);
    }

    let widths = compute_widths(&rows, col_count);

    let border_style = theme.thematic_break;
    let header_text_style = theme.strong;

    // Top border
    out.push(border_line(&widths, BorderKind::Top, border_style));

    let mut first_body = true;
    for row in &rows {
        let styled_cells: Vec<Vec<StyledSpan>> = row
            .cells
            .iter()
            .enumerate()
            .map(|(i, cell)| pad_cell(cell, widths[i], aligns[i], row.header.then_some(header_text_style)))
            .collect();
        out.push(content_line(&styled_cells, border_style));

        if row.header {
            out.push(border_line(&widths, BorderKind::HeaderSep, border_style));
        } else if first_body {
            first_body = false;
        }
    }

    // Bottom border
    out.push(border_line(&widths, BorderKind::Bottom, border_style));
}

fn collect_cell<'a>(cell: &'a AstNode<'a>, theme: &Theme) -> Vec<StyledSpan> {
    let mut tmp: Vec<StyledLine> = vec![StyledLine::new()];
    // Links inside table cells aren't navigable in the MVP — drop them.
    let mut discarded_links = Vec::new();
    inline::render_inlines(cell, Style::new(), theme, &mut tmp, &mut discarded_links);
    // Flatten: tables are single-line per cell for now. Multi-line cells
    // need a wrapping pass we don't have yet.
    let mut out = Vec::new();
    let mut first = true;
    for line in tmp {
        if !first {
            out.push(StyledSpan::plain(" "));
        }
        first = false;
        out.extend(line.spans);
    }
    out
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
    // Minimum sensible width so a separator like `---` still looks like a column.
    for w in widths.iter_mut() {
        if *w < 1 {
            *w = 1;
        }
    }
    widths
}

fn pad_cell(
    cell: &[StyledSpan],
    target_width: usize,
    align: Align,
    extra_style: Option<Style>,
) -> Vec<StyledSpan> {
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
    if left > 0 {
        out.push(StyledSpan::plain(" ".repeat(left)));
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
    out
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
            // (left, junction, right, fill)
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

fn content_line(cells: &[Vec<StyledSpan>], border_style: Style) -> StyledLine {
    let mut line = StyledLine::new();
    line.push_styled("│", border_style);
    for (i, cell) in cells.iter().enumerate() {
        line.push_plain(" ");
        for span in cell {
            line.push_span(span.clone());
        }
        line.push_plain(" ");
        line.push_styled("│", border_style);
        let _ = i;
    }
    line
}
