use std::io::{self, Write};

/// Slanted outline lettering inspired by the supplied terminal banner. ASCII
/// only: no font download, extra executable or dependency is required.
pub const LOGO: &[&str] = &[
    "    __ __    _____ __         ____",
    "   / //_/   / ___// /_  ___  / / /",
    "  / ,<      \\__ \\/ __ \\/ _ \\/ / /",
    " / /| |    ___/ / / / /  __/ / /",
    "/_/ |_|   /____/_/ /_/\\___/_/_/",
];

pub fn write(output: &mut dyn Write, color: bool, width: usize) -> io::Result<()> {
    writeln!(output)?;
    if width >= 42 {
        // Horizontal red -> yellow -> green -> blue -> purple gradient.
        let colors = [
            "224;92;84",
            "224;187;64",
            "93;194;98",
            "81;157;212",
            "173;107;215",
        ];
        for line in LOGO {
            for (column, character) in line.chars().enumerate() {
                if color {
                    write!(
                        output,
                        "\x1b[38;2;{}m{character}",
                        colors[(column / 8).min(4)]
                    )?;
                } else {
                    write!(output, "{character}")?;
                }
            }
            if color {
                write!(output, "\x1b[0m")?;
            }
            writeln!(output)?;
        }
    }
    writeln!(output, "Welcome to K Shell")?;
    writeln!(
        output,
        "Type help to begin. Type theme to choose your prompt.\n"
    )
}
