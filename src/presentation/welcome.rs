use std::io::{self, Write};

/// Shared outline lettering matched to the supplied reference. Color is confined to
/// the logo; normal welcome text uses the terminal's foreground.
pub const LOGO: &[&str] = &[
    " _  __   ____  _   _ _____ _     _     ",
    "| |/ /  / ___|| | | | ____| |   | |    ",
    "| ' /   \\___ \\| |_| |  _| | |   | |    ",
    "| . \\    ___) |  _  | |___| |___| |___ ",
    "|_|\\_\\  |____/|_| |_|_____|_____|_____|",
];

pub fn write(output: &mut dyn Write, color: bool, width: usize) -> io::Result<()> {
    writeln!(output)?;
    if width >= 42 {
        let colors = [
            "224;92;84",
            "224;187;64",
            "93;194;98",
            "81;157;212",
            "173;107;215",
        ];
        for line in LOGO {
            for (column, character) in line.chars().enumerate() {
                if color && column % 8 == 0 {
                    write!(
                        output,
                        "\x1b[38;2;{}m",
                        colors[(column / 8).min(colors.len() - 1)]
                    )?;
                }
                write!(output, "{character}")?;
            }
            if color {
                write!(output, "\x1b[0m")?;
            }
            writeln!(output)?;
        }
        writeln!(output)?;
    }
    writeln!(output, "Welcome to K Shell")?;
    writeln!(
        output,
        "Type help to begin. Type theme to choose your prompt.\n"
    )?;
    Ok(())
}
