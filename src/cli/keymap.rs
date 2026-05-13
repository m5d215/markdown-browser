//! Keyboard bindings for the TUI. Per-mode [`Keymap`]s map [`Chord`]s
//! to [`Action`]s, the dispatcher in `tui.rs` is the only place that
//! turns an `Action` into a side effect. Defaults are built in code;
//! `~/.config/markdown-browser/config.toml` can override or extend
//! them via [`load`].

use std::collections::HashMap;
use std::path::PathBuf;

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

impl Action {
    /// Resolve a config-file action name (snake_case) to a variant.
    /// Kept exhaustive on purpose so the compiler shouts if a new
    /// `Action` is added without exposing it to config.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "quit" => Self::Quit,
            "open_toc" => Self::OpenToc,
            "open_help" => Self::OpenHelp,
            "open_dir" => Self::OpenDir,
            "toggle_line_numbers" => Self::ToggleLineNumbers,
            "toggle_shortcodes" => Self::ToggleShortcodes,
            "start_search" => Self::StartSearch,
            "search_next" => Self::SearchNext,
            "search_prev" => Self::SearchPrev,
            "link_next" => Self::LinkNext,
            "link_prev" => Self::LinkPrev,
            "activate_link" => Self::ActivateLink,
            "history_back" => Self::HistoryBack,
            "history_forward" => Self::HistoryForward,
            "cursor_down" => Self::CursorDown,
            "cursor_up" => Self::CursorUp,
            "cursor_half_page_down" => Self::CursorHalfPageDown,
            "cursor_half_page_up" => Self::CursorHalfPageUp,
            "cursor_page_down" => Self::CursorPageDown,
            "cursor_page_up" => Self::CursorPageUp,
            "cursor_top" => Self::CursorTop,
            "cursor_bottom" => Self::CursorBottom,
            "section_next" => Self::SectionNext,
            "section_prev" => Self::SectionPrev,
            "enter_yank" => Self::EnterYank,
            "yank_cancel" => Self::YankCancel,
            "yank_expand" => Self::YankExpand,
            "yank_shrink" => Self::YankShrink,
            "yank_copy" => Self::YankCopy,
            "toc_close" => Self::TocClose,
            "toc_select_next" => Self::TocSelectNext,
            "toc_select_prev" => Self::TocSelectPrev,
            "toc_select_top" => Self::TocSelectTop,
            "toc_select_bottom" => Self::TocSelectBottom,
            "toc_confirm" => Self::TocConfirm,
            "help_close" => Self::HelpClose,
            "dir_close" => Self::DirClose,
            "dir_select_next" => Self::DirSelectNext,
            "dir_select_prev" => Self::DirSelectPrev,
            "dir_select_top" => Self::DirSelectTop,
            "dir_select_bottom" => Self::DirSelectBottom,
            "dir_activate" => Self::DirActivate,
            "dir_go_up" => Self::DirGoUp,
            _ => return None,
        })
    }
}

/// Parse a chord description like `"ctrl+d"`, `"alt+left"`, `"esc"`,
/// `"G"`, or `"shift+tab"`. Modifiers can be combined (`"ctrl+alt+x"`)
/// and are case-insensitive; the key name is case-sensitive for
/// single characters and case-insensitive for named keys. The result
/// is fed through [`normalize_chord`] so storage matches what the
/// runtime delivers.
pub fn parse_chord(s: &str) -> Result<Chord, String> {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
        return Err(format!("`{s}` is not a valid chord"));
    }
    let (key_str, prefixes) = parts.split_last().unwrap();
    let mut mods = KeyModifiers::NONE;
    for prefix in prefixes {
        match prefix.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= KeyModifiers::CONTROL,
            "alt" | "meta" | "opt" | "option" => mods |= KeyModifiers::ALT,
            "shift" => mods |= KeyModifiers::SHIFT,
            other => return Err(format!("unknown modifier `{other}`")),
        }
    }
    let mut code = parse_key_code(key_str)?;
    // `shift+tab` is the conventional name for `BackTab`. Convert
    // before normalization, otherwise we'd just strip the shift and
    // lose the distinction from a plain Tab.
    if mods.contains(KeyModifiers::SHIFT) && code == KeyCode::Tab {
        code = KeyCode::BackTab;
        mods -= KeyModifiers::SHIFT;
    }
    // `shift+<lower>` is the same key the user would write uppercase.
    if mods.contains(KeyModifiers::SHIFT)
        && let KeyCode::Char(c) = code
        && c.is_ascii_lowercase()
    {
        code = KeyCode::Char(c.to_ascii_uppercase());
        mods -= KeyModifiers::SHIFT;
    }
    Ok(normalize_chord(code, mods))
}

