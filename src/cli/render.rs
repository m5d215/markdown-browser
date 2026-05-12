use std::io::{self, Read, Write};
use std::path::Path;

pub fn run(file: Option<&Path>) -> io::Result<()> {
    let input = read_input(file)?;

    // TODO: parse with comrak and render via the pure renderer layer.
    // For now we echo input verbatim so the wiring is exercisable.
    let mut stdout = io::stdout().lock();
    stdout.write_all(input.as_bytes())?;
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
