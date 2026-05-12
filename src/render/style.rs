/// A styled span of text. The output device decides how to realize the style
/// (ANSI escape, ratatui `Span`, plain text, etc.).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

/// A line of styled spans. Lines are emitted by the renderer in display order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub dim: bool,
}

/// Logical color. Concrete realization (256-color, true color, ratatui) is
/// the sink's responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}
