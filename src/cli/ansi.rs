//! Encode `StyledLine`s as ANSI escape sequences for terminal output.

use std::io::{self, Write};

use crate::render::style::{Color, Style, StyledLine};

/// Write rendered lines to `out`. When `colored` is false, styling is
/// stripped (suitable for non-TTY destinations).
pub fn write_lines<W: Write>(
    out: &mut W,
    lines: &[StyledLine],
    colored: bool,
) -> io::Result<()> {
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
    let mut codes: Vec<&'static str> = Vec::new();
    if style.bold {
        codes.push("1");
    }
    if style.dim {
        codes.push("2");
    }
    if style.italic {
        codes.push("3");
    }
    if style.underline {
        codes.push("4");
    }
    if style.strikethrough {
        codes.push("9");
    }

    let mut buf = String::new();
    buf.push_str("\x1b[");
    let mut first = true;
    for code in codes {
        if !first {
            buf.push(';');
        }
        first = false;
        buf.push_str(code);
    }
    if let Some(fg) = style.fg {
        if !first {
            buf.push(';');
        }
        first = false;
        buf.push_str(fg_code(fg));
    }
    if let Some(bg) = style.bg {
        if !first {
            buf.push(';');
        }
        first = false;
        buf.push_str(bg_code(bg));
    }
    let _ = first;
    buf.push('m');
    buf
}

fn fg_code(color: Color) -> &'static str {
    match color {
        Color::Black => "30",
        Color::Red => "31",
        Color::Green => "32",
        Color::Yellow => "33",
        Color::Blue => "34",
        Color::Magenta => "35",
        Color::Cyan => "36",
        Color::White => "37",
        Color::BrightBlack => "90",
        Color::BrightRed => "91",
        Color::BrightGreen => "92",
        Color::BrightYellow => "93",
        Color::BrightBlue => "94",
        Color::BrightMagenta => "95",
        Color::BrightCyan => "96",
        Color::BrightWhite => "97",
    }
}

fn bg_code(color: Color) -> &'static str {
    match color {
        Color::Black => "40",
        Color::Red => "41",
        Color::Green => "42",
        Color::Yellow => "43",
        Color::Blue => "44",
        Color::Magenta => "45",
        Color::Cyan => "46",
        Color::White => "47",
        Color::BrightBlack => "100",
        Color::BrightRed => "101",
        Color::BrightGreen => "102",
        Color::BrightYellow => "103",
        Color::BrightBlue => "104",
        Color::BrightMagenta => "105",
        Color::BrightCyan => "106",
        Color::BrightWhite => "107",
    }
}
