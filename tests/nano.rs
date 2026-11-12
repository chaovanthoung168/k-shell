#![cfg(windows)]
use std::{fs, process::Command};

use kshell::{
    interactive::completion::{complete, native_commands},
    platform,
};
mod common;
use common::TempDir;

#[test]
fn native_nano_on_path_has_precedence_and_receives_quoted_arguments_and_directory() {
    let temp = TempDir::new();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_kshell"), bin.join("nano.exe")).unwrap();
    // A native test stand-in checks argument transport without opening an editor.
    for command in [
        "nano -c 'echo \"ឯក file.txt\" \"C:\\folder with space\\\"'",
        "NANO.EXE -c 'exit 37'",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
            .current_dir(temp.path())
            .env("PATH", &bin)
            .env("PATHEXT", ".EXE;.COM")
            .args(["-c", command])
            .output()
            .unwrap();
        if command.starts_with("nano ") {
            assert!(output.status.success(), "{:?}", output.stderr);
            assert_eq!(
                output.stdout,
                "ឯក file.txt C:\\folder with space\\\n".as_bytes()
            );
        } else {
            assert_eq!(output.status.code(), Some(37));
        }
    }
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .current_dir(temp.path())
        .env("PATH", &bin)
        .args(["-c", "nano -c pwd"])
        .output()
        .unwrap();
    assert!(output.status.success());
    // Windows may present the inherited cwd using its short (8.3) spelling.
    let reported = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        std::path::Path::new(reported.trim())
            .canonicalize()
            .unwrap(),
        temp.path().canonicalize().unwrap()
    );
}

#[test]
fn installed_nano_fallback_supports_version_and_completion_without_opening_editor() {
    let temp = TempDir::new();
    let Some(editor) = platform::nano::default_editor(temp.path()) else {
        eprintln!("Git for Windows Nano is not installed; skipping installed-editor version check");
        return;
    };
    assert!(editor.is_file());
    for command in ["nano --version", "NANO.EXE --version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
            .current_dir(temp.path())
            .env("PATH", "")
            .env("PATHEXT", ".EXE;.COM")
            .args(["-c", command])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("GNU nano"));
    }
    let names = native_commands(temp.path());
    assert!(names.iter().any(|name| name == "nano"));
    assert!(names.iter().any(|name| name == "nano.exe"));
    let (_, pairs) = complete("nan", 3, temp.path(), &names);
    assert!(pairs.iter().any(|p| p.replacement == "nano "));
    fs::write(temp.path().join("ឯក notes.txt"), b"original").unwrap();
    let line = "nano 'ឯក";
    let (_, pairs) = complete(line, line.len(), temp.path(), &names);
    assert!(pairs.iter().any(|p| p.replacement.contains("ឯក notes.txt")));
    assert_eq!(
        fs::read(temp.path().join("ឯក notes.txt")).unwrap(),
        b"original"
    );
}

#[test]
fn explicit_missing_nano_paths_and_scripts_never_use_the_fallback() {
    let temp = TempDir::new();
    for (input, code) in [
        ("./missing/nano.exe --version", 127),
        ("./nano.cmd --version", 126),
        ("./nano.bat --version", 126),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
            .current_dir(temp.path())
            .args(["-c", input])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(code));
    }
}