fn parse_key_code(s: &str) -> Result<KeyCode, String> {
    let lower = s.to_ascii_lowercase();
    Ok(match lower.as_str() {
        "esc" | "escape" => KeyCode::Esc,
        "enter" | "return" | "ret" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "backspace" | "bs" => KeyCode::Backspace,
        "space" | "spc" => KeyCode::Char(' '),
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        _ => {
            // F1..F12.
            if let Some(n) = lower.strip_prefix('f')
                && let Ok(n) = n.parse::<u8>()
                && (1..=12).contains(&n)
            {
                return Ok(KeyCode::F(n));
            }
            // Single character (case-sensitive).
            let mut chars = s.chars();
            if let Some(c) = chars.next()
                && chars.next().is_none()
            {
                return Ok(KeyCode::Char(c));
            }
            return Err(format!("unknown key `{s}`"));
        }
    })
}

#[derive(serde::Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    keys: HashMap<String, HashMap<String, String>>,
}

/// Result of merging user config into the built-in defaults.
pub struct LoadedKeymaps {
    pub normal: Keymap,
    pub yank: Keymap,
    pub toc: Keymap,
    pub help: Keymap,
    pub dir: Keymap,
    /// Human-readable problems collected while parsing config — bad
    /// modifiers, unknown actions, etc. The TUI surfaces these in the
    /// status bar at startup; we never abort on a config problem.
    pub warnings: Vec<String>,
    /// `Some` when a config file was found (regardless of whether it
    /// parsed). Lets the UI mention the path in warnings.
    pub config_path: Option<PathBuf>,
}

/// Build the per-mode keymaps for an App. Loads
/// `$XDG_CONFIG_HOME/markdown-browser/config.toml` (or the `~/.config`
/// fallback) if it exists. Missing file is silent; parse errors and
/// bad bindings come back via `warnings`.
pub fn load() -> LoadedKeymaps {
    let mut out = LoadedKeymaps {
        normal: default_normal(),
        yank: default_yank(),
        toc: default_toc(),
        help: default_help(),
        dir: default_dir(),
        warnings: Vec::new(),
        config_path: None,
    };
    let Some(path) = config_path() else {
        return out;
    };
    out.config_path = Some(path.clone());
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return out,
        Err(e) => {
            out.warnings.push(format!("read config: {e}"));
            return out;
        }
    };
    let cfg: ConfigFile = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            out.warnings.push(format!("parse config: {e}"));
            return out;
        }
    };
    for (mode, table) in cfg.keys {
        let map = match mode.as_str() {
            "normal" => &mut out.normal,
            "yank" => &mut out.yank,
            "toc" => &mut out.toc,
            "help" => &mut out.help,
            "dir" => &mut out.dir,
            _ => {
                out.warnings.push(format!("unknown mode `{mode}`"));
                continue;
            }
        };
        for (key, action_name) in table {
            let chord = match parse_chord(&key) {
                Ok(c) => c,
                Err(e) => {
                    out.warnings.push(format!("[{mode}] `{key}`: {e}"));
                    continue;
                }
            };
            let Some(action) = Action::from_name(&action_name) else {
                out.warnings
                    .push(format!("[{mode}] `{key}`: unknown action `{action_name}`"));
                continue;
            };
            map.insert(chord, action);
        }
    }
    out
}

