/// A styled span of text. The output device decides how to realize the style
/// (ANSI escape, ratatui `Span`, plain text, etc.).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

impl StyledSpan {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self::new(text, Style::default())
    }
}

/// A line of styled spans. Lines are emitted by the renderer in display order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
}

impl StyledLine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_span(&mut self, span: StyledSpan) {
        if span.text.is_empty() {
            return;
        }
        self.spans.push(span);
    }

    pub fn push_plain(&mut self, text: impl Into<String>) {
        self.push_span(StyledSpan::plain(text));
    }

    pub fn push_styled(&mut self, text: impl Into<String>, style: Style) {
        self.push_span(StyledSpan::new(text, style));
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
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
    pub reversed: bool,
}

impl Style {
    pub const fn new() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            dim: false,
            reversed: false,
        }
    }

    pub const fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub const fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    pub const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub const fn reversed(mut self) -> Self {
        self.reversed = true;
        self
    }

    /// Merge `other` on top of `self`. Foreground/background present in
    /// `other` win; boolean attributes OR together.
    pub fn merge(mut self, other: Style) -> Self {
        if other.fg.is_some() {
            self.fg = other.fg;
        }
        if other.bg.is_some() {
            self.bg = other.bg;
        }
        self.bold |= other.bold;
        self.italic |= other.italic;
        self.underline |= other.underline;
        self.strikethrough |= other.strikethrough;
        self.dim |= other.dim;
        self.reversed |= other.reversed;
        self
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
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
