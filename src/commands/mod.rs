use std::io::Write;
use std::path::PathBuf;

use crate::{
    error::ShellError,
    platform,
    runtime::{Outcome, ShellAction, ShellState},
};

mod files;
mod history;
mod listing;
mod system;
mod text;
mod theme;
mod tree;
mod utilities;

/// Plain by default. Only the interactive host supplies terminal capabilities;
/// utilities never guess whether a provided memory/file stream is a console.
#[derive(Debug, Clone, Copy, Default)]
pub struct OutputFormat {
    pub terminal_width: Option<usize>,
    pub color: bool,
    pub appearance: crate::presentation::Appearance,
}

pub const COMMAND_NAMES: &[&str] = &[
    "append", "cat", "cd", "clear", "copy", "cp", "create", "delete", "echo", "exit", "help",
    "history", "ls", "mkdir", "move", "mv", "open", "pwd", "rename", "rm", "specs", "sysinfo",
    "theme", "touch", "tree", "update", "write",
];

/// Preview argument syntax only. Uses utility/tree parsers without performing
/// filesystem actions, launching apps or mutating the shell during suggestions.
pub(crate) fn valid_preview_syntax(args: &[String]) -> Option<bool> {
    let operands = &args[1..];
    Some(match args[0].as_str() {
        "echo" => true,
        "pwd" | "clear" | "help" => operands.is_empty(),
        "cd" => {
            operands.len() <= 1
                && operands
                    .first()
                    .is_none_or(|arg| !arg.is_empty() && !arg.starts_with('-'))
        }
        "exit" => operands.is_empty() || (operands.len() == 1 && operands[0].parse::<u8>().is_ok()),
        "sysinfo" | "specs" => {
            operands.is_empty()
                || (operands.len() == 1 && matches!(operands[0].as_str(), "-h" | "--help"))
        }
        "open" => match operands {
            [path] => !path.is_empty() && !path.starts_with('-'),
            [marker, path] => marker == "--" && !path.is_empty(),
            _ => false,
        },
        "ls" | "mkdir" | "touch" | "cat" => {
            let flags = match args[0].as_str() {
                "ls" => "al",
                "mkdir" => "p",
                _ => "",
            };
            utilities::options(operands, flags, &args[0]).is_ok_and(|(_, paths)| {
                !matches!(args[0].as_str(), "mkdir" | "touch") || !paths.is_empty()
            })
        }
        "tree" => tree::valid_syntax(operands),
        "history" => history::valid_syntax(operands),
        "write" | "append" => text::valid_syntax(&args[0], operands),
        "create" | "rm" | "delete" | "cp" | "copy" | "mv" | "move" | "rename" | "update" => {
            files::valid_syntax(&args[0], operands)
        }
        "theme" => match operands {
            [] => true,
            [value] => {
                matches!(value.as_str(), "reset" | "-h" | "--help")
                    || crate::presentation::Palette::parse(value).is_some()
            }
            [kind, mode] => match kind.as_str() {
                "icons" => crate::presentation::IconMode::parse(mode).is_some(),
                "suggestions" => crate::presentation::SuggestionStyle::parse(mode).is_some(),
                _ => false,
            },
            _ => false,
        },
        _ => return None,
    })
}

