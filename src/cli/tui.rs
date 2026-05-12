use std::io;
use std::path::{Path, PathBuf};

use comrak::Arena;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color as RColor, Modifier, Style as RStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::render::style::{Color, Style, StyledLine, StyledSpan};
use crate::render::width::spans_width;
use crate::render::{self, Anchor, RenderOutput};

pub fn run(file: Option<&Path>) -> io::Result<()> {
    let path = match file {
        Some(p) if p.as_os_str() != "-" => p.to_path_buf(),
        Some(_) | None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TUI mode requires a file path. Use `markdown-browser render` for stdin input.",
            ));
        }
    };

    let input = std::fs::read_to_string(&path)?;
    let doc = parse_and_render(&input);
    let title = path.display().to_string();

    ratatui::run(|terminal| {
        let mut app = App::new(doc, title, path);
        app.run(terminal)
    })
}

fn parse_and_render(input: &str) -> RenderOutput {
    let arena = Arena::new();
    let root = render::parse::parse(&arena, input);
    render::render_document(root)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Toc,
}

struct App {
    lines: Vec<StyledLine>,
    anchors: Vec<Anchor>,
    title: String,
    #[allow(dead_code)]
    path: PathBuf,
    /// Scroll offset measured in **screen rows** (after wrap), matching what
    /// `Paragraph::scroll` consumes. Heading jumps and lookups convert
    /// logical line indices through `screen_row_of`.
    scroll: usize,
    body_height: u16,
    body_width: u16,
    mode: Mode,
    toc_selection: usize,
}

