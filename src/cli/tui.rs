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
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::cli::keymap::{self, Action, Keymap};
use crate::cli::net;
use crate::cli::source::{self, Source};
use crate::render::style::{Color, Style, StyledLine, StyledSpan};
use crate::render::width::spans_width;
use crate::render::{self, Anchor, BlockKind, BlockRange, Link, RenderOutput};

pub fn run(arg: Option<&Path>, emoji: bool) -> io::Result<()> {
    let arg = match arg {
        Some(p) if p.as_os_str() != "-" => p,
        Some(_) | None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TUI mode requires a file path, directory, or URL. Use `markdown-browser render` for stdin input.",
            ));
        }
    };
    let mut source = Source::from_arg(&arg.to_string_lossy());

    // `markdown-browser <dir>` — pick the README as the underlying
    // document when one exists, otherwise carry no buffer at all and
    // let the directory browser drive everything.
    let mut open_dir_at_start = false;
    if let Source::File(p) = &source
        && p.is_dir()
    {
        let dir = p.clone();
        open_dir_at_start = true;
        source = match source::find_readme(&dir) {
            Some(readme) => Source::File(readme),
            None => Source::Dir(dir),
        };
    }

    let input = match &source {
        Source::Dir(_) => String::new(),
        _ => read_source(&source).map_err(io::Error::other)?,
    };
    let doc = parse_and_render(&input, emoji);
    let title = source.display();

    // Start a filesystem watcher that pings the main loop whenever the
    // currently-active file changes. Failure to create one isn't fatal —
    // auto-reload just becomes unavailable. URLs never watch.
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

    let loaded = keymap::load();

    ratatui::run(move |terminal| {
        let mut app = App::new(doc, title, source, input, emoji);
        app.install_keymaps(loaded);
        if let Some(w) = watcher {
            app.attach_watcher(w, rx);
        }
        if open_dir_at_start {
            app.open_dir();
        }
        app.run(terminal)
    })
}

fn read_source(source: &Source) -> Result<String, String> {
    match source {
        Source::File(p) => std::fs::read_to_string(p).map_err(|e| e.to_string()),
        Source::Url(u) => net::fetch(u),
        Source::Dir(_) => Ok(String::new()),
    }
}

