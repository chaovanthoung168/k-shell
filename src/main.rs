use std::io::{self, BufRead, IsTerminal, Write};

use kshell::{
    error::ShellError,
    interactive,
    presentation::{Appearance, IconMode, Palette, SuggestionStyle},
    runtime::{ShellAction, ShellState},
};

fn run() -> Result<i32, ShellError> {
    let mut args = std::env::args_os().skip(1);
    let mut command = None;
    let (mut palette, mut icons, mut suggestions) = (None, None, None);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--theme") => {
                palette = Some(
                    args.next()
                        .as_deref()
                        .and_then(|s| s.to_str())
                        .and_then(Palette::parse)
                        .ok_or_else(|| {
                            ShellError::new("usage: kshell --theme ocean|dusk|midnight", 2)
                        })?,
                )
            }
            Some("--icons") => {
                icons = Some(
                    args.next()
                        .as_deref()
                        .and_then(|s| s.to_str())
                        .and_then(IconMode::parse)
                        .ok_or_else(|| {
                            ShellError::new("usage: kshell --icons unicode|nerd|off", 2)
                        })?,
                )
            }
            Some("--suggestions") => {
                suggestions = Some(
                    args.next()
                        .as_deref()
                        .and_then(|s| s.to_str())
                        .and_then(SuggestionStyle::parse)
                        .ok_or_else(|| {
                            ShellError::new("usage: kshell --suggestions soft|bold|underline", 2)
                        })?,
                )
            }
            Some("-c") => {
                command = Some(
                    args.next()
                        .ok_or_else(|| ShellError::new("usage: kshell -c <command>", 2))?
                        .into_string()
                        .map_err(|_| ShellError::new("command text must be valid Unicode", 2))?,
                );
                if args.next().is_some() {
                    return Err(ShellError::new(
                        "usage: kshell [appearance options] -c <command>",
                        2,
                    ));
                }
                break;
            }
            Some("--help" | "-h") => {
                writeln!(
                io::stdout(),
                "K Shell\nUsage: kshell [--theme ocean|dusk|midnight] [--icons unicode|nerd|off] [--suggestions soft|bold|underline] [-c <command>]\nRun help inside K Shell to list commands and keyboard shortcuts."
            )
            .map_err(|error| ShellError::new(format!("stdout: {error}"), 1))?;
                return Ok(0);
            }
            _ => {
                return Err(ShellError::new(
                    "usage: kshell [appearance options] [-c <command>]",
                    2,
                ));
            }
        }
    }
    let cwd = std::env::current_dir()
        .map_err(|error| ShellError::new(format!("current directory: {error}"), 1))?;
    let input = io::stdin();
    let mut output = io::stdout();
    let interactive = command.is_none() && input.is_terminal() && output.is_terminal();
    let mut appearance = if interactive {
        Appearance::from_environment()
    } else {
        Appearance::default()
    };
    if let Some(palette) = palette {
        appearance.palette = palette;
    }
    if let Some(icons) = icons {
        appearance.icons = icons;
    }
    if let Some(suggestions) = suggestions {
        appearance.suggestions = suggestions;
    }
    let mut state = ShellState::new(cwd).with_appearance(appearance);
    if let Some(command) = command {
        return state
            .execute(&command, &mut output)
            .map(|outcome| outcome.status);
    }

    if interactive {
        return interactive::run(&mut state);
    }
    let mut input = input.lock();
    let mut line = String::new();
    loop {
        line.clear();
        if input
            .read_line(&mut line)
            .map_err(|error| ShellError::new(format!("stdin: {error}"), 1))?
            == 0
        {
            break;
        }
        // Remove only the line terminator, preserving trailing argument spaces.
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        match state.execute(&line, &mut output) {
            Ok(outcome) if outcome.action == ShellAction::Exit => return Ok(outcome.status),
            Ok(_) => {}
            Err(error) => {
                let _ = writeln!(io::stderr(), "kshell: {error}");
            }
        }
    }
    Ok(state.last_status())
}

fn main() {
    let status = match run() {
        Ok(status) => status,
        Err(error) => {
            let _ = writeln!(io::stderr(), "kshell: {error}");
            error.status
        }
    };
    std::process::exit(status);
}
