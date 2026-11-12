use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{OutputFormat, listing};
use crate::{
    error::ShellError,
    platform::filesystem::{is_hidden, is_link_or_reparse_point},
    presentation::{Icon, Style},
};

const USAGE: &str = "usage: tree [-ad] [-L depth] [--] [directory]";

#[derive(Default)]
struct Options<'a> {
    all: bool,
    directories_only: bool,
    max_depth: Option<usize>,
    root: Option<&'a str>,
}

pub(super) fn valid_syntax(args: &[String]) -> bool {
    (args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h")) || parse(args).is_ok()
}

fn parse(args: &[String]) -> Result<Options<'_>, ShellError> {
    let mut options = Options::default();
    let mut parsing = true;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if parsing && arg == "--" {
            parsing = false;
            index += 1;
            continue;
        }
        if parsing && arg.starts_with('-') && arg != "-" {
            let mut flags = arg[1..].chars();
            while let Some(flag) = flags.next() {
                match flag {
                    'a' => options.all = true,
                    'd' => options.directories_only = true,
                    'L' => {
                        let attached = flags.as_str();
                        let value = if attached.is_empty() {
                            index += 1;
                            args.get(index).map(String::as_str)
                        } else {
                            Some(attached)
                        };
                        let depth = value
                            .and_then(|value| value.parse::<usize>().ok())
                            .filter(|depth| *depth > 0)
                            .ok_or_else(|| {
                                ShellError::new(
                                    "tree: -L expects a positive integer (for example, tree -L 2)",
                                    2,
                                )
                            })?;
                        options.max_depth = Some(depth);
                        break;
                    }
                    _ => {
                        return Err(ShellError::new(
                            format!("tree: unsupported option {arg:?}; {USAGE}"),
                            2,
                        ));
                    }
                }
            }
        } else {
            if arg.is_empty() {
                return Err(ShellError::new("tree: directory must not be empty", 2));
            }
            if options.root.replace(arg).is_some() {
                return Err(ShellError::new(USAGE, 2));
            }
        }
        index += 1;
    }
    Ok(options)
}

struct Entry {
    name: OsString,
    path: PathBuf,
    directory: bool,
    link: bool,
}

fn io_error(path: &Path, error: std::io::Error) -> ShellError {
    ShellError::new(format!("tree: {}: {error}", path.display()), 1)
}

fn children(path: &Path, options: &Options<'_>) -> Result<Vec<Entry>, ShellError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| io_error(path, error))? {
        let entry = entry.map_err(|error| io_error(path, error))?;
        let path = entry.path();
        let name = entry.file_name();
        let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
        let link = is_link_or_reparse_point(&metadata);
        let directory = metadata.is_dir() && !link;
        if (!options.all && is_hidden(&name, &metadata)) || (options.directories_only && !directory)
        {
            continue;
        }
        entries.push(Entry {
            name,
            path,
            directory,
            link,
        });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

fn write_name(
    output: &mut dyn Write,
    text: &str,
    style: Style,
    icon: Icon,
    format: OutputFormat,
) -> std::io::Result<()> {
    let label = listing::label(text, true);
    let label = format
        .appearance
        .decorate(icon, &label, format.terminal_width.is_some());
    style.write_in(
        output,
        &label,
        format.color && format.terminal_width.is_some(),
        format.appearance.palette,
    )
}

fn write_branch(
    output: &mut dyn Write,
    prefix: &str,
    last: bool,
    format: OutputFormat,
) -> std::io::Result<()> {
    let branch = if last { "└── " } else { "├── " };
    Style::Muted.write_in(
        output,
        &format!("{prefix}{branch}"),
        format.color && format.terminal_width.is_some(),
        format.appearance.palette,
    )
}

struct Frame {
    entries: std::vec::IntoIter<Entry>,
    prefix: String,
    depth: usize,
}

pub(super) fn run(
    args: &[String],
    cwd: &Path,
    output: &mut dyn Write,
    format: OutputFormat,
) -> Result<(), ShellError> {
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        return writeln!(output, "{USAGE}\n-a  Include hidden entries\n-d  Show real directories only\n-L  Limit displayed depth (tree -L 2)\nLinks and Windows reparse points are not traversed.")
            .map_err(|error| io_error(Path::new("stdout"), error));
    }
    let options = parse(args)?;
    let root = options.root.unwrap_or(".");
    let path = cwd.join(root);
    let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
    if is_link_or_reparse_point(&metadata) {
        return Err(ShellError::new(
            "tree: root is a link or Windows reparse point; links are not traversed",
            1,
        ));
    }
    if !metadata.is_dir() {
        return Err(ShellError::new("tree: expected a directory", 1));
    }
    let entries = children(&path, &options)?;
    let mut stack = vec![Frame {
        entries: entries.into_iter(),
        prefix: String::new(),
        depth: 1,
    }];
    let mut directories = 0usize;
    let mut files = 0usize;
    let result = (|| -> std::io::Result<()> {
        write_name(output, root, Style::Directory, Icon::Folder, format)?;
        writeln!(output)?;
        Ok(())
    })();
    result.map_err(|error| io_error(Path::new("stdout"), error))?;

    // Explicit frames avoid overflowing the Rust call stack on deep trees.
    while let Some(frame) = stack.last_mut() {
        let Some(entry) = frame.entries.next() else {
            stack.pop();
            continue;
        };
        let last = frame.entries.len() == 0;
        let prefix = frame.prefix.clone();
        let depth = frame.depth;
        let style = if entry.link {
            Style::Link
        } else if entry.directory {
            Style::Directory
        } else {
            Style::file(&entry.path)
        };
        let result = (|| -> std::io::Result<()> {
            write_branch(output, &prefix, last, format)?;
            write_name(
                output,
                &entry.name.to_string_lossy(),
                style,
                Icon::file(&entry.path, style),
                format,
            )?;
            if entry.link {
                match fs::read_link(&entry.path) {
                    Ok(target) => write!(
                        output,
                        " -> {}",
                        listing::label(&target.display().to_string(), true)
                    )?,
                    Err(_) => write!(output, " [not traversed]")?,
                }
            }
            writeln!(output)
        })();
        result.map_err(|error| io_error(Path::new("stdout"), error))?;
        if entry.directory {
            directories += 1;
            if options.max_depth.is_none_or(|limit| depth < limit) {
                let entries = children(&entry.path, &options)?;
                stack.push(Frame {
                    entries: entries.into_iter(),
                    prefix: format!("{prefix}{}", if last { "    " } else { "│   " }),
                    depth: depth + 1,
                });
            }
        } else {
            files += 1;
        }
    }
    let report = if options.directories_only {
        format!(
            "{directories} director{}",
            if directories == 1 { "y" } else { "ies" }
        )
    } else {
        format!(
            "{directories} director{}, {files} file{}",
            if directories == 1 { "y" } else { "ies" },
            if files == 1 { "" } else { "s" }
        )
    };
    let result = (|| -> std::io::Result<()> {
        writeln!(output)?;
        Style::Muted.write_in(
            output,
            &report,
            format.color && format.terminal_width.is_some(),
            format.appearance.palette,
        )?;
        writeln!(output)
    })();
    result.map_err(|error| io_error(Path::new("stdout"), error))
}
