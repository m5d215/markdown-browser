use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;

use comrak::{Arena, Options, parse_document};
use comrak::options::Extension;

use crate::cli::ansi;
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
    let mut opts = Options::default();
    opts.extension = Extension {
        strikethrough: true,
        table: true,
        tasklist: true,
        autolink: true,
        ..Default::default()
    };
    let root = parse_document(&arena, &input, &opts);

    let lines = render::render_document(root);
    let mut stdout = io::stdout().lock();
    ansi::write_lines(&mut stdout, &lines, color.use_color())?;
    stdout.flush()?;
    Ok(())
}

fn read_input(file: Option<&Path>) -> io::Result<String> {
    match file {
        Some(path) if path.as_os_str() != "-" => std::fs::read_to_string(path),
        _ => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}
