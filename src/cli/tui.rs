use std::io;
use std::path::{Path, PathBuf};

use comrak::Arena;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color as RColor, Modifier, Style as RStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::render;
use crate::render::style::{Color, Style, StyledLine, StyledSpan};

pub fn run(file: Option<&Path>) -> io::Result<()> {
    // The TUI consumes stdin for keystrokes, so it can't also accept
    // markdown via the pipe. Demand a file path.
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
    let lines = parse_and_render(&input);
    let title = path.display().to_string();

    ratatui::run(|terminal| {
        let mut app = App::new(lines, title, path);
        app.run(terminal)
    })
}

fn parse_and_render(input: &str) -> Vec<StyledLine> {
    let arena = Arena::new();
    let root = render::parse::parse(&arena, input);
    render::render_document(root)
}

struct App {
    lines: Vec<StyledLine>,
    title: String,
    #[allow(dead_code)]
    path: PathBuf,
    scroll: usize,
    body_height: u16,
}

impl App {
    fn new(lines: Vec<StyledLine>, title: String, path: PathBuf) -> Self {
        Self {
            lines,
            title,
            path,
            scroll: 0,
            body_height: 0,
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
        match (code, mods) {
            (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => return true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => return true,
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

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let body_area = chunks[0];
        let status_area = chunks[1];

        self.body_height = body_area.height;

        let max_scroll = self.max_scroll();
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }

        let lines: Vec<Line<'static>> = self.lines.iter().map(convert_line).collect();
        let body = Paragraph::new(lines)
            .scroll((self.scroll as u16, 0))
            .wrap(Wrap { trim: false });
        frame.render_widget(body, body_area);

        let total = self.lines.len();
        let last_visible = (self.scroll + self.body_height as usize).min(total);
        let status_text = format!(
            " {}  [{}-{}/{}]  q:quit  j/k:scroll  g/G:top/bottom  C-d/C-u:half  C-f/C-b:page ",
            self.title,
            self.scroll + 1,
            last_visible,
            total,
        );
        let status = Paragraph::new(status_text)
            .style(RStyle::default().add_modifier(Modifier::REVERSED));
        frame.render_widget(status, status_area);
    }

    fn scroll_by(&mut self, delta: isize) {
        let max = self.max_scroll() as isize;
        let next = (self.scroll as isize) + delta;
        self.scroll = next.clamp(0, max) as usize;
    }

    fn max_scroll(&self) -> usize {
        self.lines
            .len()
            .saturating_sub(self.body_height as usize)
    }

    fn half_page(&self) -> isize {
        ((self.body_height as isize).max(1)) / 2
    }

    fn page(&self) -> isize {
        (self.body_height as isize).max(1)
    }
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

