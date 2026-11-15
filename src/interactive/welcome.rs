use std::io::{self, Write};

/// Upright, spaced ASCII lettering. One white foreground and standard glyphs
/// keep the welcome legible without fonts, executables or extra dependencies.
pub const LOGO: &[&str] = &[
    " _  __   ____  _   _ _____ _     _     ",
    "| |/ /  / ___|| | | | ____| |   | |    ",
    "| ' /   \\___ \\| |_| |  _| | |   | |    ",
    "| . \\    ___) |  _  | |___| |___| |___ ",
    "|_|\\_\\  |____/|_| |_|_____|_____|_____|",
];

pub fn write(output: &mut dyn Write, color: bool, width: usize) -> io::Result<()> {
    writeln!(output)?;
    if color {
        write!(output, "\x1b[97m")?;
    }
    if width >= 42 {
        for line in LOGO {
            writeln!(output, "{line}")?;
        }
        writeln!(output)?;
    }
    writeln!(output, "Welcome to K Shell")?;
    writeln!(
        output,
        "Type help to begin. Type theme to choose your prompt.\n"
    )?;
    if color {
        write!(output, "\x1b[0m")?;
    }
    Ok(())
}