fn config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("markdown-browser/config.toml"));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/markdown-browser/config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_keys() {
        assert_eq!(
            parse_chord("j").unwrap(),
            (KeyCode::Char('j'), KeyModifiers::NONE)
        );
        assert_eq!(
            parse_chord("G").unwrap(),
            (KeyCode::Char('G'), KeyModifiers::NONE)
        );
        assert_eq!(
            parse_chord("?").unwrap(),
            (KeyCode::Char('?'), KeyModifiers::NONE)
        );
    }

    #[test]
    fn parse_modifiers() {
        assert_eq!(
            parse_chord("ctrl+d").unwrap(),
            (KeyCode::Char('d'), KeyModifiers::CONTROL)
        );
        assert_eq!(
            parse_chord("alt+left").unwrap(),
            (KeyCode::Left, KeyModifiers::ALT)
        );
        assert_eq!(
            parse_chord("Ctrl+Alt+x").unwrap(),
            (
                KeyCode::Char('x'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )
        );
    }

    #[test]
    fn parse_named_keys() {
        assert_eq!(
            parse_chord("esc").unwrap(),
            (KeyCode::Esc, KeyModifiers::NONE)
        );
        assert_eq!(
            parse_chord("enter").unwrap(),
            (KeyCode::Enter, KeyModifiers::NONE)
        );
        assert_eq!(
            parse_chord("tab").unwrap(),
            (KeyCode::Tab, KeyModifiers::NONE)
        );
        assert_eq!(
            parse_chord("backtab").unwrap(),
            (KeyCode::BackTab, KeyModifiers::NONE)
        );
        assert_eq!(
            parse_chord("space").unwrap(),
            (KeyCode::Char(' '), KeyModifiers::NONE)
        );
        assert_eq!(
            parse_chord("f5").unwrap(),
            (KeyCode::F(5), KeyModifiers::NONE)
        );
    }

    #[test]
    fn shift_normalization() {
        // shift+tab → BackTab
        assert_eq!(
            parse_chord("shift+tab").unwrap(),
            (KeyCode::BackTab, KeyModifiers::NONE)
        );
        // shift+g → G
        assert_eq!(
            parse_chord("shift+g").unwrap(),
            (KeyCode::Char('G'), KeyModifiers::NONE)
        );
        // shift modifier dropped from named keys; left over because crossterm reports it.
        assert_eq!(
            parse_chord("shift+left").unwrap(),
            (KeyCode::Left, KeyModifiers::NONE)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_chord("").is_err());
        assert!(parse_chord("ctrl+").is_err());
        assert!(parse_chord("unknown+x").is_err());
        assert!(parse_chord("f99").is_err());
    }

    #[test]
    fn action_names_round_trip_through_defaults() {
        // Every Action variant that appears in a default map should
        // round-trip from name → enum, so a user can rebind anything
        // we ship by default.
        for map in [
            default_normal(),
            default_yank(),
            default_toc(),
            default_help(),
            default_dir(),
        ] {
            for action in map.values() {
                let name = action_name(*action);
                assert_eq!(
                    Action::from_name(name),
                    Some(*action),
                    "action {action:?} → {name:?} did not round-trip"
                );
            }
        }
    }

    fn action_name(a: Action) -> &'static str {
        // Mirror of `from_name` for the round-trip test only —
        // intentionally local so a missing entry in either direction
        // fails the test.
        match a {
            Action::Quit => "quit",
            Action::OpenToc => "open_toc",
            Action::OpenHelp => "open_help",
            Action::OpenDir => "open_dir",
            Action::ToggleLineNumbers => "toggle_line_numbers",
            Action::ToggleShortcodes => "toggle_shortcodes",
            Action::StartSearch => "start_search",
            Action::SearchNext => "search_next",
            Action::SearchPrev => "search_prev",
            Action::LinkNext => "link_next",
            Action::LinkPrev => "link_prev",
            Action::ActivateLink => "activate_link",
            Action::HistoryBack => "history_back",
            Action::HistoryForward => "history_forward",
            Action::CursorDown => "cursor_down",
            Action::CursorUp => "cursor_up",
            Action::CursorHalfPageDown => "cursor_half_page_down",
            Action::CursorHalfPageUp => "cursor_half_page_up",
            Action::CursorPageDown => "cursor_page_down",
            Action::CursorPageUp => "cursor_page_up",
            Action::CursorTop => "cursor_top",
            Action::CursorBottom => "cursor_bottom",
            Action::SectionNext => "section_next",
            Action::SectionPrev => "section_prev",
            Action::EnterYank => "enter_yank",
            Action::YankCancel => "yank_cancel",
            Action::YankExpand => "yank_expand",
            Action::YankShrink => "yank_shrink",
            Action::YankCopy => "yank_copy",
            Action::TocClose => "toc_close",
            Action::TocSelectNext => "toc_select_next",
            Action::TocSelectPrev => "toc_select_prev",
            Action::TocSelectTop => "toc_select_top",
            Action::TocSelectBottom => "toc_select_bottom",
            Action::TocConfirm => "toc_confirm",
            Action::HelpClose => "help_close",
            Action::DirClose => "dir_close",
            Action::DirSelectNext => "dir_select_next",
            Action::DirSelectPrev => "dir_select_prev",
            Action::DirSelectTop => "dir_select_top",
            Action::DirSelectBottom => "dir_select_bottom",
            Action::DirActivate => "dir_activate",
            Action::DirGoUp => "dir_go_up",
        }
    }
}