impl App {
    fn new(doc: RenderOutput, title: String, path: PathBuf) -> Self {
        Self {
            lines: doc.lines,
            anchors: doc.anchors,
            title,
            path,
            scroll: 0,
            body_height: 0,
            body_width: 0,
            mode: Mode::Normal,
            toc_selection: 0,
        }
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if self.handle_key(key.code, key.modifiers) {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        match self.mode {
            Mode::Normal => self.handle_key_normal(code, mods),
            Mode::Toc => {
                self.handle_key_toc(code, mods);
                false
            }
        }
    }

    fn handle_key_normal(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        match (code, mods) {
            (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => return true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => return true,
            (KeyCode::Char('o'), _) => self.open_toc(),
            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => self.scroll_by(1),
            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => self.scroll_by(-1),
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => self.scroll_by(self.half_page()),
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => self.scroll_by(-self.half_page()),
            (KeyCode::Char('f'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
                self.scroll_by(self.page())
            }
            (KeyCode::Char('b'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
                self.scroll_by(-self.page())
            }
            (KeyCode::Char('g'), _) | (KeyCode::Home, _) => self.scroll = 0,
            (KeyCode::Char('G'), _) | (KeyCode::End, _) => self.scroll = self.max_scroll(),
            _ => {}
        }
        false
    }

    fn handle_key_toc(&mut self, code: KeyCode, _mods: KeyModifiers) {
        if self.anchors.is_empty() {
            self.mode = Mode::Normal;
            return;
        }
        match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('o') => {
                self.mode = Mode::Normal;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.toc_selection + 1 < self.anchors.len() {
                    self.toc_selection += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.toc_selection > 0 {
                    self.toc_selection -= 1;
                }
            }
            KeyCode::Home | KeyCode::Char('g') => self.toc_selection = 0,
            KeyCode::End | KeyCode::Char('G') => self.toc_selection = self.anchors.len() - 1,
            KeyCode::Enter => {
                let line = self.anchors[self.toc_selection].line;
                self.scroll = self.screen_row_of(line).min(self.max_scroll());
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn open_toc(&mut self) {
        if self.anchors.is_empty() {
            return;
        }
        self.mode = Mode::Toc;
        // Highlight the heading we're currently sitting in (compared in
        // screen-row space, the same units `scroll` uses).
        self.toc_selection = self
            .anchors
            .iter()
            .rposition(|a| self.screen_row_of(a.line) <= self.scroll)
            .unwrap_or(0);
    }

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let body_area = chunks[0];
        let status_area = chunks[1];

        self.body_height = body_area.height;
        self.body_width = body_area.width;

        let max_scroll = self.max_scroll();
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }

        let lines: Vec<Line<'static>> = self.lines.iter().map(convert_line).collect();
        let body = Paragraph::new(lines)
            .scroll((self.scroll as u16, 0))
            .wrap(Wrap { trim: false });
        frame.render_widget(body, body_area);

        let total = self.total_screen_rows();
        let last_visible = (self.scroll + self.body_height as usize).min(total);
        let hint = match self.mode {
            Mode::Normal => "q:quit  o:toc  j/k:scroll  g/G:top/bottom  C-d/C-u:half  C-f/C-b:page",
            Mode::Toc => "Enter:jump  Esc/q/o:close  j/k:select",
        };
        let status_text = format!(" {}  [{}-{}/{}]  {hint} ", self.title, self.scroll + 1, last_visible, total);
        let status = Paragraph::new(status_text)
            .style(RStyle::default().add_modifier(Modifier::REVERSED));
        frame.render_widget(status, status_area);

        if self.mode == Mode::Toc && !self.anchors.is_empty() {
            self.draw_toc(frame, body_area);
        }
    }

    fn draw_toc(&self, frame: &mut ratatui::Frame<'_>, body_area: Rect) {
        let area = centered_rect(60, 80, body_area);
        let items: Vec<ListItem> = self
            .anchors
            .iter()
            .map(|a| {
                let indent = "  ".repeat((a.level.saturating_sub(1)) as usize);
                ListItem::new(format!("{indent}{}", a.text))
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Contents "),
            )
            .highlight_style(
                RStyle::default()
                    .fg(RColor::Black)
                    .bg(RColor::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(" › ");
        let mut state = ListState::default();
        state.select(Some(self.toc_selection));
        frame.render_widget(Clear, area);
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn scroll_by(&mut self, delta: isize) {
        let max = self.max_scroll() as isize;
        let next = (self.scroll as isize) + delta;
        self.scroll = next.clamp(0, max) as usize;
    }

    fn max_scroll(&self) -> usize {
        self.total_screen_rows()
            .saturating_sub(self.body_height as usize)
    }

    fn half_page(&self) -> isize {
        ((self.body_height as isize).max(1)) / 2
    }

    fn page(&self) -> isize {
        (self.body_height as isize).max(1)
    }

    /// Total wrapped screen-row count for the whole document.
    fn total_screen_rows(&self) -> usize {
        self.compute_screen_rows(self.lines.len())
    }

    /// Screen row at which logical line `until` begins.
    fn screen_row_of(&self, until: usize) -> usize {
        self.compute_screen_rows(until)
    }

    fn compute_screen_rows(&self, until: usize) -> usize {
        let body_w = (self.body_width as usize).max(1);
        let end = until.min(self.lines.len());
        let mut rows = 0usize;
        for line in &self.lines[..end] {
            rows += wrapped_rows(line, body_w);
        }
        rows
    }
}

fn wrapped_rows(line: &StyledLine, body_w: usize) -> usize {
    let w = spans_width(&line.spans);
    if w == 0 {
        1
    } else {
        w.div_ceil(body_w).max(1)
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

fn convert_line(line: &StyledLine) -> Line<'static> {
    let spans: Vec<Span<'static>> = line.spans.iter().map(convert_span).collect();
    Line::from(spans)
}

fn convert_span(span: &StyledSpan) -> Span<'static> {
    Span::styled(span.text.clone(), to_ratatui_style(span.style))
}

fn to_ratatui_style(style: Style) -> RStyle {
    let mut s = RStyle::default();
    if let Some(c) = style.fg {
        s = s.fg(to_ratatui_color(c));
    }
    if let Some(c) = style.bg {
        s = s.bg(to_ratatui_color(c));
    }
    let mut mods = Modifier::empty();
    if style.bold {
        mods |= Modifier::BOLD;
    }
    if style.italic {
        mods |= Modifier::ITALIC;
    }
    if style.underline {
        mods |= Modifier::UNDERLINED;
    }
    if style.strikethrough {
        mods |= Modifier::CROSSED_OUT;
    }
    if style.dim {
        mods |= Modifier::DIM;
    }
    s.add_modifier(mods)
}

fn to_ratatui_color(c: Color) -> RColor {
    match c {
        Color::Black => RColor::Black,
        Color::Red => RColor::Red,
        Color::Green => RColor::Green,
        Color::Yellow => RColor::Yellow,
        Color::Blue => RColor::Blue,
        Color::Magenta => RColor::Magenta,
        Color::Cyan => RColor::Cyan,
        Color::White => RColor::Gray,
        Color::BrightBlack => RColor::DarkGray,
        Color::BrightRed => RColor::LightRed,
        Color::BrightGreen => RColor::LightGreen,
        Color::BrightYellow => RColor::LightYellow,
        Color::BrightBlue => RColor::LightBlue,
        Color::BrightMagenta => RColor::LightMagenta,
        Color::BrightCyan => RColor::LightCyan,
        Color::BrightWhite => RColor::White,
    }
}
