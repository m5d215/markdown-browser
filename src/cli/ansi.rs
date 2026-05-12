//! Encode `StyledLine`s as ANSI escape sequences for terminal output.

use std::io::{self, Write};

use crate::render::style::{Color, Style, StyledLine};

/// Write rendered lines to `out`. When `colored` is false, styling is
/// stripped (suitable for non-TTY destinations).
pub fn write_lines<W: Write>(out: &mut W, lines: &[StyledLine], colored: bool) -> io::Result<()> {
    for line in lines {
        for span in &line.spans {
            if colored && !span.style.is_default() {
                write!(out, "{}", style_prefix(span.style))?;
                out.write_all(span.text.as_bytes())?;
                write!(out, "{}", RESET)?;
            } else {
                out.write_all(span.text.as_bytes())?;
            }
        }
        out.write_all(b"\n")?;
    }
    Ok(())
}

const RESET: &str = "\x1b[0m";

fn style_prefix(style: Style) -> String {
    let mut parts: Vec<String> = Vec::new();
    if style.bold {
        parts.push("1".into());
    }
    if style.dim {
        parts.push("2".into());
    }
    if style.italic {
        parts.push("3".into());
    }
    if style.underline {
        parts.push("4".into());
    }
    if style.reversed {
        parts.push("7".into());
    }
    if style.strikethrough {
        parts.push("9".into());
    }
    if let Some(fg) = style.fg {
        parts.push(fg_code(fg));
    }
    if let Some(bg) = style.bg {
        parts.push(bg_code(bg));
    }

    let mut buf = String::with_capacity(parts.iter().map(|p| p.len() + 1).sum::<usize>() + 3);
    buf.push_str("\x1b[");
    buf.push_str(&parts.join(";"));
    buf.push('m');
    buf
}

fn fg_code(color: Color) -> String {
    match color {
        Color::Black => "30".into(),
        Color::Red => "31".into(),
        Color::Green => "32".into(),
        Color::Yellow => "33".into(),
        Color::Blue => "34".into(),
        Color::Magenta => "35".into(),
        Color::Cyan => "36".into(),
        Color::White => "37".into(),
        Color::BrightBlack => "90".into(),
        Color::BrightRed => "91".into(),
        Color::BrightGreen => "92".into(),
        Color::BrightYellow => "93".into(),
        Color::BrightBlue => "94".into(),
        Color::BrightMagenta => "95".into(),
        Color::BrightCyan => "96".into(),
        Color::BrightWhite => "97".into(),
        Color::Rgb(r, g, b) => format!("38;2;{r};{g};{b}"),
    }
}

fn bg_code(color: Color) -> String {
    match color {
        Color::Black => "40".into(),
        Color::Red => "41".into(),
        Color::Green => "42".into(),
        Color::Yellow => "43".into(),
        Color::Blue => "44".into(),
        Color::Magenta => "45".into(),
        Color::Cyan => "46".into(),
        Color::White => "47".into(),
        Color::BrightBlack => "100".into(),
        Color::BrightRed => "101".into(),
        Color::BrightGreen => "102".into(),
        Color::BrightYellow => "103".into(),
        Color::BrightBlue => "104".into(),
        Color::BrightMagenta => "105".into(),
        Color::BrightCyan => "106".into(),
        Color::BrightWhite => "107".into(),
        Color::Rgb(r, g, b) => format!("48;2;{r};{g};{b}"),
    }
}
