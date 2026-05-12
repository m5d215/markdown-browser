use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub mod ansi;
pub mod net;
pub mod render;
pub mod source;
pub mod tui;

#[derive(Parser, Debug)]
#[command(name = "markdown-browser", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Markdown file or `http(s)://` URL to open. Use "-" or omit to
    /// read from stdin.
    pub file: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Render a markdown file as styled text to stdout.
    Render {
        /// Markdown file or `http(s)://` URL to render. Use "-" or omit
        /// to read from stdin.
        file: Option<PathBuf>,

        /// Force ANSI color output even when stdout is not a TTY.
        #[arg(long)]
        color: bool,

        /// Disable ANSI color output even when stdout is a TTY.
        #[arg(long, conflicts_with = "color")]
        no_color: bool,
    },
}

pub fn run(cli: Cli) -> std::io::Result<()> {
    match cli.command {
        Some(Command::Render {
            file,
            color,
            no_color,
        }) => render::run(
            file.as_deref(),
            render::ColorChoice::resolve(color, no_color),
        ),
        None => tui::run(cli.file.as_deref()),
    }
}
