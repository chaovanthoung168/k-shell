//! Explicit text writing, independent of shell redirection syntax.
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use crate::{
    error::ShellError,
    platform::{self, filesystem as native},
};

struct Request<'a> {
    path: &'a str,
    text: &'a [String],
}

// Only the filename position has option handling. Everything after it is text,
// including empty strings, flags, and a literal `--`.
fn parse<'a>(command: &str, args: &'a [String]) -> Result<Request<'a>, ShellError> {
    let (args, literal) = if args.first().is_some_and(|s| s == "--") {
        (&args[1..], true)
    } else {
        (args, false)
    };
    let [path, text @ ..] = args else {
        return Err(usage(command));
    };
    if text.is_empty() || (!literal && path.starts_with('-')) {
        return Err(usage(command));
    }
    if path.is_empty() || path.ends_with(['/', '\\']) {
        return Err(ShellError::new(
            format!("{command}: expected a filename"),
            2,
        ));
    }
    native::validate_file_operand(Path::new(path))
        .map_err(|e| failure(command, Path::new(path), e))?;
    Ok(Request { path, text })
}

fn usage(command: &str) -> ShellError {
    ShellError::new(
        format!("usage: {command} [--] file text ... (quote text containing spaces)"),
        2,
    )
}

fn failure(command: &str, path: &Path, error: impl std::fmt::Display) -> ShellError {
    ShellError::new(
        format!("{command}: {}: {error}", platform::display_path(path)),
        1,
    )
}

pub(super) fn valid_syntax(command: &str, args: &[String]) -> bool {
    parse(command, args).is_ok()
}

pub(super) fn run(command: &str, args: &[String], cwd: &Path) -> Result<(), ShellError> {
    let request = parse(command, args)?;
    let target = super::files::resolve(command, cwd, request.path)?;
    match fs::symlink_metadata(&target) {
        Ok(m) if !m.is_file() || native::is_link_or_reparse_point(&m) => {
            return Err(failure(
                command,
                &target,
                "expected a regular file, not a directory, link or reparse point",
            ));
        }
        Ok(_) => (),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(failure(command, &target, e)),
    }
    let append = command == "append";
    // The platform helper checks the opened handle before any truncation. The
    // original file survives a failed open or a final-entry link substitution.
    let mut file =
        native::open_text_target(&target, append).map_err(|e| failure(command, &target, e))?;
    if !append {
        file.set_len(0).map_err(|e| failure(command, &target, e))?;
    }
    let mut text = request.text.join(" ");
    text.push('\n');
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| failure(command, &target, e))
}
