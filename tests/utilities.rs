use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use kshell::presentation::{Appearance, IconMode};
use kshell::runtime::ShellState;
use kshell::{commands::OutputFormat, runtime::ShellAction};
mod common;
use common::TempDir;

#[test]
fn mkdir_parents_and_touch_preserve_existing_contents_and_update_times() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    state
        .execute("mkdir -p 'dir ឯក/nested space'", &mut Vec::new())
        .unwrap();
    assert!(temp.path().join("dir ឯក/nested space").is_dir());
    state
        .execute("mkdir -p 'dir ឯក/nested space'", &mut Vec::new())
        .unwrap();
    assert!(state.execute("mkdir 'dir ឯក'", &mut Vec::new()).is_err());
    state
        .execute("touch 'dir ឯក/nested space/new.txt'", &mut Vec::new())
        .unwrap();
    let file = temp.path().join("dir ឯក/nested space/new.txt");
    fs::write(&file, b"existing\0\xffdata").unwrap();
    let yesterday = SystemTime::now() - Duration::from_secs(86400);
    fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(yesterday)
        .unwrap();
    state
        .execute("touch 'dir ឯក/nested space/new.txt'", &mut Vec::new())
        .unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"existing\0\xffdata");
    assert!(fs::metadata(&file).unwrap().modified().unwrap() > yesterday);
}

#[test]
fn unknown_options_are_rejected_before_any_filesystem_mutation() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    for input in [
        "mkdir should-not-exist -x",
        "touch should-not-exist -x",
        "ls -z",
        "cat -n",
    ] {
        let error = state.execute(input, &mut Vec::new()).unwrap_err();
        assert_eq!(error.status, 2, "{input}");
        assert!(error.message.contains("unsupported option"), "{input}");
    }
    assert!(!temp.path().join("should-not-exist").exists());
    state
        .execute("mkdir -- -directory", &mut Vec::new())
        .unwrap();
    state.execute("touch -- -file", &mut Vec::new()).unwrap();
    assert!(temp.path().join("-directory").is_dir());
    assert!(temp.path().join("-file").is_file());
    for input in ["mkdir -p ''", "touch ''", "ls ''", "cat ''"] {
        assert!(
            state
                .execute(input, &mut Vec::new())
                .unwrap_err()
                .message
                .contains("path must not be empty")
        );
    }
}

#[test]
fn ls_sorts_entries_and_honors_all_and_long_flags() {
    let temp = TempDir::new();
    fs::write(temp.path().join("zeta"), b"hello").unwrap();
    fs::write(temp.path().join(".hidden"), b"").unwrap();
    fs::create_dir(temp.path().join("alpha ឯក")).unwrap();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let mut output = Vec::new();
    state.execute("ls", &mut output).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "alpha ឯក\nzeta\n");
    let mut output = Vec::new();
    state.execute("ls -la", &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.lines().any(|line| line.ends_with(" .")));
    assert!(output.lines().any(|line| line.ends_with(" ..")));
    assert!(output.lines().any(|line| line.ends_with(" .hidden")));
    assert!(
        output
            .lines()
            .any(|line| line.starts_with('-') && line.contains('5') && line.ends_with(" zeta"))
    );
    let mut output = Vec::new();
    state.execute("ls zeta", &mut output).unwrap();
    assert_eq!(output, b"zeta\n");
}

#[test]
fn cat_is_binary_safe_and_resolves_multiple_paths_from_shell_directory() {
    let temp = TempDir::new();
    fs::write(temp.path().join("file one"), b"hello\0\xff").unwrap();
    fs::write(temp.path().join("second"), b"world\n").unwrap();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let mut output = Vec::new();
    state.execute("cat 'file one' second", &mut output).unwrap();
    assert_eq!(output, b"hello\0\xffworld\n");
    assert_eq!(
        state
            .execute("cat missing", &mut Vec::new())
            .unwrap_err()
            .status,
        1
    );
}

