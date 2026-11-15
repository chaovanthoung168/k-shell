use kshell::{
    commands::{COMMAND_NAMES, OutputFormat},
    interactive::completion::complete,
    presentation::{Appearance, Palette},
    runtime::ShellState,
};
use std::{
    fs,
    io::{self, Write},
    process::Command,
};
use unicode_width::UnicodeWidthStr;
mod common;
use common::TempDir;

fn render(command: &str, width: Option<usize>, color: bool) -> String {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().into());
    let mut output = Vec::new();
    state
        .execute_with_format(
            command,
            &mut output,
            OutputFormat {
                terminal_width: width,
                color,
                appearance: Appearance {
                    palette: Palette::Midnight,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    String::from_utf8(output).unwrap()
}

fn plain(text: &str) -> String {
    let mut chars = text.chars();
    let mut result = String::new();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[test]
fn overview_is_compact_and_includes_all_builtins_with_focused_help_paths() {
    let overview = render("help", Some(80), false);
    assert!(overview.lines().count() <= 22);
    for name in COMMAND_NAMES {
        assert!(overview.contains(name), "missing {name}");
    }
    for text in [
        "Navigate",
        "Read & write",
        "File actions",
        "Session",
        "Customize",
        "Computer",
        "Tools",
        "help <command>",
        "help files",
        "help shortcuts",
    ] {
        assert!(overview.contains(text), "{text}");
    }
    let files = render("help files", None, false);
    assert!(files.contains("Create or replace a text file."));
    assert!(!files.contains("Connect to a remote computer."));
}

#[test]
fn every_builtin_alias_has_usage_examples_and_does_not_execute_anything() {
    for name in COMMAND_NAMES {
        let output = render(&format!("help {name}"), None, false);
        assert!(output.contains("Usage"), "{name}");
        assert!(output.contains("Examples"), "{name}");
    }
    for (alias, original) in [
        ("copy", "cp"),
        ("move", "mv"),
        ("delete", "rm"),
        ("update", "rename"),
        ("specs", "sysinfo"),
    ] {
        assert_eq!(
            render(&format!("help {alias}"), None, false),
            render(&format!("help {original}"), None, false)
        );
    }
    for name in ["git", "ssh", "curl", "nano", "code", "npm", "npx"] {
        assert!(render(&format!("help {name}"), None, false).contains("Examples"));
    }
    let shortcuts = render("help shortcuts", None, false);
    for key in [
        "Ctrl+L",
        "Ctrl+C",
        "Ctrl+D",
        "Tab",
        "Right arrow",
        "Up / Down",
    ] {
        assert!(shortcuts.contains(key));
    }
}

#[test]
fn help_wraps_at_narrow_widths_and_terminal_color_never_changes_visible_text() {
    for width in [40, 60, 80] {
        for topic in [
            "help",
            "help all",
            "help theme",
            "help files",
            "help shortcuts",
            "help help",
            "help curl",
        ] {
            let text = render(topic, Some(width), false);
            for line in text.lines() {
                assert!(line.width() <= width, "{topic} ({width}): {line}");
            }
            let colored = render(topic, Some(width), true);
            assert!(colored.contains('\x1b'));
            assert_eq!(plain(&colored), text);
        }
    }
    assert!(!render("help", None, true).contains('\x1b'));
    let batch = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "help ls"])
        .output()
        .unwrap();
    assert!(batch.status.success());
    assert!(!batch.stdout.contains(&0x1b));
    assert!(batch.stderr.is_empty());
}

#[test]
fn completion_offers_topics_instead_of_files_and_errors_leave_output_empty() {
    let temp = TempDir::new();
    fs::write(temp.path().join("ls-unrelated-file"), "keep").unwrap();
    for (input, expected) in [
        ("help l", "ls "),
        ("help fil", "files "),
        ("help co", "copy "),
        ("help short", "shortcuts "),
    ] {
        let (_, choices) = complete(input, input.len(), temp.path(), &[]);
        assert!(choices.iter().any(|choice| choice.replacement == expected));
        assert!(
            choices
                .iter()
                .all(|choice| !choice.replacement.contains("unrelated"))
        );
    }
    assert!(complete("help ls ", 8, temp.path(), &[]).1.is_empty());
    for command in ["help unknown-topic", "help ls extra"] {
        let mut state = ShellState::new(temp.path().into());
        let mut output = Vec::new();
        assert_eq!(state.execute(command, &mut output).unwrap_err().status, 2);
        assert!(output.is_empty());
    }
    assert_eq!(
        fs::read_to_string(temp.path().join("ls-unrelated-file")).unwrap(),
        "keep"
    );
}

#[test]
fn help_propagates_output_errors() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().into());
    assert_eq!(state.execute("help", &mut Broken).unwrap_err().status, 1);
}
