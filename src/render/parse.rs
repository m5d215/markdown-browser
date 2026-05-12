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

pub fn parse<'a>(arena: &'a Arena<'a>, input: &'a str) -> &'a AstNode<'a> {
    let opts = options();
    parse_document(arena, input, &opts)
}
