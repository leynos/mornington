//! `Mornington` application entry point.

/// Application entry point.
fn main() -> std::io::Result<()> {
    use std::io::Write;

    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "Hello from Mornington!")
}
