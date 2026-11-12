use std::io::Write;

use super::OutputFormat;
use crate::{error::ShellError, presentation::Style};

enum Request {
    Show(Option<usize>),
    Help,
}

fn parse(args: &[String]) -> Result<Request, ShellError> {
    match args {
        [] => Ok(Request::Show(None)),
        [option] if matches!(option.as_str(), "--help" | "-h") => Ok(Request::Help),
        [count] if !count.is_empty() && count.bytes().all(|c| c.is_ascii_digit()) => count
            .parse()
            .map(|count| Request::Show(Some(count)))
            .map_err(|_| ShellError::new("history: count is too large", 2)),
        _ => Err(ShellError::new(
            "usage: history [count] | history --help",
            2,
        )),
    }
}

pub(super) fn valid_syntax(args: &[String]) -> bool {
    parse(args).is_ok()
}

pub(super) fn run(
    args: &[String],
    entries: &[String],
    output: &mut dyn Write,
    format: OutputFormat,
) -> Result<(), ShellError> {
    let write_error = |e| ShellError::new(format!("history: {e}"), 1);
    let count = match parse(args)? {
        Request::Show(count) => count,
        Request::Help => return writeln!(output, "usage: history [count]\nShow numbered command history, oldest first. A count shows only the last entries.\nUses the interactive editor's saved/session history; leading-space entries are omitted.\nCommand mode has no interactive history. This command only lists entries.").map_err(write_error),
    };
    let start = entries.len().saturating_sub(count.unwrap_or(entries.len()));
    for (index, entry) in entries.iter().enumerate().skip(start) {
        Style::Muted
            .write_in(
                output,
                &format!("{:>5}  ", index + 1),
                format.color && format.terminal_width.is_some(),
                format.appearance.palette,
            )
            .map_err(write_error)?;
        // History is display data, never executable input. A saved command may
        // contain controls; keep each entry on one safe, readable terminal row.
        let mut display = String::new();
        for c in entry.chars() {
            if c.is_control() {
                display.extend(c.escape_default());
            } else {
                display.push(c);
            }
        }
        writeln!(output, "{display}").map_err(write_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn listing_limits_keep_original_numbers_quotes_unicode_and_safe_controls() {
        let entries = vec![
            "echo first".into(),
            "cd 'ឯក folder'".into(),
            "echo \"two  spaces\"\x1b[2J\nlast".into(),
        ];
        let mut out = Vec::new();
        run(&["2".into()], &entries, &mut out, OutputFormat::default()).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "    2  cd 'ឯក folder'\n    3  echo \"two  spaces\"\\u{1b}[2J\\nlast\n"
        );
        for args in [vec!["0".into()], vec![]] {
            let mut out = Vec::new();
            run(&args, &[], &mut out, OutputFormat::default()).unwrap();
            assert!(out.is_empty());
        }
        let mut out = Vec::new();
        run(&["0".into()], &entries, &mut out, OutputFormat::default()).unwrap();
        assert!(out.is_empty());
        run(
            &[usize::MAX.to_string()],
            &entries,
            &mut out,
            OutputFormat::default(),
        )
        .unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .starts_with("    1  echo first\n")
        );
    }

    #[test]
    fn usage_is_validated_before_output_and_writer_errors_propagate() {
        for args in [
            vec!["-c".into()],
            vec!["-1".into()],
            vec!["+2".into()],
            vec!["".into()],
            vec!["1".into(), "2".into()],
            vec!["99999999999999999999999999999999".into()],
        ] {
            let mut out = Vec::new();
            assert_eq!(
                run(
                    &args,
                    &["echo safe".into()],
                    &mut out,
                    OutputFormat::default()
                )
                .unwrap_err()
                .status,
                2
            );
            assert!(out.is_empty());
        }
        struct Failing;
        impl Write for Failing {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            run(
                &[],
                &["echo safe".into()],
                &mut Failing,
                OutputFormat::default()
            )
            .unwrap_err()
            .status,
            1
        );
        let mut out = Vec::new();
        run(&["--help".into()], &[], &mut out, OutputFormat::default()).unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .contains("usage: history [count]")
        );
    }
}
