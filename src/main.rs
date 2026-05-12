use std::process::ExitCode;

use clap::Parser;
use markdown_browser::cli::{self, Cli};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("markdown-browser: {err}");
            ExitCode::FAILURE
        }
    }
}
