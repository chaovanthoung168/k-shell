use kshell::{interactive::completion::complete, runtime::ShellState};
use std::{fs, process::Command};
mod common;
use common::TempDir;

#[test]
fn command_mode_does_not_load_or_modify_a_history_file() {
    let temp = TempDir::new();
    let history = temp.path().join("history.txt");
    fs::write(&history, b"echo saved-private-fixture\n").unwrap();
    for input in ["history", "history 5", "history 0"] {
        let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
            .args(["-c", input])
            .env("KSHELL_HISTORY_FILE", &history)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output.stderr);
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read(&history).unwrap(), b"echo saved-private-fixture\n");
    }
}

#[test]
fn dispatch_help_usage_and_completion_do_not_offer_file_arguments() {
    let temp = TempDir::new();
    fs::write(temp.path().join("example.txt"), b"example").unwrap();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let mut out = Vec::new();
    state.execute("history --help", &mut out).unwrap();
    assert!(
        String::from_utf8(out)
            .unwrap()
            .contains("usage: history [count]")
    );
    for input in ["history -c", "history -1", "history 2 extra"] {
        let mut out = Vec::new();
        assert_eq!(state.execute(input, &mut out).unwrap_err().status, 2);
        assert!(out.is_empty());
    }
    assert!(
        complete("hist", 4, temp.path(), &[])
            .1
            .iter()
            .any(|p| p.replacement == "history ")
    );
    for input in ["history ", "history ex"] {
        assert!(complete(input, input.len(), temp.path(), &[]).1.is_empty());
    }
}
