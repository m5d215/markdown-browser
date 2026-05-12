//! Document anchors — one per heading. Drive the TOC overlay and resolve
//! `#slug` link targets.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub level: u8,
    pub text: String,
    pub slug: String,
    /// Index into the rendered `Vec<StyledLine>` where the heading starts.
    pub line: usize,
}

/// Generate a GitHub-flavoured slug. Lowercases ASCII, drops most
/// punctuation, replaces whitespace with `-`, and keeps non-ASCII alphanumeric
/// characters (so Japanese headings produce a usable anchor too).
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_dash = true; // suppress leading dashes
    for c in text.chars() {
        let mapped = if c.is_ascii_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else if c == '-' || c == '_' || c.is_whitespace() {
            Some('-')
        } else if c.is_alphanumeric() {
            Some(c)
        } else {
            None
        };
        if let Some(ch) = mapped {
            if ch == '-' {
                if !prev_dash {
                    out.push('-');
                    prev_dash = true;
                }
            } else {
                out.push(ch);
                prev_dash = false;
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}
