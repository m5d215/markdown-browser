use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub mod ansi;
pub mod keymap;
pub mod net;
pub mod preview;
pub mod render;
pub mod source;
pub mod tui;

#[derive(Parser, Debug)]
#[command(name = "markdown-browser", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Markdown file, directory (opens README + the directory browser),
    /// or `http(s)://` URL to open. Use "-" or omit to read from stdin.
    pub file: Option<PathBuf>,

    /// Disable GitHub-style emoji shortcodes (`:rocket:` stays as text).
    /// Inside the TUI, press `e` to toggle at runtime.
    #[arg(long, global = true)]
    pub no_emoji: bool,

    /// Disable the embedded mermaid preview server.
    #[arg(long)]
    pub no_mermaid: bool,

    /// Bind the mermaid preview server to this port. Defaults to an
    /// OS-assigned ephemeral port.
    #[arg(long, value_name = "PORT")]
    pub mermaid_port: Option<u16>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Render a markdown file as styled text to stdout.
    Render {
        /// Markdown file, directory (renders the README inside), or
        /// `http(s)://` URL. Use "-" or omit to read from stdin.
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
    let emoji = !cli.no_emoji;
    match cli.command {
        Some(Command::Render {
            file,
            color,
            no_color,
        }) => render::run(
            file.as_deref(),
            render::ColorChoice::resolve(color, no_color),
            emoji,
        ),
        None => tui::run(
            cli.file.as_deref(),
            emoji,
            tui::PreviewConfig {
                enabled: !cli.no_mermaid,
                port: cli.mermaid_port,
            },
        ),
    }
}
