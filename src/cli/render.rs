use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;

use comrak::Arena;

use crate::cli::ansi;
use crate::cli::net;
use crate::cli::source;
use crate::render;

#[derive(Debug, Clone, Copy)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    pub fn resolve(color: bool, no_color: bool) -> Self {
        if color {
            Self::Always
        } else if no_color {
            Self::Never
        } else {
            Self::Auto
        }
    }

    fn use_color(self) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => io::stdout().is_terminal(),
        }
    }
}

pub fn run(file: Option<&Path>, color: ColorChoice) -> io::Result<()> {
    let input = read_input(file)?;

    let arena = Arena::new();
    let root = render::parse::parse(&arena, &input);
    let doc = render::render_document(root);
    let mut stdout = io::stdout().lock();
    ansi::write_lines(&mut stdout, &doc.lines, color.use_color())?;
    stdout.flush()?;
    Ok(())
}

fn read_input(file: Option<&Path>) -> io::Result<String> {
    match file {
        Some(path) if path.as_os_str() != "-" => {
            let as_str = path.to_string_lossy();
            if source::is_url(&as_str) {
                net::fetch(&as_str).map_err(io::Error::other)
            } else {
                std::fs::read_to_string(path)
            }
        }
        _ => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}