#[test]
fn cat_stdin_in_command_mode_is_binary_safe_and_prompt_free() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "cat -"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"hello\0\xffworld\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"hello\0\xffworld\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_lists_current_commands_and_batch_clear_emits_no_ansi() {
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for name in kshell::commands::COMMAND_NAMES {
        assert!(help.contains(name));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "clear"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn terminal_ls_uses_vertical_columns_that_fit_the_available_width() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("alpha")).unwrap();
    for name in ["beta", "delta", "gamma.exe"] {
        fs::write(temp.path().join(name), b"").unwrap();
    }
    let mut state = ShellState::new(temp.path().to_path_buf());
    let mut output = Vec::new();
    state
        .execute_with_format(
            "ls",
            &mut output,
            OutputFormat {
                terminal_width: Some(20),
                color: false,
                appearance: Appearance {
                    icons: IconMode::Off,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert_eq!(output, b"alpha   delta\nbeta    gamma.exe\n");
    let mut output = Vec::new();
    state
        .execute_with_format(
            "ls",
            &mut output,
            OutputFormat {
                terminal_width: Some(4),
                color: true,
                appearance: Appearance {
                    icons: IconMode::Off,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.lines().count(), 4);
    assert!(output.contains("\x1b[1;38;2;129;184;234malpha\x1b[0m"));
    assert!(output.contains("gamma.exe")); // Never truncate a long name.
}

#[test]
fn ls_counts_unicode_cells_and_quotes_spaces_in_terminal_columns() {
    let temp = TempDir::new();
    for name in ["a", "界", "界界"] {
        fs::write(temp.path().join(name), b"").unwrap();
    }
    let mut state = ShellState::new(temp.path().to_path_buf());
    let mut output = Vec::new();
    state
        .execute_with_format(
            "ls",
            &mut output,
            OutputFormat {
                terminal_width: Some(13),
                color: false,
                appearance: Appearance {
                    icons: IconMode::Off,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "a   界   界界\n");
    fs::create_dir(temp.path().join("Saved Games")).unwrap();
    let mut output = Vec::new();
    state
        .execute_with_format(
            "ls",
            &mut output,
            OutputFormat {
                terminal_width: Some(80),
                color: true,
                appearance: Appearance {
                    icons: IconMode::Off,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("\x1b[1;38;2;129;184;234m'Saved Games'\x1b[0m")
    );
}

#[test]
fn long_ls_remains_detailed_and_nonterminal_output_is_always_plain() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("folder")).unwrap();
    fs::write(temp.path().join("file"), b"12345").unwrap();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let mut output = Vec::new();
    state
        .execute_with_format(
            "ls -l",
            &mut output,
            OutputFormat {
                terminal_width: Some(80),
                color: true,
                appearance: Appearance {
                    icons: IconMode::Off,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.lines().count(), 2);
    assert!(output.contains("\x1b[38;2;111;122;138m-          5 \x1b[0mfile\n"));
    assert!(output.contains("\x1b[1;38;2;129;184;234mfolder\x1b[0m"));
    let mut output = Vec::new();
    state
        .execute_with_format(
            "ls",
            &mut output,
            OutputFormat {
                terminal_width: None,
                color: true,
                appearance: Appearance {
                    icons: IconMode::Off,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert_eq!(output, b"file\nfolder\n");
}

#[test]
fn clear_requests_a_screen_action_without_writing_escape_codes_to_streams() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let mut output = Vec::new();
    let outcome = state.execute("clear", &mut output).unwrap();
    assert_eq!(outcome.action, ShellAction::ClearScreen);
    assert_eq!(outcome.status, 0);
    assert!(output.is_empty());
    assert_eq!(
        state
            .execute("clear extra", &mut output)
            .unwrap_err()
            .status,
        2
    );
    assert_eq!(
        state
            .execute("clear > file", &mut output)
            .unwrap_err()
            .status,
        2
    );
    assert!(!temp.path().join("file").exists());
}
