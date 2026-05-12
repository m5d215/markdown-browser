//! Visual theme — maps semantic roles to concrete styles.
//!
//! Centralized so we can swap palettes (dark/light) without crawling the
//! renderer. Defaults assume a dark terminal background.

use crate::render::style::{Color, Style};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub heading: [Style; 6],
    pub paragraph: Style,
    pub blockquote: Style,
    pub blockquote_marker: Style,
    pub list_marker: Style,
    pub task_marker_done: Style,
    pub task_marker_todo: Style,
    pub code_inline: Style,
    pub code_block: Style,
    pub code_fence: Style,
    pub link_text: Style,
    pub link_url: Style,
    pub emph: Style,
    pub strong: Style,
    pub strikethrough: Style,
    pub thematic_break: Style,
    pub image_alt: Style,
}

impl Theme {
    pub const fn dark() -> Self {
        Self {
            heading: [
                Style::new().fg(Color::BrightCyan).bold(),
                Style::new().fg(Color::BrightMagenta).bold(),
                Style::new().fg(Color::BrightYellow).bold(),
                Style::new().fg(Color::BrightGreen).bold(),
                Style::new().fg(Color::BrightBlue).bold(),
                Style::new().fg(Color::BrightBlack).bold(),
            ],
            paragraph: Style::new(),
            blockquote: Style::new().dim(),
            blockquote_marker: Style::new().fg(Color::BrightBlack),
            list_marker: Style::new().fg(Color::BrightYellow),
            task_marker_done: Style::new().fg(Color::BrightGreen),
            task_marker_todo: Style::new().fg(Color::BrightBlack),
            code_inline: Style::new().fg(Color::Yellow),
            code_block: Style::new().fg(Color::BrightWhite),
            code_fence: Style::new().fg(Color::BrightBlack),
            link_text: Style::new().fg(Color::BrightBlue).underline(),
            link_url: Style::new().fg(Color::Blue).dim(),
            emph: Style::new().italic(),
            strong: Style::new().bold(),
            strikethrough: Style::new().strikethrough().dim(),
            thematic_break: Style::new().fg(Color::BrightBlack),
            image_alt: Style::new().fg(Color::Magenta),
        }
    }

    pub fn heading(&self, level: u32) -> Style {
        let idx = (level.saturating_sub(1).min(5)) as usize;
        self.heading[idx]
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
