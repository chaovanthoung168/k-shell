use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{OutputFormat, listing};
use crate::{
    error::ShellError,
    platform::filesystem::is_hidden,
    runtime::{Outcome, ShellAction},
};

fn io_error(command: &str, path: &Path, error: io::Error) -> ShellError {
    ShellError::new(format!("{command}: {}: {error}", path.display()), 1)
}

// Validate every option before doing any filesystem writes.
pub(super) fn options<'a>(
    args: &'a [String],
    supported: &str,
    command: &str,
) -> Result<(Vec<char>, Vec<&'a str>), ShellError> {
    let mut flags = Vec::new();
    let mut operands = Vec::new();
    let mut parsing = true;
    for arg in args {
        if parsing && arg == "--" {
            parsing = false;
            continue;
        }
        if parsing && arg.starts_with('-') && arg != "-" {
            for flag in arg[1..].chars() {
                if !supported.contains(flag) {
                    return Err(ShellError::new(
                        format!(
                            "{command}: unsupported option {arg:?}; use help for supported flags"
                        ),
                        2,
                    ));
                }
                flags.push(flag);
            }
        } else if arg.is_empty() {
            return Err(ShellError::new(
                format!("{command}: path must not be empty"),
                1,
            ));
        } else {
            operands.push(arg.as_str());
        }
    }
    Ok((flags, operands))
}

fn ls(
    args: &[String],
    cwd: &Path,
    output: &mut dyn Write,
    format: OutputFormat,
) -> Result<(), ShellError> {
    let (flags, mut operands) = options(args, "al", "ls")?;
    if operands.is_empty() {
        operands.push(".");
    }
    for (index, operand) in operands.iter().enumerate() {
        let path = cwd.join(operand);
        let metadata = fs::symlink_metadata(&path).map_err(|error| io_error("ls", &path, error))?;
        if !metadata.is_dir() {
            listing::render(
                &[(OsStr::new(operand).to_os_string(), path)],
                flags.contains(&'l'),
                format,
                output,
            )?;
            continue;
        }
        if operands.len() > 1 {
            if index > 0 {
                writeln!(output).map_err(|error| io_error("ls", &path, error))?;
            }
            writeln!(output, "{operand}:").map_err(|error| io_error("ls", &path, error))?;
        }
        let mut entries: Vec<(std::ffi::OsString, PathBuf)> = Vec::new();
        if flags.contains(&'a') {
            entries.push((".".into(), path.clone()));
            entries.push(("..".into(), path.join("..")));
        }
        for entry in fs::read_dir(&path).map_err(|error| io_error("ls", &path, error))? {
            let entry = entry.map_err(|error| io_error("ls", &path, error))?;
            let name = entry.file_name();
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| io_error("ls", &entry.path(), error))?;
            if flags.contains(&'a') || !is_hidden(&name, &metadata) {
                entries.push((name, entry.path()));
            }
        }
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        listing::render(&entries, flags.contains(&'l'), format, output)?;
    }
    Ok(())
}

pub fn run(
    args: &[String],
    cwd: &Path,
    output: &mut dyn Write,
    format: OutputFormat,
) -> Result<Option<Outcome>, ShellError> {
    match args[0].as_str() {
        "ls" => ls(&args[1..], cwd, output, format)?,
        "tree" => super::tree::run(&args[1..], cwd, output, format)?,
        "mkdir" => {
            let (flags, operands) = options(&args[1..], "p", "mkdir")?;
            if operands.is_empty() {
                return Err(ShellError::new("usage: mkdir [-p] directory ...", 2));
            }
            for operand in operands {
                let path = cwd.join(operand);
                let result = if flags.contains(&'p') {
                    fs::create_dir_all(&path)
                } else {
                    fs::create_dir(&path)
                };
                result.map_err(|error| io_error("mkdir", &path, error))?;
            }
        }
        "touch" => {
            let (_, operands) = options(&args[1..], "", "touch")?;
            if operands.is_empty() {
                return Err(ShellError::new("usage: touch file ...", 2));
            }
            for operand in operands {
                let path = cwd.join(operand);
                let metadata = fs::metadata(&path);
                let file = if metadata.as_ref().is_ok_and(|metadata| metadata.is_dir()) {
                    // Opening directories requires platform-specific flags on Windows;
                    // refuse rather than silently claiming to update their times.
                    return Err(ShellError::new(
                        "touch: directory timestamps are not supported yet",
                        1,
                    ));
                } else {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                        .map_err(|error| io_error("touch", &path, error))?
                };
                let now = SystemTime::now();
                file.set_times(fs::FileTimes::new().set_accessed(now).set_modified(now))
                    .map_err(|error| io_error("touch", &path, error))?;
            }
        }
        "cat" => {
            let (_, mut operands) = options(&args[1..], "", "cat")?;
            if operands.is_empty() {
                operands.push("-");
            }
            for operand in operands {
                if operand == "-" {
                    io::copy(&mut io::stdin().lock(), output)
                        .map_err(|error| io_error("cat", Path::new("stdin"), error))?;
                } else {
                    let path = cwd.join(operand);
                    let mut file =
                        File::open(&path).map_err(|error| io_error("cat", &path, error))?;
                    io::copy(&mut file, output).map_err(|error| io_error("cat", &path, error))?;
                }
            }
        }
        "clear" => {
            if args.len() != 1 {
                return Err(ShellError::new("usage: clear", 2));
            }
            return Ok(Some(Outcome {
                status: 0,
                action: ShellAction::ClearScreen,
            }));
        }
        _ => return Ok(None),
    }
    Ok(Some(Outcome {
        status: 0,
        action: ShellAction::Continue,
    }))
}
