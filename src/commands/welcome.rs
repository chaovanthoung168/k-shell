use std::io::Write;

use super::OutputFormat;
use crate::{error::ShellError, presentation::welcome};

pub(super) fn valid_syntax(args: &[String]) -> bool {
    args.is_empty() || matches!(args, [flag] if flag == "--welcome")
}

/// An explicit welcome request uses the supplied stream, never launches another
/// shell, and stays plain unless the interactive host supplies terminal color.
pub(super) fn run(
    args: &[String],
    output: &mut dyn Write,
    format: OutputFormat,
) -> Result<(), ShellError> {
    if !valid_syntax(args) {
        return Err(ShellError::new(
            "usage: kshell [--welcome] (show the welcome banner); use help kshell for details",
            2,
        ));
    }
    welcome::write(
        output,
        format.color && format.terminal_width.is_some(),
        format.terminal_width.unwrap_or(80),
    )
    .map_err(|error| ShellError::new(format!("kshell: {error}"), 1))
}