fn parse_and_render(input: &str, shortcodes: bool) -> RenderOutput {
    let arena = Arena::new();
    let root = render::parse::parse(&arena, input, shortcodes);
    render::render_document(root)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Toc,
    Help,
    Search,
    Yank,
    Dir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirEntryKind {
    Parent,
    Dir,
    File,
}

#[derive(Debug, Clone)]
struct DirEntry {
    name: String,
    kind: DirEntryKind,
    path: PathBuf,
}

/// One row in the directory overlay. `display` is pre-rendered (tree
/// prefix + icon + name + optional current-marker) so the renderer just
/// concatenates strings. `kind` and `path` drive activation: re-root for
/// Dir/Parent, open for File.
#[derive(Debug, Clone)]
struct DirRow {
    kind: DirEntryKind,
    path: PathBuf,
    display: String,
}

/// Live state for yank mode. `path` lists candidate ranges from smallest
/// (always the single cursor line) to largest (always the whole document).
/// `level` indexes the currently-active selection.
#[derive(Debug, Clone)]
struct YankSelection {
    path: Vec<(usize, usize)>,
    level: usize,
}

impl YankSelection {
    fn current(&self) -> (usize, usize) {
        self.path[self.level]
    }
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
    ("d", "ディレクトリブラウザ (h/← で親 / l/→ で開く)"),
    ("#", "行番号の表示 ON/OFF"),
    ("e", "絵文字 shortcode (`:rocket:`) の展開 ON/OFF"),
    ("/", "検索を開始 (Enter で確定 / Esc で取り消し)"),
    ("n / N", "次 / 前のマッチへ"),
    ("Tab / Shift-Tab", "リンクをフォーカス移動"),
    ("Enter", "フォーカス中のリンクを開く"),
    ("[ / ]", "履歴 Back / Forward"),
    ("Backspace", "履歴 Back (別名)"),
    ("Alt-← / Alt-→", "履歴 Back / Forward (端末対応依存)"),
    ("j / ↓", "カーソルを 1 行下へ"),
    ("k / ↑", "カーソルを 1 行上へ"),
    ("Ctrl-d / Ctrl-u", "半画面カーソル移動"),
    ("Ctrl-f / PgDn", "1 画面カーソル移動 (下)"),
    ("Ctrl-b / PgUp", "1 画面カーソル移動 (上)"),
    ("g / Home", "カーソルを先頭へ"),
    ("G / End", "カーソルを末尾へ"),
    ("<数字> G", "指定行へジャンプ (例: 42G で 42 行目)"),
    ("} / {", "次 / 前のセクション (見出し) へ"),
    ("y", "Yank 開始 / 選択を拡張"),
    ("Y (Shift-y)", "Yank の選択を縮小"),
    ("Enter (Yank 中)", "選択範囲をクリップボードへコピー"),
    ("Esc (Yank 中)", "Yank をキャンセル"),
];

#[derive(Debug, Clone)]
struct Location {
    source: Source,
    /// Logical line the cursor was on when leaving this entry.
    cursor: usize,
    /// Screen-row scroll offset (post-wrap), so the visible region is
    /// restored exactly even if the wrap width happens to match.
    scroll: usize,
}

struct App {
    lines: Vec<StyledLine>,
    anchors: Vec<Anchor>,
    links: Vec<Link>,
    blocks: Vec<BlockRange>,
    title: String,
    source: Source,
    /// Scroll offset measured in **screen rows** (after wrap), matching what
    /// `Paragraph::scroll` consumes. Heading jumps and lookups convert
    /// logical line indices through `screen_row_of`.
    scroll: usize,
    /// Cursor position as a logical line index into `lines`. Movement keys
    /// (j/k, Ctrl-d/u, g/G, ...) drive this; `scroll` follows via
    /// `scroll_to_cursor`.
    cursor_line: usize,
    /// Whether the line-number gutter is rendered. Toggled with `#`.
    show_line_numbers: bool,
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
    /// Live yank-mode selection. `Some` exactly when `mode == Mode::Yank`.
    yank: Option<YankSelection>,
    /// Pending numeric count entered before a motion (e.g. `42` waiting
    /// for `G`). Cleared by any non-digit key that isn't consuming it.
    pending_count: Option<u32>,
    /// Raw markdown source for the active buffer, kept so toggles
    /// (emoji on/off, etc.) can re-parse without re-fetching.
    raw_input: String,
    /// Whether to expand `:shortcode:` to its emoji during parse. Toggle
    /// with `e`.
    shortcodes: bool,
    /// Currently displayed directory in the directory overlay. `Some`
    /// exactly when `mode == Mode::Dir`. The view shows parent context
    /// (siblings + parent's files) with `dir_path`'s children inlined
    /// one level deeper, so `dir_path` is the "focused" dir, not the
    /// outermost shown.
    dir_path: Option<PathBuf>,
    dir_rows: Vec<DirRow>,
    dir_selection: usize,
    /// Filesystem watcher kept alive for the lifetime of the App so its
    /// callback continues to push events into `file_events`.
    watcher: Option<RecommendedWatcher>,
    /// Channel receiver fed by `watcher`.
    file_events: Option<Receiver<()>>,
    /// Path currently registered with the watcher.
    watched_path: Option<PathBuf>,
    /// Per-mode keymaps. Phase 1 just holds the defaults; Phase 2
    /// will layer a user config file on top.
    keymap_normal: Keymap,
    keymap_yank: Keymap,
    keymap_toc: Keymap,
    keymap_help: Keymap,
    keymap_dir: Keymap,
}

impl App {
    fn new(
        doc: RenderOutput,
        title: String,
        source: Source,
        raw_input: String,
        shortcodes: bool,
    ) -> Self {
        let initial = Location {
            source: source.clone(),
            cursor: 0,
            scroll: 0,
        };
        Self {
            lines: doc.lines,
            anchors: doc.anchors,
            links: doc.links,
            blocks: doc.blocks,
            title,
            source,
            scroll: 0,
            cursor_line: 0,
            show_line_numbers: true,
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
            yank: None,
            pending_count: None,
            raw_input,
            shortcodes,
            dir_path: None,
            dir_rows: Vec::new(),
            dir_selection: 0,
            watcher: None,
            file_events: None,
            watched_path: None,
            keymap_normal: keymap::default_normal(),
            keymap_yank: keymap::default_yank(),
            keymap_toc: keymap::default_toc(),
            keymap_help: keymap::default_help(),
            keymap_dir: keymap::default_dir(),
        }
    }

    fn attach_watcher(&mut self, watcher: RecommendedWatcher, rx: Receiver<()>) {
        self.watcher = Some(watcher);
        self.file_events = Some(rx);
        self.refresh_watch();
    }

    /// Replace the per-mode keymaps with the merged result from
    /// `keymap::load`. Any warnings collected while parsing the user
    /// config are funneled into the status bar so the user finds out
    /// about a bad config without having to dig through logs.
    fn install_keymaps(&mut self, loaded: keymap::LoadedKeymaps) {
        self.keymap_normal = loaded.normal;
        self.keymap_yank = loaded.yank;
        self.keymap_toc = loaded.toc;
        self.keymap_help = loaded.help;
        self.keymap_dir = loaded.dir;
        if !loaded.warnings.is_empty() {
            let path = loaded
                .config_path
                .as_deref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "config".into());
            let head = &loaded.warnings[0];
            let extra = loaded.warnings.len().saturating_sub(1);
            self.status_message = Some(if extra == 0 {
                format!("{path}: {head}")
            } else {
                format!("{path}: {head} (+{extra} more)")
            });
        }
    }

    /// Re-target the filesystem watcher at `self.source`, replacing any
    /// prior watch. No-op when no watcher is attached, or when the active
    /// source is a URL (nothing on the local filesystem to watch).
    fn refresh_watch(&mut self) {
        let Some(w) = self.watcher.as_mut() else {
            return;
        };
        if let Some(old) = self.watched_path.take() {
            let _ = w.unwatch(&old);
        }
        let Some(path) = self.source.as_file() else {
            return;
        };
        match w.watch(path, RecursiveMode::NonRecursive) {
            Ok(()) => {
                self.watched_path = Some(path.to_path_buf());
            }
            Err(e) => {
                self.status_message = Some(format!("watch {}: {e}", path.display()));
            }
        }
    }

    /// Re-read the currently-displayed file and refresh the buffer, keeping
    /// scroll position and re-running any active search. Invoked from the
    /// event loop when the filesystem watcher pings. Only Files are
    /// watched, so URL sources never trigger a reload here.
    fn reload_in_place(&mut self) {
        let Some(path) = self.source.as_file().map(Path::to_path_buf) else {
            return;
        };
        let saved_scroll = self.scroll;
        let saved_cursor = self.cursor_line;
        match std::fs::read_to_string(&path) {
            Ok(input) => {
                let doc = parse_and_render(&input, self.shortcodes);
                self.raw_input = input;
                self.lines = doc.lines;
                self.anchors = doc.anchors;
                self.links = doc.links;
                self.blocks = doc.blocks;
                self.focused_link = None;
                self.search_matches.clear();
                self.search_cursor = None;
                self.cancel_yank();
                if !self.search_query.is_empty() {
                    let q = self.search_query.clone();
                    self.commit_search(q);
                }
                self.cursor_line = saved_cursor.min(self.last_line_index());
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
            Mode::Yank => {
                self.handle_key_yank(code, mods);
                false
            }
            Mode::Dir => {
                self.handle_key_dir(code, mods);
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

    fn handle_key_help(&mut self, code: KeyCode, mods: KeyModifiers) {
        if let Some(action) = keymap::lookup(&self.keymap_help, code, mods) {
            self.dispatch(action);
        }
    }

    /// Apply a resolved [`Action`]. The keymap-driven handlers all go
    /// through here; returning `true` signals the event loop to exit.
    /// New keybindings hook in by extending the `Action` enum and
    /// adding one arm below — nothing else in the TUI moves.
    fn dispatch(&mut self, action: Action) -> bool {
        match action {
            Action::Quit => return true,

            Action::OpenToc => self.open_toc(),
            Action::OpenHelp => self.mode = Mode::Help,
            Action::OpenDir => self.open_dir(),
            Action::ToggleLineNumbers => self.toggle_line_numbers(),
            Action::ToggleShortcodes => self.toggle_shortcodes(),
            Action::StartSearch => {
                self.mode = Mode::Search;
                self.search_input = Some(String::new());
            }
            Action::SearchNext => self.search_step(1),
            Action::SearchPrev => self.search_step(-1),
            Action::LinkNext => self.focus_next_link(1),
            Action::LinkPrev => self.focus_next_link(-1),
            Action::ActivateLink => self.activate_focused_link(),
            Action::HistoryBack => self.go_back(),
            Action::HistoryForward => self.go_forward(),
            Action::CursorDown => self.cursor_by(1),
            Action::CursorUp => self.cursor_by(-1),
            Action::CursorHalfPageDown => self.cursor_by(self.half_page()),
            Action::CursorHalfPageUp => self.cursor_by(-self.half_page()),
            Action::CursorPageDown => self.cursor_by(self.page()),
            Action::CursorPageUp => self.cursor_by(-self.page()),
            Action::CursorTop => self.cursor_to(0),
            Action::CursorBottom => self.cursor_to(self.last_line_index()),
            Action::SectionNext => self.cursor_to_next_section(),
            Action::SectionPrev => self.cursor_to_prev_section(),
            Action::EnterYank => self.enter_yank(),

            Action::YankCancel => self.cancel_yank(),
            Action::YankExpand => self.yank_expand(),
            Action::YankShrink => self.yank_shrink(),
            Action::YankCopy => self.yank_copy(),

            Action::TocClose => self.mode = Mode::Normal,
            Action::TocSelectNext => {
                if self.toc_selection + 1 < self.anchors.len() {
                    self.toc_selection += 1;
                }
            }
            Action::TocSelectPrev => {
                if self.toc_selection > 0 {
                    self.toc_selection -= 1;
                }
            }
            Action::TocSelectTop => self.toc_selection = 0,
            Action::TocSelectBottom => {
                self.toc_selection = self.anchors.len().saturating_sub(1);
            }
            Action::TocConfirm => {
                if !self.anchors.is_empty() {
                    let line = self.anchors[self.toc_selection].line;
                    self.jump_and_push(line);
                    self.mode = Mode::Normal;
                }
            }

            Action::HelpClose => self.mode = Mode::Normal,

            Action::DirClose => self.close_dir(),
            Action::DirSelectNext => {
                let last = self.dir_rows.len().saturating_sub(1);
                if self.dir_selection < last {
                    self.dir_selection += 1;
                }
            }
            Action::DirSelectPrev => {
                if self.dir_selection > 0 {
                    self.dir_selection -= 1;
                }
            }
            Action::DirSelectTop => self.dir_selection = 0,
            Action::DirSelectBottom => {
                self.dir_selection = self.dir_rows.len().saturating_sub(1);
            }
            Action::DirActivate => self.activate_dir_entry(),
            Action::DirGoUp => self.dir_go_up(),
        }
        false
    }

    fn handle_key_normal(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        // Any keystroke clears a stale status message.
        self.status_message = None;

        // Digit prefix accumulator: `1`-`9` always start (or extend) a
        // pending count; `0` only extends an in-progress one (a lone `0`
        // is reserved for future start-of-line behaviour and would
        // otherwise swallow the bare keypress).
        if let (KeyCode::Char(c), KeyModifiers::NONE) = (code, mods)
            && c.is_ascii_digit()
            && (c != '0' || self.pending_count.is_some())
        {
            let digit = (c as u32) - ('0' as u32);
            let next = self
                .pending_count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(digit);
            self.pending_count = Some(next);
            return false;
        }

        // Esc while a count is pending cancels the count instead of
        // quitting — matches vim's "any non-consuming key clears it"
        // rule and prevents an accidental quit mid-typing.
        if let (KeyCode::Esc, _) = (code, mods)
            && self.pending_count.take().is_some()
        {
            return false;
        }

        // `<count>G` / `<count>End` — line-jump prefix, handled before
        // the keymap so the bare `G` still binds to `CursorBottom`.
        let count = self.pending_count.take();
        if let Some(n) = count
            && matches!((code, mods), (KeyCode::Char('G'), _) | (KeyCode::End, _))
        {
            let target = (n.max(1) - 1) as usize;
            self.cursor_to(target);
            return false;
        }

        if let Some(action) = keymap::lookup(&self.keymap_normal, code, mods) {
            return self.dispatch(action);
        }
        false
    }

    fn handle_key_yank(&mut self, code: KeyCode, mods: KeyModifiers) {
        self.status_message = None;
        if let Some(action) = keymap::lookup(&self.keymap_yank, code, mods) {
            self.dispatch(action);
        }
    }

    fn handle_key_toc(&mut self, code: KeyCode, mods: KeyModifiers) {
        if self.anchors.is_empty() {
            self.mode = Mode::Normal;
            return;
        }
        if let Some(action) = keymap::lookup(&self.keymap_toc, code, mods) {
            self.dispatch(action);
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
        let line = self.search_matches[idx].line.min(self.last_line_index());
        self.cursor_line = line;
        self.scroll_to_cursor();
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
        let line = self.links[i].line.min(self.last_line_index());
        self.cursor_line = line;
        self.scroll_to_cursor();
    }

    fn activate_focused_link(&mut self) {
        let Some(i) = self.focused_link else {
            self.status_message = Some("no link focused — press Tab to select one".into());
            return;
        };
        let url = self.links[i].url.clone();
        let target = classify_link(&url, &self.source);
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
                self.navigate(&Source::File(path), None);
            }
            LinkTarget::LocalAnchor(path, slug) => {
                self.navigate(&Source::File(path), Some(&slug));
            }
            LinkTarget::RemoteFile(url) => {
                self.navigate(&Source::Url(url), None);
            }
            LinkTarget::RemoteAnchor(url, slug) => {
                self.navigate(&Source::Url(url), Some(&slug));
            }
        }
    }

    /// Push a same-document jump onto the history stack. `target_line` is a
    /// logical line index; the heading is aligned to the top of the
    /// viewport.
    fn jump_and_push(&mut self, target_line: usize) {
        if let Some(loc) = self.history.get_mut(self.history_cursor) {
            loc.cursor = self.cursor_line;
            loc.scroll = self.scroll;
        }
        let target = target_line.min(self.last_line_index());
        self.cursor_line = target;
        self.scroll = self.screen_row_of(target).min(self.max_scroll());
        self.history.truncate(self.history_cursor + 1);
        self.history.push(Location {
            source: self.source.clone(),
            cursor: self.cursor_line,
            scroll: self.scroll,
        });
        self.history_cursor = self.history.len() - 1;
    }

    fn navigate_within(&mut self, slug: &str) {
        if let Some(a) = self.anchors.iter().find(|a| a.slug == slug) {
            let line = a.line;
            self.jump_and_push(line);
        } else {
            self.status_message = Some(format!("anchor #{slug} not found"));
        }
    }

    fn jump_to_anchor(&mut self, slug: &str) {
        if let Some(a) = self.anchors.iter().find(|a| a.slug == slug) {
            let line = a.line.min(self.last_line_index());
            self.cursor_line = line;
            self.scroll = self.screen_row_of(line).min(self.max_scroll());
        } else {
            self.status_message = Some(format!("anchor #{slug} not found"));
        }
    }

    /// Open a different document and record it as a new entry on the
    /// history stack. Forward history past the cursor is discarded.
    fn navigate(&mut self, source: &Source, anchor: Option<&str>) {
        // Save where we were leaving from.
        if let Some(loc) = self.history.get_mut(self.history_cursor) {
            loc.cursor = self.cursor_line;
            loc.scroll = self.scroll;
        }

        if !self.load_document(source, anchor) {
            return;
        }

        self.history.truncate(self.history_cursor + 1);
        self.history.push(Location {
            source: source.clone(),
            cursor: self.cursor_line,
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
        let leaving_cursor = self.cursor_line;
        if !self.load_document(&target_loc.source, None) {
            return;
        }
        self.history[self.history_cursor].scroll = leaving_scroll;
        self.history[self.history_cursor].cursor = leaving_cursor;
        self.history_cursor = target;
        self.cursor_line = target_loc.cursor.min(self.last_line_index());
        self.scroll = target_loc.scroll.min(self.max_scroll());
    }

    /// Read and re-render `source`. Returns false on error (the buffer is
    /// left unchanged in that case); the status bar carries the reason.
    fn load_document(&mut self, source: &Source, anchor: Option<&str>) -> bool {
        if let Source::Url(u) = source {
            self.status_message = Some(format!("fetching {u}…"));
        }
        match read_source(source) {
            Ok(input) => {
                let doc = parse_and_render(&input, self.shortcodes);
                self.raw_input = input;
                self.lines = doc.lines;
                self.anchors = doc.anchors;
                self.links = doc.links;
                self.blocks = doc.blocks;
                self.title = source.display();
                self.source = source.clone();
                self.scroll = 0;
                self.cursor_line = 0;
                self.focused_link = None;
                self.toc_selection = 0;
                // Drop search state — matches no longer point anywhere
                // meaningful in the new buffer.
                self.search_query.clear();
                self.search_matches.clear();
                self.search_cursor = None;
                self.cancel_yank();
                if let Some(slug) = anchor {
                    self.jump_to_anchor(slug);
                }
                self.refresh_watch();
                self.status_message = None;
                // A dir source carries no buffer — the directory browser
                // is the actual view, so open it automatically (covers
                // history navigation back to a `markdown-browser <dir>`
                // start).
                if matches!(source, Source::Dir(_)) {
                    self.open_dir();
                }
                true
            }
            Err(e) => {
                self.status_message = Some(format!("open {}: {e}", source.display()));
                false
            }
        }
    }

    /// Open the directory overlay scoped to the active file's parent.
    /// No-op + status message for URL sources or any I/O failure — the
    /// underlying document view stays put.
    fn open_dir(&mut self) {
        let (dir, focus): (PathBuf, PathBuf) = match &self.source {
            Source::Dir(d) => (d.clone(), d.clone()),
            Source::File(f) => {
                let f = f.clone();
                let Some(parent) = f.parent() else {
                    self.status_message = Some("no parent directory".into());
                    return;
                };
                let dir = if parent.as_os_str().is_empty() {
                    Path::new(".").to_path_buf()
                } else {
                    parent.to_path_buf()
                };
                (dir, f)
            }
            Source::Url(_) => {
                self.status_message = Some("directory browser unavailable for URL sources".into());
                return;
            }
        };
        self.load_dir(dir, &focus);
    }

    fn load_dir(&mut self, dir: PathBuf, focus_on: &Path) {
        // Canonicalize so `..` keeps walking up real ancestors instead of
        // bottoming out at the empty path that `Path::new("foo").parent()`
        // hands back for relative inputs.
        let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        match build_dir_view(&dir) {
            Ok(rows) => {
                let selection = rows
                    .iter()
                    .position(|r| r.path == focus_on)
                    .or_else(|| {
                        // Fall back to matching by filename — useful when
                        // `focus_on` came from a non-canonical path.
                        let name = focus_on.file_name()?;
                        rows.iter().position(|r| r.path.file_name() == Some(name))
                    })
                    .unwrap_or(0);
                self.dir_path = Some(dir);
                self.dir_rows = rows;
                self.dir_selection = selection;
                self.mode = Mode::Dir;
            }
            Err(e) => {
                self.status_message = Some(format!("dir {}: {e}", dir.display()));
            }
        }
    }

    fn close_dir(&mut self) {
        self.dir_path = None;
        self.dir_rows.clear();
        self.dir_selection = 0;
        self.mode = Mode::Normal;
    }

    fn activate_dir_entry(&mut self) {
        let Some(row) = self.dir_rows.get(self.dir_selection).cloned() else {
            return;
        };
        match row.kind {
            DirEntryKind::File => {
                self.close_dir();
                self.navigate(&Source::File(row.path), None);
            }
            DirEntryKind::Dir | DirEntryKind::Parent => {
                // Activating a dir (including `..` or the parent header)
                // re-roots the tree on that dir. `focus_on` = the new
                // current so the selection lands on the row representing
                // the re-rooted dir.
                let focus = row.path.clone();
                self.load_dir(row.path, &focus);
            }
        }
    }

    fn handle_key_dir(&mut self, code: KeyCode, mods: KeyModifiers) {
        self.status_message = None;
        if let Some(action) = keymap::lookup(&self.keymap_dir, code, mods) {
            self.dispatch(action);
        }
    }

    /// `h` / Left in the directory overlay: re-root the view on the
    /// current dir's parent. No-op at the filesystem root.
    fn dir_go_up(&mut self) {
        let parent = self.dir_path.as_ref().and_then(|p| p.parent());
        if let Some(parent) = parent {
            let parent = parent.to_path_buf();
            self.load_dir(parent.clone(), &parent);
        }
    }

    fn open_toc(&mut self) {
        if self.anchors.is_empty() {
            return;
        }
        self.mode = Mode::Toc;
        // Highlight the heading whose section currently contains the
        // cursor — the deepest anchor whose line is at or above the
        // cursor.
        self.toc_selection = self
            .anchors
            .iter()
            .rposition(|a| a.line <= self.cursor_line)
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

        let body_w = body_area.width as usize;
        let lines = self.convert_lines_for_display();
        let wrapped: Vec<Line<'static>> = lines
            .into_iter()
            .flat_map(|l| wrap_line(l, body_w))
            .collect();
        let body = Paragraph::new(wrapped).scroll((self.scroll as u16, 0));
        frame.render_widget(body, body_area);

        let hint = match self.mode {
            Mode::Normal => "?:help  /:find  o:toc  d:dir  Tab:link  Enter:open  y:yank  q:quit",
            Mode::Toc => "Enter:jump  Esc/q/o:close  j/k:select",
            Mode::Help => "Esc/q/?:close",
            Mode::Search => "Enter:confirm  Esc:cancel",
            Mode::Yank => "y:expand  Y:shrink  Enter:copy  Esc:cancel",
            Mode::Dir => "Enter/l/→:open  h/←:up  j/k:select  Esc/q/d:close",
        };
        let status_text = if let Some(input) = &self.search_input {
            format!(" /{input}_  {hint} ")
        } else if self.mode == Mode::Yank
            && let Some(y) = &self.yank
        {
            let (start, end) = y.current();
            let n = end - start + 1;
            format!(
                " yank  [{}/{}]  {} line{}  {hint} ",
                y.level + 1,
                y.path.len(),
                n,
                if n == 1 { "" } else { "s" },
            )
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
        } else if let Some(n) = self.pending_count {
            format!(
                " {}  [count: {n}_]  G:jump  Esc:cancel  {hint} ",
                self.title,
            )
        } else {
            format!(
                " {}  [{}/{}]  {hint} ",
                self.title,
                self.cursor_line + 1,
                self.lines.len(),
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
        if self.mode == Mode::Dir {
            self.draw_dir(frame, body_area);
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
        let prefixes = toc_tree_prefixes(&self.anchors);
        let items: Vec<ListItem> = self
            .anchors
            .iter()
            .zip(prefixes)
            .map(|(a, prefix)| ListItem::new(format!("{prefix}{}", a.text)))
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

    fn draw_dir(&self, frame: &mut ratatui::Frame<'_>, body_area: Rect) {
        let area = centered_rect(70, 80, body_area);
        let items: Vec<ListItem> = self
            .dir_rows
            .iter()
            .map(|r| ListItem::new(r.display.clone()))
            .collect();
        let title = match &self.dir_path {
            Some(p) => format!(" {} ", p.display()),
            None => " Directory ".into(),
        };
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(title),
            )
            .highlight_style(
                RStyle::default()
                    .fg(RColor::Black)
                    .bg(RColor::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(" › ");
        let mut state = ListState::default();
        state.select(Some(self.dir_selection));
        frame.render_widget(Clear, area);
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn cursor_by(&mut self, delta: isize) {
        let max = self.last_line_index() as isize;
        let next = (self.cursor_line as isize) + delta;
        self.cursor_line = next.clamp(0, max) as usize;
        self.scroll_to_cursor();
    }

    fn cursor_to(&mut self, line: usize) {
        self.cursor_line = line.min(self.last_line_index());
        self.scroll_to_cursor();
    }

    /// Jump the cursor to the next heading line. Anchors are kept in
    /// document order during rendering, so a linear scan is fine. No-op
    /// when the document has no headings or the cursor is already at or
    /// past the last one.
    fn cursor_to_next_section(&mut self) {
        if let Some(a) = self.anchors.iter().find(|a| a.line > self.cursor_line) {
            self.cursor_to(a.line);
        }
    }

    /// Jump the cursor to the previous heading line.
    fn cursor_to_prev_section(&mut self) {
        if let Some(a) = self
            .anchors
            .iter()
            .rev()
            .find(|a| a.line < self.cursor_line)
        {
            self.cursor_to(a.line);
        }
    }

    /// Adjust `scroll` so the cursor line is visible. No-op if it already
    /// fits in the viewport.
    fn scroll_to_cursor(&mut self) {
        let body_h = self.body_height as usize;
        if body_h == 0 {
            return;
        }
        let row_start = self.screen_row_of(self.cursor_line);
        let row_end = self.screen_row_of(self.cursor_line + 1);
        if row_start < self.scroll {
            self.scroll = row_start;
        } else if row_end > self.scroll + body_h {
            self.scroll = row_end.saturating_sub(body_h);
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn last_line_index(&self) -> usize {
        self.lines.len().saturating_sub(1)
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
        let gutter = self.gutter_width();
        let end = until.min(self.lines.len());
        let mut rows = 0usize;
        for line in &self.lines[..end] {
            // Count against the line as it'll be rendered: raw content +
            // gutter prefix, char-wrapped to body_w. Padding (cursor /
            // yank) tops content out at body_w so it stays a single row,
            // which `wrapped_rows` already handles via the empty-line
            // fallback when w == 0.
            let w = spans_width(&line.spans) + gutter;
            rows += wrapped_rows_for_width(w, body_w);
        }
        rows
    }

    /// Width of the line-number gutter (including its trailing space), or
    /// 0 when the gutter is hidden.
    fn gutter_width(&self) -> usize {
        if !self.show_line_numbers {
            return 0;
        }
        let max = self.lines.len().max(1);
        let digits = max.ilog10() as usize + 1;
        digits + 1
    }

    fn toggle_line_numbers(&mut self) {
        self.show_line_numbers = !self.show_line_numbers;
        // Wrap geometry changed — re-pin the viewport on the cursor so we
        // don't end up looking at the wrong region.
        self.scroll_to_cursor();
    }

    /// Re-parse the cached buffer with the opposite `shortcodes` setting.
    /// Line counts can change (a `:rocket:` is one cell shorter than
    /// `:rocket:` text), so re-pin the viewport on the cursor afterwards.
    fn toggle_shortcodes(&mut self) {
        self.shortcodes = !self.shortcodes;
        let doc = parse_and_render(&self.raw_input, self.shortcodes);
        self.lines = doc.lines;
        self.anchors = doc.anchors;
        self.links = doc.links;
        self.blocks = doc.blocks;
        self.focused_link = None;
        self.cancel_yank();
        if !self.search_query.is_empty() {
            let q = self.search_query.clone();
            self.commit_search(q);
        }
        self.cursor_line = self.cursor_line.min(self.last_line_index());
        self.scroll_to_cursor();
        self.status_message = Some(
            if self.shortcodes {
                "emoji shortcodes: on"
            } else {
                "emoji shortcodes: off"
            }
            .into(),
        );
    }

    fn enter_yank(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        let path = self.build_yank_path(self.cursor_line);
        self.yank = Some(YankSelection { path, level: 0 });
        self.mode = Mode::Yank;
        self.scroll_to_yank_selection();
    }

    fn yank_expand(&mut self) {
        let Some(y) = self.yank.as_mut() else { return };
        if y.level + 1 < y.path.len() {
            y.level += 1;
            self.scroll_to_yank_selection();
        }
    }

    fn yank_shrink(&mut self) {
        let Some(y) = self.yank.as_mut() else { return };
        if y.level > 0 {
            y.level -= 1;
            self.scroll_to_yank_selection();
        }
    }

    fn cancel_yank(&mut self) {
        if self.yank.take().is_some() {
            self.mode = Mode::Normal;
        }
    }

    fn yank_copy(&mut self) {
        let Some(y) = self.yank.clone() else { return };
        let (start, end) = y.current();
        let text = self.selection_text(start, end);
        let line_count = end - start + 1;
        match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text)) {
            Ok(()) => {
                self.status_message = Some(format!("yanked {line_count} lines"));
            }
            Err(e) => {
                self.status_message = Some(format!("yank failed: {e}"));
            }
        }
        self.yank = None;
        self.mode = Mode::Normal;
    }

    fn selection_text(&self, start: usize, end: usize) -> String {
        let end = end.min(self.last_line_index());
        let mut out = String::new();
        for idx in start..=end {
            if idx > start {
                out.push('\n');
            }
            for span in &self.lines[idx].spans {
                out.push_str(&span.text);
            }
        }
        out
    }

    /// Build the ordered list of candidate ranges for yank-mode expansion,
    /// from smallest (single cursor line) to largest (whole document). Each
    /// step must be a strict superset of the previous.
    fn build_yank_path(&self, cursor: usize) -> Vec<(usize, usize)> {
        let last = self.last_line_index();
        let cursor = cursor.min(last);
        let mut path: Vec<(usize, usize)> = vec![(cursor, cursor)];

        // Step 2: every leaf block containing the cursor, smallest to
        // largest. A regular paragraph contributes one entry; a fenced code
        // block contributes two (inner content, then full with fences).
        let mut leaves: Vec<&BlockRange> = self
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Leaf && b.start <= cursor && cursor <= b.end)
            .collect();
        leaves.sort_by_key(|b| b.end - b.start);
        for leaf in leaves {
            push_strict_superset(&mut path, (leaf.start, leaf.end));
        }

        // Step 3: containers (list-item / blockquote) from innermost to
        // outermost. Multiple nested containers contribute multiple stops.
        let mut containers: Vec<&BlockRange> = self
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Container && b.start <= cursor && cursor <= b.end)
            .collect();
        containers.sort_by_key(|b| b.end - b.start);
        for c in containers {
            push_strict_superset(&mut path, (c.start, c.end));
        }

        // Step 4: heading sections h6 → h5 → ... → h1, only the ones whose
        // section currently contains the cursor.
        let mut open: [Option<usize>; 7] = [None; 7];
        for a in &self.anchors {
            if a.line > cursor {
                break;
            }
            let lvl = a.level.clamp(1, 6) as usize;
            for slot in open.iter_mut().skip(lvl) {
                *slot = None;
            }
            open[lvl] = Some(a.line);
        }
        for n in (1..=6).rev() {
            let Some(start) = open[n] else { continue };
            let end = self
                .anchors
                .iter()
                .find(|a| a.line > start && (a.level as usize) <= n)
                .map(|a| a.line.saturating_sub(1))
                .unwrap_or(last);
            push_strict_superset(&mut path, (start, end));
        }

        // Step 5: whole document.
        push_strict_superset(&mut path, (0, last));

        path
    }

    fn scroll_to_yank_selection(&mut self) {
        let Some(y) = self.yank.as_ref() else { return };
        let (start, end) = y.current();
        let body_h = self.body_height as usize;
        if body_h == 0 {
            return;
        }
        let row_start = self.screen_row_of(start);
        let row_end = self.screen_row_of(end + 1);
        if row_start < self.scroll {
            self.scroll = row_start;
        } else if row_end > self.scroll + body_h {
            // Prefer to keep the selection's top in view when it's bigger
            // than the viewport.
            let span = row_end.saturating_sub(row_start);
            self.scroll = if span >= body_h {
                row_start
            } else {
                row_end.saturating_sub(body_h)
            };
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }
}

/// Append `range` to `path` only when it's a strict superset of the last
/// entry. Keeps the path deduplicated and monotonically expanding.
fn push_strict_superset(path: &mut Vec<(usize, usize)>, range: (usize, usize)) {
    if let Some(&last) = path.last()
        && range.0 <= last.0
        && range.1 >= last.1
        && (range.0 < last.0 || range.1 > last.1)
    {
        path.push(range);
    } else if path.is_empty() {
        path.push(range);
    }
}

fn wrapped_rows_for_width(line_w: usize, body_w: usize) -> usize {
    let body_w = body_w.max(1);
    if line_w == 0 {
        1
    } else {
        line_w.div_ceil(body_w).max(1)
    }
}

/// Hard char-boundary wrap of a ratatui `Line` to fit `body_w` cells.
/// Drives the body view in `draw()`; we wrap ourselves and disable
/// `Paragraph::wrap` so that `screen_row_of` matches what's on screen
/// exactly. ratatui's `WordWrapper` would otherwise split at word
/// boundaries and produce extra rows we can't account for, which made
/// `G` fall short of the bottom on narrow viewports.
fn wrap_line(line: Line<'static>, body_w: usize) -> Vec<Line<'static>> {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;

    let body_w = body_w.max(1);
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut col = 0usize;

    for span in line.spans {
        let style = span.style;
        let mut buf = String::new();
        for g in span.content.graphemes(true) {
            let gw = UnicodeWidthStr::width(g);
            if col > 0 && col + gw > body_w {
                if !buf.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut buf), style));
                }
                out.push(Line::from(std::mem::take(&mut spans)));
                col = 0;
            }
            buf.push_str(g);
            col += gw;
        }
        if !buf.is_empty() {
            spans.push(Span::styled(buf, style));
        }
    }
    out.push(Line::from(spans));
    out
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

/// Read a directory and return the entries the overlay can usefully
/// show: subdirectories and renderable files. Hidden entries are
/// skipped. Sort order: dirs alphabetical, then files alphabetical with
/// `README*` floated to the top.
fn read_renderable_entries(path: &Path) -> std::io::Result<Vec<DirEntry>> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let ft = entry.file_type()?;
        let p = entry.path();
        if ft.is_dir() {
            dirs.push(DirEntry {
                name,
                kind: DirEntryKind::Dir,
                path: p,
            });
        } else if ft.is_file() && is_renderable_local_file(&p) {
            files.push(DirEntry {
                name,
                kind: DirEntryKind::File,
                path: p,
            });
        }
    }
    dirs.sort_by(|a, b| a.name.cmp(&b.name));
    files.sort_by(|a, b| {
        let ar = a.name.to_ascii_lowercase().starts_with("readme");
        let br = b.name.to_ascii_lowercase().starts_with("readme");
        match (ar, br) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });
    let mut out = Vec::with_capacity(dirs.len() + files.len());
    out.extend(dirs);
    out.extend(files);
    Ok(out)
}

/// Build the rows that the directory overlay shows. The view is
/// `parent / cur / cur's children` (depth 0 / 1 / 2), with `..` at the
/// top, the parent dir name as the tree root, parent's entries at
/// depth 1, and `cur`'s entries inline-expanded under its own depth-1
/// row. Falls back to "cur only" when `cur` has no parent.
fn build_dir_view(cur: &Path) -> std::io::Result<Vec<DirRow>> {
    let mut rows = Vec::new();
    let parent = cur.parent();

    if let Some(parent) = parent {
        // `..` row: re-roots to the dir above `cur`. Use `cur.parent()`
        // here, not `parent.parent()`, so activating `..` and pressing
        // `h` both land on the same destination.
        rows.push(DirRow {
            kind: DirEntryKind::Parent,
            path: parent.to_path_buf(),
            display: "📁 ../".into(),
        });

        // Parent header: visual anchor for the tree, doubles as a click
        // target — activating it does the same thing as `..`.
        let parent_label = parent
            .file_name()
            .and_then(|s| s.to_str())
            .map(|n| format!("{n}/"))
            .unwrap_or_else(|| parent.display().to_string());
        rows.push(DirRow {
            kind: DirEntryKind::Dir,
            path: parent.to_path_buf(),
            display: format!("📁 {parent_label}"),
        });

        let siblings = read_renderable_entries(parent)?;
        let last = siblings.len().saturating_sub(1);
        for (i, e) in siblings.iter().enumerate() {
            let is_last = i == last;
            let leaf = if is_last { "└── " } else { "├── " };
            let is_current = e.path == cur;
            let marker = if is_current { "  ◀" } else { "" };
            rows.push(DirRow {
                kind: e.kind,
                path: e.path.clone(),
                display: format!("{leaf}{} {}{marker}", dir_entry_icon(e), entry_name(e),),
            });
            if is_current && let Ok(children) = read_renderable_entries(cur) {
                let down_col = if is_last { "    " } else { "│   " };
                let clast = children.len().saturating_sub(1);
                for (j, c) in children.iter().enumerate() {
                    let cleaf = if j == clast {
                        "└── "
                    } else {
                        "├── "
                    };
                    rows.push(DirRow {
                        kind: c.kind,
                        path: c.path.clone(),
                        display: format!(
                            "{down_col}{cleaf}{} {}",
                            dir_entry_icon(c),
                            entry_name(c),
                        ),
                    });
                }
            }
        }
    } else {
        // `cur` is the filesystem root: skip the parent layer, just
        // show `cur` as the tree root with its children at depth 1.
        rows.push(DirRow {
            kind: DirEntryKind::Dir,
            path: cur.to_path_buf(),
            display: format!("📁 {}  ◀", cur.display()),
        });
        if let Ok(children) = read_renderable_entries(cur) {
            let last = children.len().saturating_sub(1);
            for (i, c) in children.iter().enumerate() {
                let leaf = if i == last {
                    "└── "
                } else {
                    "├── "
                };
                rows.push(DirRow {
                    kind: c.kind,
                    path: c.path.clone(),
                    display: format!("{leaf}{} {}", dir_entry_icon(c), entry_name(c)),
                });
            }
        }
    }

    Ok(rows)
}

/// Display name for a dir entry — appends `/` for directories so the
/// tree row clearly distinguishes folder rows from files.
fn entry_name(e: &DirEntry) -> String {
    match e.kind {
        DirEntryKind::Dir => format!("{}/", e.name),
        DirEntryKind::File | DirEntryKind::Parent => e.name.clone(),
    }
}

/// Pick a leading glyph for a directory-browser row. Emoji (width 2) is
/// rendered as-is by ratatui; works on any terminal with reasonable
/// emoji coverage without a Nerd Font dependency.
fn dir_entry_icon(entry: &DirEntry) -> &'static str {
    match entry.kind {
        DirEntryKind::Parent | DirEntryKind::Dir => "📁",
        DirEntryKind::File => {
            let ext = entry
                .path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase());
            match ext.as_deref() {
                Some("md" | "markdown") => "📝",
                _ => "📄",
            }
        }
    }
}

/// Heuristic: which local files can we open without showing garbage?
/// Stricter than the URL variant (which accepts any extensionless path)
/// because the filesystem hands us executables and other binaries that
/// happen to lack an extension.
fn is_renderable_local_file(p: &Path) -> bool {
    if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
        let lower = ext.to_ascii_lowercase();
        return matches!(lower.as_str(), "md" | "markdown" | "txt" | "text");
    }
    p.file_name()
        .and_then(|s| s.to_str())
        .map(|n| n.eq_ignore_ascii_case("license") || n.eq_ignore_ascii_case("readme"))
        .unwrap_or(false)
}

/// Build one tree-style prefix per anchor (`├── ` / `└── ` with `│   `
/// continuation columns for ancestors that still have siblings below).
///
/// A "sibling at level `d`" is the next anchor whose level is `<= d`; it
/// counts as a sibling only when its level equals `d`. Anchors are in
/// document order, so a single forward scan per (anchor, depth) is enough
/// — fine for TOC sizes.
fn toc_tree_prefixes(anchors: &[Anchor]) -> Vec<String> {
    anchors
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let level = a.level.max(1) as usize;
            let mut prefix = String::with_capacity(level * 4);
            for d in 1..=level {
                let last = is_last_at_depth(anchors, i, d);
                let leaf = d == level;
                prefix.push_str(match (leaf, last) {
                    (true, false) => "├── ",
                    (true, true) => "└── ",
                    (false, false) => "│   ",
                    (false, true) => "    ",
                });
            }
            prefix
        })
        .collect()
}

/// True iff no later anchor extends the subtree at level `depth` containing
/// `i`. We walk forward, stop the moment we leave the subtree (level <
/// depth), and look for any equal-depth peer in between.
fn is_last_at_depth(anchors: &[Anchor], i: usize, depth: usize) -> bool {
    for a in &anchors[i + 1..] {
        let lvl = a.level as usize;
        if lvl < depth {
            return true;
        }
        if lvl == depth {
            return false;
        }
    }
    true
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

        // Dark blue-gray bg for the cursor line — applied per-span so it
        // survives ratatui's wrap rendering.
        let cursor_overlay = Style::new().bg(Color::Rgb(72, 72, 92));
        // Distinct, more saturated bg used for the yank selection so the
        // user can see at a glance how big the current scope is.
        let yank_overlay = Style::new().bg(Color::Rgb(58, 78, 110));

        let yank_range = self.yank.as_ref().map(|y| y.current());

        let show_numbers = self.show_line_numbers;
        let gutter_w = self.gutter_width();
        let number_pad = gutter_w.saturating_sub(1);
        let normal_lineno = Style::new().fg(Color::BrightBlack);
        let cursor_lineno = Style::new().fg(Color::BrightYellow).bold();

        let mut result: Vec<Line<'static>> = Vec::with_capacity(self.lines.len());
        for (line_idx, line) in self.lines.iter().enumerate() {
            let line_matches = &matches_by_line[line_idx];
            let has_search = !line_matches.is_empty();
            let has_focused_link = focused_link_line == Some(line_idx);
            let is_cursor = line_idx == self.cursor_line;
            let in_yank = yank_range.is_some_and(|(s, e)| line_idx >= s && line_idx <= e);
            if !has_search && !has_focused_link && !is_cursor && !show_numbers && !in_yank {
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

            // Order: yank bg goes on first as a base layer (covers entire
            // selection); cursor-line overlay merges on top to keep the
            // anchor row distinguishable. Both reach the right edge via
            // the trailing-padding span below.
            if in_yank {
                for span in &mut working.spans {
                    span.style = span.style.merge(yank_overlay);
                }
            }
            if is_cursor {
                for span in &mut working.spans {
                    span.style = span.style.merge(cursor_overlay);
                }
            }

            if show_numbers {
                let mut gstyle = if is_cursor {
                    cursor_lineno
                } else {
                    normal_lineno
                };
                if in_yank {
                    gstyle = gstyle.merge(yank_overlay);
                }
                if is_cursor {
                    gstyle = gstyle.merge(cursor_overlay);
                }
                let text = format!("{:>width$} ", line_idx + 1, width = number_pad);
                working.spans.insert(0, StyledSpan::new(text, gstyle));
            }

            if is_cursor || in_yank {
                let body_w = self.body_width as usize;
                let used = spans_width(&working.spans);
                if body_w > 0 && used < body_w {
                    let pad_style = if is_cursor {
                        cursor_overlay
                    } else {
                        yank_overlay
                    };
                    working
                        .spans
                        .push(StyledSpan::new(" ".repeat(body_w - used), pad_style));
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
    /// Hand off to the OS — non-markdown URLs, mailto/tel, etc.
    Url(String),
    /// `#slug` within the current document.
    SameAnchor(String),
    /// Local markdown file on disk.
    LocalFile(PathBuf),
    /// Local file with a heading anchor.
    LocalAnchor(PathBuf, String),
    /// Remote markdown file we can fetch and render in-app.
    RemoteFile(String),
    /// Remote markdown file with a heading anchor.
    RemoteAnchor(String, String),
}

fn classify_link(link: &str, current: &Source) -> LinkTarget {
    // Same-document anchor — always handled internally.
    if let Some(slug) = link.strip_prefix('#') {
        return LinkTarget::SameAnchor(slug.to_string());
    }

    // mailto / tel — straight to the OS, ignoring the current source.
    if link.starts_with("mailto:") || link.starts_with("tel:") {
        return LinkTarget::Url(link.to_string());
    }

    // Try to resolve the link as a URL, either absolute or relative to
    // the current URL source.
    let resolved = if source::is_url(link) {
        url::Url::parse(link).ok()
    } else if let Source::Url(base) = current {
        url::Url::parse(base).and_then(|b| b.join(link)).ok()
    } else {
        None
    };

    if let Some(u) = resolved {
        return classify_resolved_url(u);
    }

    // Plain local path (only meaningful when current source is a File).
    let parent = match current {
        Source::File(p) => p
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        Source::Dir(p) => p.clone(),
        Source::Url(_) => PathBuf::from("."),
    };
    if let Some((path, anchor)) = link.split_once('#') {
        return LinkTarget::LocalAnchor(parent.join(path), anchor.to_string());
    }
    LinkTarget::LocalFile(parent.join(link))
}

fn classify_resolved_url(mut u: url::Url) -> LinkTarget {
    if !source::url_path_is_renderable(&u) {
        return LinkTarget::Url(u.to_string());
    }
    let fragment = u.fragment().map(str::to_string);
    u.set_fragment(None);
    let s = u.to_string();
    match fragment {
        Some(slug) => LinkTarget::RemoteAnchor(s, slug),
        None => LinkTarget::RemoteFile(s),
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
