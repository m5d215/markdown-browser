use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub mod render;
pub mod tui;

#[derive(Parser, Debug)]
#[command(name = "markdown-browser", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Markdown file to open. Use "-" or omit to read from stdin.
    pub file: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Render a markdown file as styled text to stdout.
    Render {
        /// Markdown file to render. Use "-" or omit to read from stdin.
        file: Option<PathBuf>,
    },
}

pub fn run(cli: Cli) -> std::io::Result<()> {
    match cli.command {
        Some(Command::Render { file }) => render::run(file.as_deref()),
        None => tui::run(cli.file.as_deref()),
    }
}
