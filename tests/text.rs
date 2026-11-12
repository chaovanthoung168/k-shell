use kshell::runtime::ShellState;
use std::{fs, process::Command};
mod common;
use common::TempDir;

fn run(state: &mut ShellState, command: &str) {
    let mut output = Vec::new();
    assert_eq!(state.execute(command, &mut output).unwrap().status, 0);
    assert!(output.is_empty());
}

#[test]
fn write_and_append_create_replace_and_read_back_utf8_text() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    run(&mut state, "touch notes.txt");
    run(&mut state, "write notes.txt 'Hello world'");
    run(&mut state, "append notes.txt 'សួស្តី 世界'");
    let mut output = Vec::new();
    state.execute("cat notes.txt", &mut output).unwrap();
    assert_eq!(output, "Hello world\nសួស្តី 世界\n".as_bytes());
    run(&mut state, "write notes.txt short");
    assert_eq!(fs::read(temp.path().join("notes.txt")).unwrap(), b"short\n");
    run(&mut state, "append new.txt 'created by append'");
    assert_eq!(
        fs::read(temp.path().join("new.txt")).unwrap(),
        b"created by append\n"
    );
    assert_eq!(state.cwd(), temp.path());
}

#[test]
fn text_arguments_preserve_quotes_empty_words_flags_and_windows_backslashes() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    run(
        &mut state,
        "write 'ឯក file.txt' 'first  word' '' -n -- 'C:\\path\\file' 'a>b' '$HOME'",
    );
    assert_eq!(
        fs::read(temp.path().join("ឯក file.txt")).unwrap(),
        b"first  word  -n -- C:\\path\\file a>b $HOME\n"
    );
    run(&mut state, "write -- -name.txt ''");
    run(&mut state, "append -- -name.txt \"it's fine\"");
    assert_eq!(
        fs::read(temp.path().join("-name.txt")).unwrap(),
        b"\nit's fine\n"
    );
    run(&mut state, "write json.txt '{\"message\":\"hello\"}'");
    assert_eq!(
        fs::read(temp.path().join("json.txt")).unwrap(),
        b"{\"message\":\"hello\"}\n"
    );
}

#[test]
fn append_preserves_existing_bytes_without_inventing_a_leading_newline() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::write(temp.path().join("notes.txt"), b"previous text").unwrap();
    run(&mut state, "append notes.txt ' continued'");
    assert_eq!(
        fs::read(temp.path().join("notes.txt")).unwrap(),
        b"previous text continued\n"
    );
}

#[test]
fn invalid_syntax_and_targets_leave_existing_text_and_folders_unchanged() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    fs::write(temp.path().join("keep.txt"), b"keep original").unwrap();
    fs::create_dir(temp.path().join("folder")).unwrap();
    for line in [
        "write",
        "append",
        "write keep.txt",
        "append keep.txt",
        "write keep.txt 'unterminated",
        "write keep.txt changed | cat",
        "append keep.txt changed > other.txt",
        "write '' text",
        "write folder text",
        "append folder text",
        "write folder/ text",
        "write missing/file.txt text",
        "append -bad text",
        "write -- -missing-text",
    ] {
        assert!(state.execute(line, &mut Vec::new()).is_err(), "{line}");
        assert_eq!(
            fs::read(temp.path().join("keep.txt")).unwrap(),
            b"keep original"
        );
        assert!(temp.path().join("folder").is_dir());
        assert!(!temp.path().join("other.txt").exists());
        assert!(!temp.path().join("missing").exists());
    }
    #[cfg(windows)]
    for target in ["NUL", "NUL.txt", "file:stream", "C:relative", "trailing."] {
        for command in ["write", "append"] {
            assert!(
                state
                    .execute(&format!("{command} '{target}' content"), &mut Vec::new())
                    .is_err()
            );
        }
    }
}

#[test]
fn command_mode_writes_relative_to_child_directory_and_preserves_statuses() {
    let temp = TempDir::new();
    for (line, success) in [
        ("write 'cli file.txt' 'Hello from -c'", true),
        ("append 'cli file.txt' next", true),
        ("write 'cli file.txt'", false),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
            .current_dir(temp.path())
            .args(["-c", line])
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    }
    assert_eq!(
        fs::read(temp.path().join("cli file.txt")).unwrap(),
        b"Hello from -c\nnext\n"
    );
}

#[cfg(windows)]
#[test]
fn failed_write_open_does_not_truncate_a_readonly_file() {
    let temp = TempDir::new();
    let path = temp.path().join("keep.txt");
    fs::write(&path, b"keep original").unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let write = state.execute("write keep.txt replacement", &mut Vec::new());
    let append = state.execute("append keep.txt extra", &mut Vec::new());
    fs::set_permissions(&path, original).unwrap();
    assert!(write.is_err());
    assert!(append.is_err());
    assert_eq!(fs::read(&path).unwrap(), b"keep original");
}
