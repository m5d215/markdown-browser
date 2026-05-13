//! Keyboard bindings for the TUI. Phase 1: every key handler is
//! refactored to look up a single [`Action`] in a per-mode [`Keymap`]
//! and dispatch it. The defaults reproduce the previous hard-coded
//! match arms exactly; user-supplied overrides land in Phase 2.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};

/// A keyboard chord. We normalize away `SHIFT` for printable
/// uppercase chars so the user can write `"G"` (rather than `"shift+G"`)
/// in a future config and have it match what crossterm delivers.
pub type Chord = (KeyCode, KeyModifiers);

pub type Keymap = HashMap<Chord, Action>;

/// One unit of user intent. The dispatcher in `tui.rs` is the single
/// place that maps these to side effects, so adding a new keybinding
/// is just (1) extend this enum, (2) wire one arm in `dispatch`,
/// (3) bind it in the default map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Quit,

    // Normal-mode movement and document interaction.
    OpenToc,
    OpenHelp,
    OpenDir,
    ToggleLineNumbers,
    ToggleShortcodes,
    StartSearch,
    SearchNext,
    SearchPrev,
    LinkNext,
    LinkPrev,
    ActivateLink,
    HistoryBack,
    HistoryForward,
    CursorDown,
    CursorUp,
    CursorHalfPageDown,
    CursorHalfPageUp,
    CursorPageDown,
    CursorPageUp,
    CursorTop,
    CursorBottom,
    SectionNext,
    SectionPrev,
    EnterYank,

    // Yank mode.
    YankCancel,
    YankExpand,
    YankShrink,
    YankCopy,

    // TOC overlay.
    TocClose,
    TocSelectNext,
    TocSelectPrev,
    TocSelectTop,
    TocSelectBottom,
    TocConfirm,

    // Help overlay.
    HelpClose,

    // Directory browser overlay.
    DirClose,
    DirSelectNext,
    DirSelectPrev,
    DirSelectTop,
    DirSelectBottom,
    DirActivate,
    DirGoUp,
}

/// Drop `SHIFT` from the modifier set before lookup. Shift is redundant
/// for uppercase chars (already encoded in the `Char`) and for
/// `BackTab` (inherently Shift+Tab), and crossterm reports it
/// inconsistently across terminals. Strip it everywhere — any future
/// binding that genuinely needs Shift will live on `Char` casing.
pub fn normalize_chord(code: KeyCode, mods: KeyModifiers) -> Chord {
    (code, mods - KeyModifiers::SHIFT)
}

pub fn lookup(map: &Keymap, code: KeyCode, mods: KeyModifiers) -> Option<Action> {
    map.get(&normalize_chord(code, mods)).copied()
}

fn ch(c: char) -> Chord {
    (KeyCode::Char(c), KeyModifiers::NONE)
}

fn ctrl(c: char) -> Chord {
    (KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn key(code: KeyCode) -> Chord {
    (code, KeyModifiers::NONE)
}

fn alt(code: KeyCode) -> Chord {
    (code, KeyModifiers::ALT)
}

pub fn default_normal() -> Keymap {
    use Action::*;
    let mut m = Keymap::new();
    m.insert(ch('q'), Quit);
    m.insert(key(KeyCode::Esc), Quit);
    m.insert(ctrl('c'), Quit);
    m.insert(ch('o'), OpenToc);
    m.insert(ch('?'), OpenHelp);
    m.insert(ch('d'), OpenDir);
    m.insert(ch('#'), ToggleLineNumbers);
    m.insert(ch('e'), ToggleShortcodes);
    m.insert(ch('/'), StartSearch);
    m.insert(ch('n'), SearchNext);
    m.insert(ch('N'), SearchPrev);
    m.insert(key(KeyCode::Tab), LinkNext);
    m.insert(key(KeyCode::BackTab), LinkPrev);
    m.insert(key(KeyCode::Enter), ActivateLink);
    m.insert(key(KeyCode::Backspace), HistoryBack);
    m.insert(ch('['), HistoryBack);
    m.insert(alt(KeyCode::Left), HistoryBack);
    m.insert(ch(']'), HistoryForward);
    m.insert(alt(KeyCode::Right), HistoryForward);
    m.insert(ch('j'), CursorDown);
    m.insert(key(KeyCode::Down), CursorDown);
    m.insert(ch('k'), CursorUp);
    m.insert(key(KeyCode::Up), CursorUp);
    m.insert(ctrl('d'), CursorHalfPageDown);
    m.insert(ctrl('u'), CursorHalfPageUp);
    m.insert(ctrl('f'), CursorPageDown);
    m.insert(key(KeyCode::PageDown), CursorPageDown);
    m.insert(ctrl('b'), CursorPageUp);
    m.insert(key(KeyCode::PageUp), CursorPageUp);
    m.insert(ch('g'), CursorTop);
    m.insert(key(KeyCode::Home), CursorTop);
    m.insert(ch('G'), CursorBottom);
    m.insert(key(KeyCode::End), CursorBottom);
    m.insert(ch('}'), SectionNext);
    m.insert(ch('{'), SectionPrev);
    m.insert(ch('y'), EnterYank);
    m
}

pub fn default_yank() -> Keymap {
    use Action::*;
    let mut m = Keymap::new();
    m.insert(key(KeyCode::Esc), YankCancel);
    m.insert(ch('q'), YankCancel);
    m.insert(ch('y'), YankExpand);
    m.insert(ch('Y'), YankShrink);
    m.insert(key(KeyCode::Enter), YankCopy);
    m
}

pub fn default_toc() -> Keymap {
    use Action::*;
    let mut m = Keymap::new();
    m.insert(key(KeyCode::Esc), TocClose);
    m.insert(ch('q'), TocClose);
    m.insert(ch('o'), TocClose);
    m.insert(ch('j'), TocSelectNext);
    m.insert(key(KeyCode::Down), TocSelectNext);
    m.insert(ch('k'), TocSelectPrev);
    m.insert(key(KeyCode::Up), TocSelectPrev);
    m.insert(ch('g'), TocSelectTop);
    m.insert(key(KeyCode::Home), TocSelectTop);
    m.insert(ch('G'), TocSelectBottom);
    m.insert(key(KeyCode::End), TocSelectBottom);
    m.insert(key(KeyCode::Enter), TocConfirm);
    m
}

pub fn default_help() -> Keymap {
    use Action::*;
    let mut m = Keymap::new();
    m.insert(key(KeyCode::Esc), HelpClose);
    m.insert(ch('q'), HelpClose);
    m.insert(ch('?'), HelpClose);
    m
}

pub fn default_dir() -> Keymap {
    use Action::*;
    let mut m = Keymap::new();
    m.insert(key(KeyCode::Esc), DirClose);
    m.insert(ch('q'), DirClose);
    m.insert(ch('d'), DirClose);
    m.insert(ch('j'), DirSelectNext);
    m.insert(key(KeyCode::Down), DirSelectNext);
    m.insert(ch('k'), DirSelectPrev);
    m.insert(key(KeyCode::Up), DirSelectPrev);
    m.insert(ch('g'), DirSelectTop);
    m.insert(key(KeyCode::Home), DirSelectTop);
    m.insert(ch('G'), DirSelectBottom);
    m.insert(key(KeyCode::End), DirSelectBottom);
    m.insert(key(KeyCode::Enter), DirActivate);
    m.insert(key(KeyCode::Right), DirActivate);
    m.insert(ch('l'), DirActivate);
    m.insert(key(KeyCode::Left), DirGoUp);
    m.insert(ch('h'), DirGoUp);
    m
}
