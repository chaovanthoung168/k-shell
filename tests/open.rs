use std::fs;
use std::path::Path;
use std::process::Command;

use kshell::interactive::completion::{ShellHelper, complete};
use kshell::language::parser::parse;
use kshell::platform::open::OpenRequest;
use kshell::runtime::ShellState;
use rustyline::{Context, completion::Completer, history::DefaultHistory};

mod common;
use common::TempDir;

#[test]
fn media_requests_resolve_spaces_unicode_and_absolute_paths_after_cd() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("media ឯក")).unwrap();
    let mut state = ShellState::new(temp.path().to_path_buf());
    state.execute("cd 'media ឯក'", &mut Vec::new()).unwrap();
    for name in ["song with spaces.mp3", "movie ឯក.mp4"] {
        let expected = state.cwd().join(name);
        fs::write(
            &expected,
            b"fixture; request preparation does not play this file",
        )
        .unwrap();
        for target in [Path::new(name), expected.as_path()] {
            let request = OpenRequest::new(target, state.cwd()).unwrap();
            assert_eq!(
                request.file().canonicalize().unwrap(),
                expected.canonicalize().unwrap()
            );
            assert_eq!(request.directory().canonicalize().unwrap(), state.cwd());
            assert!(request.file().is_absolute());
            #[cfg(windows)]
            assert!(
                !request
                    .file()
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .starts_with(r"\\?\")
            );
        }
    }
}

#[test]
fn open_errors_explain_missing_files_folders_and_quoting_without_launching_apps() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    let mut output = Vec::new();
    for input in [
        "open",
        "open song with spaces.mp3",
        "open --unknown",
        "open ''",
    ] {
        let error = state.execute(input, &mut output).unwrap_err();
        assert_eq!(error.status, 2, "{input}");
    }
    let error = state.execute("open missing.mp3", &mut output).unwrap_err();
    assert_eq!(error.status, 1);
    assert!(error.message.starts_with("open: missing.mp3:"));
    assert_eq!(state.last_status(), 1);
    let error = state.execute("open .", &mut output).unwrap_err();
    assert!(error.message.contains("expected a file"));
    assert!(output.is_empty());
}

#[test]
fn open_does_not_bypass_native_program_and_batch_script_policies() {
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_path_buf());
    for filename in [
        "script.cmd",
        "script.BAT",
        "program.exe",
        "program.COM",
        "-script.cmd",
    ] {
        fs::write(temp.path().join(filename), b"must never execute").unwrap();
        let error = state
            .execute(&format!("open -- {filename}"), &mut Vec::new())
            .unwrap_err();
        assert_eq!(error.status, 126, "{filename}");
    }
}

#[test]
fn open_completes_media_filenames_with_spaces_and_command_names() {
    let temp = TempDir::new();
    fs::write(temp.path().join("song with spaces.mp3"), b"").unwrap();
    fs::write(temp.path().join("movie ឯក.mp4"), b"").unwrap();
    let helper = ShellHelper::new(temp.path().to_path_buf());
    let history = DefaultHistory::new();
    for (line, expected) in [
        ("open 'song", "song with spaces.mp3"),
        ("open mov", "movie ឯក.mp4"),
    ] {
        let (start, candidates) = helper
            .complete(line, line.len(), &Context::new(&history))
            .unwrap();
        let completed = format!("{}{}", &line[..start], candidates[0].replacement);
        let command = parse(&completed).unwrap().unwrap();
        assert_eq!(command.words[0].literal(), "open");
        assert_eq!(command.words[1].literal(), expected);
    }
    assert_eq!(
        complete("ope", 3, temp.path(), &[]).1[0].replacement,
        "open "
    );
}

#[test]
fn command_mode_open_reports_diagnostics_on_stderr_and_nonzero_status() {
    let temp = TempDir::new();
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "open missing.mp4"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("open: missing.mp4:")
    );
}
