use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use comrak::Arena;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::DefaultTerminal;
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color as RColor, Modifier, Style as RStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};

use crate::render::style::{Color, Style, StyledLine, StyledSpan};
use crate::render::width::spans_width;
use crate::render::{self, Anchor, Link, RenderOutput};

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

    // Start a filesystem watcher that pings the main loop whenever the
    // currently-active file changes. Failure to create one isn't fatal —
    // auto-reload just becomes unavailable.
    let (tx, rx) = mpsc::channel::<()>();
    let watcher_tx = tx;
    let watcher: Option<RecommendedWatcher> =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res
                && matches!(
                    ev.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                )
            {
                let _ = watcher_tx.send(());
            }
        })
        .ok();

    ratatui::run(move |terminal| {
        let mut app = App::new(doc, title, path);
        if let Some(w) = watcher {
            app.attach_watcher(w, rx);
        }
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
    Help,
    Search,
}

#[derive(Debug, Clone)]
struct MatchPos {
    /// Logical line index in `App.lines`.
    line: usize,
    /// Character offset within the line's joined plain text, half-open.
    char_start: usize,
    char_end: usize,
}

/// Hard-coded keybinding catalogue used by the help overlay. Lives in code
/// for now; the customizable-keybindings work in Phase 2 will replace this
/// with a data-driven binding table.
const HELP_ROWS: &[(&str, &str)] = &[
    ("q / Esc / Ctrl-C", "終了"),
    ("?", "このヘルプを開閉"),
    ("o", "目次オーバーレイ"),
    ("/", "検索を開始 (Enter で確定 / Esc で取り消し)"),
    ("n / N", "次 / 前のマッチへ"),
    ("Tab / Shift-Tab", "リンクをフォーカス移動"),
    ("Enter", "フォーカス中のリンクを開く"),
    ("[ / ]", "履歴 Back / Forward"),
    ("Backspace", "履歴 Back (別名)"),
    ("Alt-← / Alt-→", "履歴 Back / Forward (端末対応依存)"),
    ("j / ↓", "1 行スクロール下"),
    ("k / ↑", "1 行スクロール上"),
    ("Ctrl-d / Ctrl-u", "半画面スクロール"),
    ("Ctrl-f / PgDn", "1 画面スクロール下"),
    ("Ctrl-b / PgUp", "1 画面スクロール上"),
    ("g / Home", "先頭へジャンプ"),
    ("G / End", "末尾へジャンプ"),
];

#[derive(Debug, Clone)]
struct Location {
    path: PathBuf,
    /// Saved screen-row scroll offset (in the renderer's wrapped row space).
    scroll: usize,
}

struct App {
    lines: Vec<StyledLine>,
    anchors: Vec<Anchor>,
    links: Vec<Link>,
    title: String,
    path: PathBuf,
    /// Scroll offset measured in **screen rows** (after wrap), matching what
    /// `Paragraph::scroll` consumes. Heading jumps and lookups convert
    /// logical line indices through `screen_row_of`.
    scroll: usize,
    body_height: u16,
    body_width: u16,
    mode: Mode,
    toc_selection: usize,
    focused_link: Option<usize>,
    /// Transient one-shot message rendered into the status bar (cleared on
    /// the next user input).
    status_message: Option<String>,
    /// Browser-style location stack. `history_cursor` points at the
    /// currently displayed entry; anything past it is forward history.
    history: Vec<Location>,
    history_cursor: usize,
    /// Active search query (empty == none).
    search_query: String,
    /// Live edit buffer while the user is typing into the search prompt.
    /// `None` when no search input is in progress.
    search_input: Option<String>,
    /// All hits for `search_query`, ordered by document position.
    search_matches: Vec<MatchPos>,
    /// Index into `search_matches` for the "current" hit, if any.
    search_cursor: Option<usize>,
    /// Filesystem watcher kept alive for the lifetime of the App so its
    /// callback continues to push events into `file_events`.
    watcher: Option<RecommendedWatcher>,
    /// Channel receiver fed by `watcher`.
    file_events: Option<Receiver<()>>,
    /// Path currently registered with the watcher.
    watched_path: Option<PathBuf>,
}

impl App {
    fn new(doc: RenderOutput, title: String, path: PathBuf) -> Self {
        let initial = Location {
            path: path.clone(),
            scroll: 0,
        };
        Self {
            lines: doc.lines,
            anchors: doc.anchors,
            links: doc.links,
            title,
            path,
            scroll: 0,
            body_height: 0,
            body_width: 0,
            mode: Mode::Normal,
            toc_selection: 0,
            focused_link: None,
            status_message: None,
            history: vec![initial],
            history_cursor: 0,
            search_query: String::new(),
            search_input: None,
            search_matches: Vec::new(),
            search_cursor: None,
            watcher: None,
            file_events: None,
            watched_path: None,
        }
    }

    fn attach_watcher(&mut self, watcher: RecommendedWatcher, rx: Receiver<()>) {
        self.watcher = Some(watcher);
        self.file_events = Some(rx);
        self.refresh_watch();
    }

    /// Re-target the filesystem watcher at `self.path`, replacing any prior
    /// watch. No-op when no watcher is attached.
    fn refresh_watch(&mut self) {
        let Some(w) = self.watcher.as_mut() else {
            return;
        };
        if let Some(old) = self.watched_path.take() {
            let _ = w.unwatch(&old);
        }
        match w.watch(&self.path, RecursiveMode::NonRecursive) {
            Ok(()) => {
                self.watched_path = Some(self.path.clone());
            }
            Err(e) => {
                self.status_message = Some(format!("watch {}: {e}", self.path.display()));
            }
        }
    }

    /// Re-read the currently-displayed file and refresh the buffer, keeping
    /// scroll position and re-running any active search. Invoked from the
    /// event loop when the filesystem watcher pings.
    fn reload_in_place(&mut self) {
        let path = self.path.clone();
        let saved_scroll = self.scroll;
        match std::fs::read_to_string(&path) {
            Ok(input) => {
                let doc = parse_and_render(&input);
                self.lines = doc.lines;
                self.anchors = doc.anchors;
                self.links = doc.links;
                self.focused_link = None;
                self.search_matches.clear();
                self.search_cursor = None;
                if !self.search_query.is_empty() {
                    let q = self.search_query.clone();
                    self.commit_search(q);
                }
                let max = self.max_scroll();
                self.scroll = saved_scroll.min(max);
                self.status_message = Some("reloaded".into());
            }
            Err(e) => {
                self.status_message = Some(format!("reload {}: {e}", path.display()));
            }
        }
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            // Poll for input with a short timeout so we periodically wake
            // up to drain the filesystem watcher's channel. Without the
            // poll we'd block in event::read() and never see file events.
            if event::poll(Duration::from_millis(150))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
                && self.handle_key(key.code, key.modifiers)
            {
                return Ok(());
            }

            if self.drain_file_events() {
                self.reload_in_place();
            }
        }
    }

    /// Pull all pending file-change pings off the channel. Returns true if
    /// at least one event arrived; multiple events from one editor save
    /// (atomic writes often fire several) are collapsed into a single
    /// reload.
    fn drain_file_events(&mut self) -> bool {
        let Some(rx) = self.file_events.as_ref() else {
            return false;
        };
        let mut got = false;
        while rx.try_recv().is_ok() {
            got = true;
        }
        got
    }

    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        match self.mode {
            Mode::Normal => self.handle_key_normal(code, mods),
            Mode::Toc => {
                self.handle_key_toc(code, mods);
                false
            }
            Mode::Help => {
                self.handle_key_help(code, mods);
                false
            }
            Mode::Search => {
                self.handle_key_search(code, mods);
                false
            }
        }
    }

    fn handle_key_search(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
            KeyCode::Esc => {
                self.search_input = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let q = self.search_input.take().unwrap_or_default();
                self.mode = Mode::Normal;
                self.commit_search(q);
            }
            KeyCode::Backspace => {
                if let Some(s) = self.search_input.as_mut() {
                    s.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(s) = self.search_input.as_mut() {
                    s.push(c);
                }
            }
            _ => {}
        }
    }

    fn handle_key_help(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn handle_key_normal(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        // Any keystroke clears a stale status message.
        self.status_message = None;
        match (code, mods) {
            (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => return true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => return true,
            (KeyCode::Char('o'), _) => self.open_toc(),
            (KeyCode::Char('?'), _) => self.mode = Mode::Help,
            (KeyCode::Char('/'), _) => {
                self.mode = Mode::Search;
                self.search_input = Some(String::new());
            }
            (KeyCode::Char('n'), _) => self.search_step(1),
            (KeyCode::Char('N'), _) => self.search_step(-1),
            (KeyCode::Tab, _) => self.focus_next_link(1),
            (KeyCode::BackTab, _) => self.focus_next_link(-1),
            (KeyCode::Enter, _) => self.activate_focused_link(),
            (KeyCode::Backspace, _)
            | (KeyCode::Char('['), KeyModifiers::NONE)
            | (KeyCode::Left, KeyModifiers::ALT) => self.go_back(),
            (KeyCode::Char(']'), KeyModifiers::NONE) | (KeyCode::Right, KeyModifiers::ALT) => {
                self.go_forward()
            }
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
                let row = self.screen_row_of(line);
                self.jump_and_push(row);
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn commit_search(&mut self, query: String) {
        self.search_query = query.clone();
        self.search_matches.clear();
        self.search_cursor = None;
        if query.is_empty() {
            return;
        }
        let needle = query.to_lowercase();
        for (line_idx, line) in self.lines.iter().enumerate() {
            let plain: String = line.spans.iter().map(|s| s.text.as_str()).collect();
            let plain_lower = plain.to_lowercase();
            let mut from = 0;
            while let Some(pos) = plain_lower[from..].find(&needle) {
                let byte_start = from + pos;
                let byte_end = byte_start + needle.len();
                let char_start = plain_lower[..byte_start].chars().count();
                let char_end = plain_lower[..byte_end].chars().count();
                self.search_matches.push(MatchPos {
                    line: line_idx,
                    char_start,
                    char_end,
                });
                from = byte_end;
                if from >= plain_lower.len() {
                    break;
                }
            }
        }
        if self.search_matches.is_empty() {
            self.status_message = Some(format!("/{query}: no match"));
            return;
        }
        // Pick the first match at or after the current scroll, falling back
        // to the very first hit.
        let start = self
            .search_matches
            .iter()
            .position(|m| self.screen_row_of(m.line) >= self.scroll)
            .unwrap_or(0);
        self.search_cursor = Some(start);
        self.scroll_to_match(start);
    }

    fn search_step(&mut self, dir: isize) {
        if self.search_matches.is_empty() {
            if !self.search_query.is_empty() {
                self.status_message = Some(format!("/{}: no match", self.search_query));
            }
            return;
        }
        let n = self.search_matches.len() as isize;
        let cur = self.search_cursor.unwrap_or(0) as isize + dir;
        let next = ((cur % n + n) % n) as usize;
        self.search_cursor = Some(next);
        self.scroll_to_match(next);
    }

    fn scroll_to_match(&mut self, idx: usize) {
        let m = &self.search_matches[idx];
        let row = self.screen_row_of(m.line);
        let height = self.body_height as usize;
        if row < self.scroll {
            self.scroll = row;
        } else if height > 0 && row >= self.scroll + height {
            self.scroll = row.saturating_sub(height / 2);
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn focus_next_link(&mut self, dir: isize) {
        if self.links.is_empty() {
            return;
        }
        let next = match self.focused_link {
            Some(i) => {
                let n = self.links.len() as isize;
                let i = i as isize + dir;
                ((i % n + n) % n) as usize
            }
            None => {
                if dir > 0 {
                    self.first_link_at_or_after(self.scroll).unwrap_or(0)
                } else {
                    self.last_link_before(self.scroll + self.body_height as usize)
                        .unwrap_or(self.links.len() - 1)
                }
            }
        };
        self.focused_link = Some(next);
        self.scroll_to_focused();
    }

    fn first_link_at_or_after(&self, scroll: usize) -> Option<usize> {
        self.links
            .iter()
            .position(|l| self.screen_row_of(l.line) >= scroll)
    }

    fn last_link_before(&self, scroll_end: usize) -> Option<usize> {
        self.links
            .iter()
            .rposition(|l| self.screen_row_of(l.line) < scroll_end)
    }

    fn scroll_to_focused(&mut self) {
        let Some(i) = self.focused_link else { return };
        let row = self.screen_row_of(self.links[i].line);
        let height = self.body_height as usize;
        if row < self.scroll {
            self.scroll = row;
        } else if height > 0 && row >= self.scroll + height {
            self.scroll = row.saturating_sub(height / 2);
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn activate_focused_link(&mut self) {
        let Some(i) = self.focused_link else {
            self.status_message = Some("no link focused — press Tab to select one".into());
            return;
        };
        let url = self.links[i].url.clone();
        let parent = self
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let target = classify_link(&url, &parent);
        self.execute_link(target);
    }

    fn execute_link(&mut self, target: LinkTarget) {
        match target {
            LinkTarget::Url(u) => match open::that_detached(&u) {
                Ok(()) => self.status_message = Some(format!("opened: {u}")),
                Err(e) => self.status_message = Some(format!("open failed: {e}")),
            },
            LinkTarget::SameAnchor(slug) => {
                self.navigate_within(&slug);
            }
            LinkTarget::LocalFile(path) => {
                self.navigate(&path, None);
            }
            LinkTarget::LocalAnchor(path, slug) => {
                self.navigate(&path, Some(&slug));
            }
        }
    }

    /// Push a same-document scroll position onto the history stack.
    fn jump_and_push(&mut self, target_row: usize) {
        if let Some(loc) = self.history.get_mut(self.history_cursor) {
            loc.scroll = self.scroll;
        }
        self.scroll = target_row.min(self.max_scroll());
        self.history.truncate(self.history_cursor + 1);
        self.history.push(Location {
            path: self.path.clone(),
            scroll: self.scroll,
        });
        self.history_cursor = self.history.len() - 1;
    }

    fn navigate_within(&mut self, slug: &str) {
        if let Some(a) = self.anchors.iter().find(|a| a.slug == slug) {
            let row = self.screen_row_of(a.line);
            self.jump_and_push(row);
        } else {
            self.status_message = Some(format!("anchor #{slug} not found"));
        }
    }

    fn jump_to_anchor(&mut self, slug: &str) {
        if let Some(a) = self.anchors.iter().find(|a| a.slug == slug) {
            self.scroll = self.screen_row_of(a.line).min(self.max_scroll());
        } else {
            self.status_message = Some(format!("anchor #{slug} not found"));
        }
    }

    /// Open a different document and record it as a new entry on the
    /// history stack. Forward history past the cursor is discarded.
    fn navigate(&mut self, path: &Path, anchor: Option<&str>) {
        // Save where we were leaving from.
        if let Some(loc) = self.history.get_mut(self.history_cursor) {
            loc.scroll = self.scroll;
        }

        if !self.load_document(path, anchor) {
            return;
        }

        self.history.truncate(self.history_cursor + 1);
        self.history.push(Location {
            path: path.to_path_buf(),
            scroll: self.scroll,
        });
        self.history_cursor = self.history.len() - 1;
    }

    fn go_back(&mut self) {
        if self.history_cursor == 0 {
            self.status_message = Some("no further history (back)".into());
            return;
        }
        self.travel(self.history_cursor - 1);
    }

    fn go_forward(&mut self) {
        if self.history_cursor + 1 >= self.history.len() {
            self.status_message = Some("no further history (forward)".into());
            return;
        }
        self.travel(self.history_cursor + 1);
    }

    /// Move the history cursor to `target` and load that location. If the
    /// load fails the cursor and active buffer are left unchanged so the
    /// history stays internally consistent.
    fn travel(&mut self, target: usize) {
        let target_loc = self.history[target].clone();
        let leaving_scroll = self.scroll;
        if !self.load_document(&target_loc.path, None) {
            return;
        }
        self.history[self.history_cursor].scroll = leaving_scroll;
        self.history_cursor = target;
        self.scroll = target_loc.scroll.min(self.max_scroll());
    }

    /// Read and re-render `path`. Returns false on error (the buffer is
    /// left unchanged in that case); the status bar carries the reason.
    fn load_document(&mut self, path: &Path, anchor: Option<&str>) -> bool {
        match std::fs::read_to_string(path) {
            Ok(input) => {
                let doc = parse_and_render(&input);
                self.lines = doc.lines;
                self.anchors = doc.anchors;
                self.links = doc.links;
                self.title = path.display().to_string();
                self.path = path.to_path_buf();
                self.scroll = 0;
                self.focused_link = None;
                self.toc_selection = 0;
                // Drop search state — matches no longer point anywhere
                // meaningful in the new buffer.
                self.search_query.clear();
                self.search_matches.clear();
                self.search_cursor = None;
                if let Some(slug) = anchor {
                    self.jump_to_anchor(slug);
                }
                self.refresh_watch();
                true
            }
            Err(e) => {
                self.status_message = Some(format!("open {}: {e}", path.display()));
                false
            }
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

        let lines = self.convert_lines_for_display();
        let body = Paragraph::new(lines)
            .scroll((self.scroll as u16, 0))
            .wrap(Wrap { trim: false });
        frame.render_widget(body, body_area);

        let total = self.total_screen_rows();
        let last_visible = (self.scroll + self.body_height as usize).min(total);
        let hint = match self.mode {
            Mode::Normal => "?:help  /:find  o:toc  Tab:link  Enter:open  [/]:back/fwd  q:quit",
            Mode::Toc => "Enter:jump  Esc/q/o:close  j/k:select",
            Mode::Help => "Esc/q/?:close",
            Mode::Search => "Enter:confirm  Esc:cancel",
        };
        let status_text = if let Some(input) = &self.search_input {
            format!(" /{input}_  {hint} ")
        } else if let Some(msg) = &self.status_message {
            format!(" {msg} ")
        } else if !self.search_query.is_empty() {
            let count = self.search_matches.len();
            if count == 0 {
                format!(" /{}  (no match)  {hint} ", self.search_query)
            } else {
                let cur = self.search_cursor.map(|c| c + 1).unwrap_or(0);
                format!(
                    " /{}  [{}/{}]  n/N:next/prev  {hint} ",
                    self.search_query, cur, count,
                )
            }
        } else {
            format!(
                " {}  [{}-{}/{}]  {hint} ",
                self.title,
                self.scroll + 1,
                last_visible,
                total,
            )
        };
        let status =
            Paragraph::new(status_text).style(RStyle::default().add_modifier(Modifier::REVERSED));
        frame.render_widget(status, status_area);

        if self.mode == Mode::Toc && !self.anchors.is_empty() {
            self.draw_toc(frame, body_area);
        }
        if self.mode == Mode::Help {
            self.draw_help(frame, body_area);
        }
    }

    fn draw_help(&self, frame: &mut ratatui::Frame<'_>, body_area: Rect) {
        let area = centered_rect(70, 80, body_area);
        let key_col_width = HELP_ROWS
            .iter()
            .map(|(k, _)| k.chars().count())
            .max()
            .unwrap_or(0);
        let items: Vec<ListItem> = HELP_ROWS
            .iter()
            .map(|(k, d)| {
                let pad = key_col_width.saturating_sub(k.chars().count());
                let line = Line::from(vec![
                    Span::styled(
                        format!("{}{}", k, " ".repeat(pad)),
                        RStyle::default()
                            .fg(RColor::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("   "),
                    Span::raw(d.to_string()),
                ]);
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Keybindings "),
        );
        frame.render_widget(Clear, area);
        frame.render_widget(list, area);
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
    if w == 0 { 1 } else { w.div_ceil(body_w).max(1) }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

impl App {
    fn convert_lines_for_display(&self) -> Vec<Line<'static>> {
        // Group search matches by line so we only touch affected lines.
        let mut matches_by_line: Vec<Vec<&MatchPos>> = vec![Vec::new(); self.lines.len()];
        for m in &self.search_matches {
            if m.line < self.lines.len() {
                matches_by_line[m.line].push(m);
            }
        }
        let current_match = self.search_cursor.and_then(|i| self.search_matches.get(i));
        let focused_link_line = self
            .focused_link
            .and_then(|i| self.links.get(i))
            .map(|l| l.line);

        // Differentiate the active match from the rest: current = solid
        // yellow on black, other = dim reverse video.
        let search_style = Style::new().reversed();
        let current_search_style = Style::new().bg(Color::BrightYellow).fg(Color::Black).bold();
        let link_overlay = Style::new().reversed().bold();

        let mut result: Vec<Line<'static>> = Vec::with_capacity(self.lines.len());
        for (line_idx, line) in self.lines.iter().enumerate() {
            let line_matches = &matches_by_line[line_idx];
            let has_search = !line_matches.is_empty();
            let has_focused_link = focused_link_line == Some(line_idx);
            if !has_search && !has_focused_link {
                result.push(convert_line(line));
                continue;
            }

            let mut working = line.clone();

            // Apply focused-link overlay first while span indices still
            // match the original line layout.
            if has_focused_link
                && let Some(i) = self.focused_link
                && let Some(link) = self.links.get(i)
            {
                for span_idx in link.span_range.clone() {
                    if let Some(span) = working.spans.get_mut(span_idx) {
                        span.style = span.style.merge(link_overlay);
                    }
                }
            }

            // Then apply search-match overlays by splitting spans on char
            // boundaries. Apply matches right-to-left so earlier ranges
            // keep their offsets valid.
            if has_search {
                let mut ordered: Vec<&MatchPos> = line_matches.clone();
                ordered.sort_by_key(|m| m.char_start);
                for m in ordered.iter().rev() {
                    let is_current = current_match
                        .map(|cm| cm.line == m.line && cm.char_start == m.char_start)
                        .unwrap_or(false);
                    let style = if is_current {
                        current_search_style
                    } else {
                        search_style
                    };
                    apply_match_highlight(&mut working, m.char_start, m.char_end, style);
                }
            }

            result.push(convert_line(&working));
        }
        result
    }
}

/// Split spans that intersect the char range `[start, end)` and merge
/// `style` onto the slice inside that range.
fn apply_match_highlight(line: &mut StyledLine, start: usize, end: usize, style: Style) {
    if start >= end {
        return;
    }
    let mut new_spans: Vec<StyledSpan> = Vec::with_capacity(line.spans.len() + 2);
    let mut cursor = 0;
    for span in line.spans.drain(..) {
        let span_chars = span.text.chars().count();
        let span_start = cursor;
        let span_end = cursor + span_chars;
        cursor = span_end;

        if span_end <= start || span_start >= end {
            new_spans.push(span);
            continue;
        }

        let mut before = String::new();
        let mut middle = String::new();
        let mut after = String::new();
        for (i, c) in span.text.chars().enumerate() {
            let abs = span_start + i;
            if abs < start {
                before.push(c);
            } else if abs < end {
                middle.push(c);
            } else {
                after.push(c);
            }
        }
        if !before.is_empty() {
            new_spans.push(StyledSpan::new(before, span.style));
        }
        if !middle.is_empty() {
            new_spans.push(StyledSpan::new(middle, span.style.merge(style)));
        }
        if !after.is_empty() {
            new_spans.push(StyledSpan::new(after, span.style));
        }
    }
    line.spans = new_spans;
}

#[derive(Debug, Clone)]
enum LinkTarget {
    Url(String),
    SameAnchor(String),
    LocalFile(PathBuf),
    LocalAnchor(PathBuf, String),
}

fn classify_link(url: &str, current_dir: &Path) -> LinkTarget {
    if url.contains("://") || url.starts_with("mailto:") || url.starts_with("tel:") {
        return LinkTarget::Url(url.to_string());
    }
    if let Some(slug) = url.strip_prefix('#') {
        return LinkTarget::SameAnchor(slug.to_string());
    }
    if let Some((path, anchor)) = url.split_once('#') {
        return LinkTarget::LocalAnchor(current_dir.join(path), anchor.to_string());
    }
    LinkTarget::LocalFile(current_dir.join(url))
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
    if style.reversed {
        mods |= Modifier::REVERSED;
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
        Color::Rgb(r, g, b) => RColor::Rgb(r, g, b),
    }
}