pub fn run(
    args: &[String],
    state: &mut ShellState,
    output: &mut dyn Write,
    format: OutputFormat,
) -> Result<Option<Outcome>, ShellError> {
    let mut outcome = Outcome {
        status: 0,
        action: ShellAction::Continue,
    };
    match args[0].as_str() {
        "theme" => theme::run(&args[1..], state, output)?,
        "history" => history::run(&args[1..], &state.history, output, format)?,
        "sysinfo" | "specs" => system::run(args, output, format)?,
        "write" | "append" => text::run(&args[0], &args[1..], &state.cwd)?,
        "create" | "rm" | "delete" | "cp" | "copy" | "mv" | "move" | "rename" | "update" => {
            files::run(&args[0], &args[1..], &state.cwd)?;
        }
        "echo" => {
            writeln!(output, "{}", args[1..].join(" "))
                .map_err(|error| ShellError::new(format!("echo: {error}"), 1))?;
        }
        "pwd" => {
            if args.len() != 1 {
                return Err(ShellError::new("usage: pwd", 2));
            }
            writeln!(output, "{}", platform::display_path(&state.cwd))
                .map_err(|error| ShellError::new(format!("pwd: {error}"), 1))?;
        }
        "cd" => {
            if args.len() > 2 || args.get(1).is_some_and(|arg| arg.starts_with('-')) {
                return Err(ShellError::new(
                    "usage: cd [directory] (flags are not supported)",
                    2,
                ));
            }
            let target = match args.get(1) {
                Some(arg) if arg.is_empty() => {
                    return Err(ShellError::new("cd: directory must not be empty", 1));
                }
                Some(arg) => PathBuf::from(arg),
                None => PathBuf::from(
                    platform::home_directory()
                        .ok_or_else(|| ShellError::new("cd: home directory is not set", 1))?,
                ),
            };
            let target = state
                .cwd
                .join(target)
                .canonicalize()
                .map_err(|error| ShellError::new(format!("cd: {error}"), 1))?;
            if !target.is_dir() {
                return Err(ShellError::new("cd: target is not a directory", 1));
            }
            state.cwd = target;
        }
        "exit" => {
            if args.len() > 2 {
                return Err(ShellError::new("usage: exit [0-255]", 2));
            }
            outcome.status = match args.get(1) {
                Some(arg) => i32::from(
                    arg.parse::<u8>()
                        .map_err(|_| ShellError::new("exit: expected a status from 0 to 255", 2))?,
                ),
                None => state.last_status,
            };
            outcome.action = ShellAction::Exit;
        }
        "open" => {
            let target = match &args[1..] {
                [path] if !path.starts_with('-') => path,
                [marker, path] if marker == "--" => path,
                _ => {
                    return Err(ShellError::new(
                        "usage: open [--] <file> (quote paths containing spaces)",
                        2,
                    ));
                }
            };
            platform::open::OpenRequest::new(std::path::Path::new(target), &state.cwd)?.launch()?;
        }
        "help" => {
            if args.len() != 1 {
                return Err(ShellError::new("usage: help", 2));
            }
            writeln!(
                output,
                concat!(
                    "K Shell\nCommands: {}\n",
                    "ls [-al] [path ...] (colored columns in a terminal; -l for details)\n",
                    "tree [-ad] [-L depth] [directory]: show folder structure\n",
                    "mkdir [-p] directory ...\ntouch file ...\ncat [file ...]\n",
                    "create [--] file ...: create new empty files; never overwrite\n",
                    "write [--] file text ...: create or replace UTF-8 text (adds a newline)\n",
                    "append [--] file text ...: append UTF-8 text (adds a newline)\n",
                    "nano file: edit in GNU nano (when installed); Ctrl+O, Enter saves; Ctrl+X exits\n",
                    "cp / copy [-r] source destination: copy without overwriting\n",
                    "mv / move source destination: move without overwriting\n",
                    "rename / update source new-name: rename in the same folder\n",
                    "rm / delete [-rf] path ...: permanently delete (-r for folders; -f ignores missing)\n",
                    "sysinfo (alias: specs): show OS, current user, CPU, GPU, RAM and local drive space\n",
                    "ssh [options] user@host: connect using native OpenSSH Client\n",
                    "curl [options] URL: make requests/download files using native curl\n",
                    "ssh -p 2222 user@host: connect on a custom port; exit leaves the server\n",
                    "open [--] file: open media/documents with the Windows default app\n",
                    "code .: open this folder in VS Code (when installed)\n",
                    "npm run dev: run a package script using Node.js/npm (when installed)\n",
                    "npx [arguments]: launch npm's package runner (when installed)\n",
                    "clear: clear the terminal screen\n",
                    "history [count]: list saved/session commands, optionally only the last count\n",
                    "theme [ocean|dusk|midnight]: choose colors; theme icons unicode|nerd|off\n",
                    "theme suggestions soft|bold|underline: style suggestion previews\n",
                    "Tab: completion; Right: accept suggestion; Up/Down: history\n",
                    "Pipes, redirection, and variable expansion are not implemented yet."
                ),
                COMMAND_NAMES.join(", ")
            )
            .map_err(|error| ShellError::new(format!("help: {error}"), 1))?;
        }
        _ => return utilities::run(args, &state.cwd, output, format),
    }
    Ok(Some(outcome))
}
