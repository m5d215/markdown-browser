use std::io;
use std::path::Path;

pub fn run(_file: Option<&Path>) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "TUI mode is not implemented yet. Use `markdown-browser render <file>` for now.",
    ))
}
