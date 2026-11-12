use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use unicode_width::UnicodeWidthStr;

use super::OutputFormat;
use crate::error::ShellError;
use crate::presentation::{Icon, Style};

const COLUMN_GAP: usize = 3;

struct Entry {
    label: String,
    width: usize,
    kind: char,
    bytes: u64,
    style: Style,
}

pub(super) fn label(name: &str, terminal: bool) -> String {
    if !terminal {
        return name.to_owned();
    }
    // A filename must never inject cursor movement or terminal commands.
    let text: String = name
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect();
    if text
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '\'' | '"'))
    {
        format!("'{}'", text.replace('\'', "'\"'\"'"))
    } else {
        text
    }
}

fn write_name(entry: &Entry, format: OutputFormat, output: &mut dyn Write) -> io::Result<()> {
    entry.style.write_in(
        output,
        &entry.label,
        format.color && format.terminal_width.is_some(),
        format.appearance.palette,
    )
}

/// Sorted, vertical columns like ls. Choose widths from visible Unicode cells,
/// never from ANSI byte lengths; long filenames are printed without truncation.
fn columns(entries: &[Entry], terminal_width: usize) -> (usize, Vec<usize>) {
    let max_columns = entries
        .len()
        .min(terminal_width.saturating_add(COLUMN_GAP) / (COLUMN_GAP + 1))
        .max(1);
    for count in (1..=max_columns).rev() {
        let rows = entries.len().div_ceil(count);
        let actual_columns = entries.len().div_ceil(rows);
        let mut widths = vec![0; actual_columns];
        for (index, entry) in entries.iter().enumerate() {
            widths[index / rows] = widths[index / rows].max(entry.width);
        }
        if actual_columns == 1
            || widths.iter().sum::<usize>() + COLUMN_GAP * (actual_columns - 1) <= terminal_width
        {
            return (rows, widths);
        }
    }
    (entries.len(), vec![0])
}

pub(super) fn render(
    paths: &[(OsString, PathBuf)],
    long: bool,
    format: OutputFormat,
    output: &mut dyn Write,
) -> Result<(), ShellError> {
    let terminal = format.terminal_width.is_some();
    let mut entries = Vec::with_capacity(paths.len());
    for (name, path) in paths {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| ShellError::new(format!("ls: {}: {error}", path.display()), 1))?;
        let kind = if metadata.is_symlink() {
            'l'
        } else if metadata.is_dir() {
            'd'
        } else {
            '-'
        };
        let name = name.display().to_string();
        let label = label(&name, terminal);
        let native_executable = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("com"));
        #[cfg(unix)]
        let native_executable = {
            use std::os::unix::fs::PermissionsExt;
            !metadata.is_symlink() && metadata.permissions().mode() & 0o111 != 0
        };
        let style = match kind {
            'd' => Style::Directory,
            'l' => Style::Link,
            _ if native_executable => Style::Executable,
            _ => Style::file(path),
        };
        let label = format
            .appearance
            .decorate(Icon::file(path, style), &label, terminal);
        entries.push(Entry {
            width: UnicodeWidthStr::width(label.as_str()),
            label,
            kind,
            bytes: metadata.len(),
            style,
        });
    }
    let result = render_entries(&entries, long, format, output);
    result.map_err(|error| ShellError::new(format!("ls: stdout: {error}"), 1))
}

fn render_entries(
    entries: &[Entry],
    long: bool,
    format: OutputFormat,
    output: &mut dyn Write,
) -> io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let color = format.color && format.terminal_width.is_some();
    if long || format.terminal_width.is_none() {
        for entry in entries {
            if long {
                Style::Muted.write_in(
                    output,
                    &format!("{} {:>10} ", entry.kind, entry.bytes),
                    color,
                    format.appearance.palette,
                )?;
            }
            write_name(entry, format, output)?;
            writeln!(output)?;
        }
        return Ok(());
    }
    let (rows, widths) = columns(entries, format.terminal_width.unwrap_or(80).max(1));
    for row in 0..rows {
        for (column, &width) in widths.iter().enumerate() {
            let index = column * rows + row;
            let Some(entry) = entries.get(index) else {
                break;
            };
            write_name(entry, format, output)?;
            if index + rows < entries.len() {
                write!(
                    output,
                    "{:padding$}",
                    "",
                    padding = width.saturating_sub(entry.width) + COLUMN_GAP
                )?;
            }
        }
        writeln!(output)?;
    }
    Ok(())
}
