//! Syntax highlighting for fenced code blocks.
//!
//! Backed by `syntect`. The bundled syntaxes and themes ship with the crate;
//! we ignore each theme's background colour so it doesn't fight the
//! terminal's own background. Foreground colour + font style only.

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SyntectStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::render::style::{Color, Style, StyledLine, StyledSpan};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    THEME.get_or_init(|| {
        let mut ts = ThemeSet::load_defaults();
        // Picked for dark backgrounds. Falls back to any available theme
        // if this name ever disappears upstream.
        ts.themes
            .remove("base16-ocean.dark")
            .or_else(|| ts.themes.remove("Solarized (dark)"))
            .or_else(|| ts.themes.values().next().cloned())
            .unwrap_or_default()
    })
}

/// Highlight a code block body. `lang` is the fence info string's first
/// token (e.g. "rust", "py"). Returns one StyledLine per source line.
pub fn highlight_code(body: &str, lang: &str) -> Vec<StyledLine> {
    let ss = syntax_set();
    let theme = theme();

    let syntax = if lang.is_empty() {
        ss.find_syntax_plain_text()
    } else {
        ss.find_syntax_by_token(lang)
            .or_else(|| ss.find_syntax_by_name(lang))
            .unwrap_or_else(|| ss.find_syntax_plain_text())
    };

    let mut hl = HighlightLines::new(syntax, theme);
    let mut out = Vec::new();
    for raw_line in LinesWithEndings::from(body) {
        let ranges = hl.highlight_line(raw_line, ss).unwrap_or_default();
        let mut styled = StyledLine::new();
        for (s_style, text) in ranges {
            let text = text.trim_end_matches('\n');
            if text.is_empty() {
                continue;
            }
            styled.push_span(StyledSpan::new(text, convert_style(s_style)));
        }
        out.push(styled);
    }
    out
}

fn convert_style(s: SyntectStyle) -> Style {
    let mut style = Style::new().fg(Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b));
    if s.font_style.contains(FontStyle::BOLD) {
        style.bold = true;
    }
    if s.font_style.contains(FontStyle::ITALIC) {
        style.italic = true;
    }
    if s.font_style.contains(FontStyle::UNDERLINE) {
        style.underline = true;
    }
    style
}
