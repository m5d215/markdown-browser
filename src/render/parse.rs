//! Markdown parsing configuration shared by every front-end (CLI render,
//! TUI). Centralized so extension flags don't drift across sinks.

use comrak::nodes::AstNode;
use comrak::options::Extension;
use comrak::{Arena, Options, parse_document};

pub fn options() -> Options<'static> {
    Options {
        extension: Extension {
            strikethrough: true,
            table: true,
            tasklist: true,
            autolink: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Detect a YAML (`---`) or TOML (`+++`) front matter delimiter on the
/// first line of `input` and pick the matching delimiter for comrak so the
/// block lands in the AST as `NodeValue::FrontMatter` instead of being
/// parsed as content.
pub fn options_for(input: &str) -> Options<'static> {
    let mut opts = options();
    let first = input.lines().next().unwrap_or("").trim_end_matches('\r');
    if first == "+++" {
        opts.extension.front_matter_delimiter = Some("+++".into());
    } else if first == "---" {
        opts.extension.front_matter_delimiter = Some("---".into());
    }
    opts
}

pub fn parse<'a>(arena: &'a Arena<'a>, input: &'a str) -> &'a AstNode<'a> {
    let opts = options_for(input);
    parse_document(arena, input, &opts)
}
