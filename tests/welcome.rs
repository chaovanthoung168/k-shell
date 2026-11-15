use kshell::{
    commands::OutputFormat,
    interactive::completion::complete,
    presentation::{Appearance, Palette, welcome},
    runtime::{ShellAction, ShellState},
};
use std::process::Command;
mod common;
use common::TempDir;

#[test]
fn kshell_prints_the_shared_banner_and_keeps_the_current_session() {
    let temp = TempDir::new();
    let appearance = Appearance {
        palette: Palette::Dusk,
        ..Default::default()
    };
    let mut state = ShellState::new(temp.path().into()).with_appearance(appearance);
    for (width, color) in [
        (None, false),
        (None, true),
        (Some(80), true),
        (Some(30), false),
    ] {
        let mut output = Vec::new();
        let outcome = state
            .execute_with_format(
                "kshell",
                &mut output,
                OutputFormat {
                    terminal_width: width,
                    color,
                    appearance,
                },
            )
            .unwrap();
        let mut expected = Vec::new();
        welcome::write(&mut expected, color && width.is_some(), width.unwrap_or(80)).unwrap();
        assert_eq!(output, expected);
        assert_eq!(outcome.action, ShellAction::Continue);
        assert_eq!(outcome.status, 0);
        assert_eq!(state.cwd(), temp.path());
        assert_eq!(state.appearance(), appearance);
    }
    let mut output = Vec::new();
    state.execute("kshell --welcome", &mut output).unwrap();
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Welcome to K Shell")
    );
    let mut output = Vec::new();
    state.execute("echo still-here", &mut output).unwrap();
    assert_eq!(output, b"still-here\n");
}

#[test]
fn kshell_needs_no_path_lookup_and_batch_output_is_plain() {
    let result = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .env("PATH", "")
        .args(["-c", "kshell"])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    assert!(!result.stdout.contains(&0x1b));
    let mut expected = Vec::new();
    welcome::write(&mut expected, false, 80).unwrap();
    assert_eq!(result.stdout, expected);
}

#[test]
fn welcome_command_has_help_and_completion_and_rejects_unsupported_arguments() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().into());
    let mut output = Vec::new();
    state.execute("help kshell", &mut output).unwrap();
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Show the K Shell welcome banner again.")
    );
    assert!(
        complete("ks", 2, temp.path(), &[])
            .1
            .iter()
            .any(|choice| choice.replacement == "kshell ")
    );
    assert!(complete("kshell other", 12, temp.path(), &[]).1.is_empty());
    for command in [
        "kshell extra",
        "kshell --theme ocean",
        "kshell --welcome extra",
    ] {
        let mut output = Vec::new();
        assert_eq!(state.execute(command, &mut output).unwrap_err().status, 2);
        assert!(output.is_empty());
    }
}
