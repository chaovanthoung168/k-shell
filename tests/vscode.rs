#![cfg(windows)]

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kshell::platform::vscode::CodeLauncher;

mod common;
use common::TempDir;

fn installation(parent: &Path, name: &str, version: Option<&str>) -> (PathBuf, PathBuf) {
    let root = parent.join(name);
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::write(root.join("Code.exe"), b"test fixture, not launched").unwrap();
    let relative = match version {
        Some(version) => format!(r"{version}\resources\app\out\cli.js"),
        None => r"resources\app\out\cli.js".into(),
    };
    let cli = root.join(&relative);
    fs::create_dir_all(cli.parent().unwrap()).unwrap();
    fs::write(&cli, b"fixture").unwrap();
    fs::write(
        root.join("bin/code.cmd"),
        format!("@echo off\r\n\"%~dp0..\\Code.exe\" \"%~dp0..\\{relative}\" %*\r\nexit /b 99\r\n"),
    )
    .unwrap();
    (root, cli)
}

#[test]
fn discovers_legacy_and_versioned_vscode_cli_layouts() {
    let temp = TempDir::new();
    for (name, version) in [("classic", None), ("versioned", Some("c4453f8ea4"))] {
        let (root, cli) = installation(temp.path(), name, version);
        let command = CodeLauncher::from_installation(&root)
            .unwrap()
            .command(&[], temp.path());
        assert_eq!(command.get_program(), root.join("Code.exe"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![cli.as_os_str()]
        );
    }
}

#[test]
fn uses_the_active_version_from_the_wrapper_even_if_an_old_cli_remains() {
    let temp = TempDir::new();
    let (root, active) = installation(temp.path(), "VS Code", Some("active-version"));
    let old = root.join("resources/app/out/cli.js");
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::write(old, b"old version").unwrap();
    let command = CodeLauncher::from_installation(&root)
        .unwrap()
        .command(&[], temp.path());
    assert_eq!(command.get_args().next(), Some(active.as_os_str()));
}

#[test]
fn discovery_respects_path_order_before_common_installation_locations() {
    let temp = TempDir::new();
    let (first, _) = installation(temp.path(), "first", None);
    let (second, _) = installation(temp.path(), "second", None);
    let paths = [first.join("bin"), second.join("bin")];
    let launcher = CodeLauncher::find_in(&paths, std::slice::from_ref(&second)).unwrap();
    assert_eq!(
        launcher.command(&[], temp.path()).get_program(),
        first.join("Code.exe")
    );
    assert!(CodeLauncher::find_in(&[], &[second]).is_some());
}

#[test]
fn native_command_preserves_cwd_arguments_and_limits_environment_changes() {
    let temp = TempDir::new();
    let (root, cli) = installation(temp.path(), "VS Code ឯក", None);
    let args: Vec<OsString> = [
        ".",
        "--new-window",
        "folder ឯក & spaces",
        "\"quoted\"",
        "--goto",
        "file.rs:12:3",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    let command = CodeLauncher::from_installation(&root)
        .unwrap()
        .command(&args, temp.path());
    let expected: Vec<&OsStr> = std::iter::once(cli.as_os_str())
        .chain(args.iter().map(OsString::as_os_str))
        .collect();
    assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
    assert_eq!(command.get_current_dir(), Some(temp.path()));
    let changes: Vec<_> = command.get_envs().collect();
    assert!(changes.contains(&(OsStr::new("ELECTRON_RUN_AS_NODE"), Some(OsStr::new("1")))));
    assert!(changes.contains(&(OsStr::new("VSCODE_DEV"), None)));
    assert_eq!(changes.len(), 2);
}

#[test]
fn incomplete_installations_and_paths_outside_the_installation_are_not_used() {
    let temp = TempDir::new();
    assert!(CodeLauncher::from_installation(temp.path()).is_none());
    let (root, cli) = installation(temp.path(), "broken", Some("version"));
    fs::remove_file(&cli).unwrap();
    assert!(CodeLauncher::from_installation(&root).is_none());
    fs::write(&cli, b"fixture").unwrap();
    fs::write(
        root.join("bin/code.cmd"),
        r#""%~dp0..\..\outside\resources\app\out\cli.js""#,
    )
    .unwrap();
    assert!(CodeLauncher::from_installation(&root).is_none());
}

#[test]
fn shell_dispatches_code_to_a_native_cli_and_never_executes_the_batch_wrapper() {
    let temp = TempDir::new();
    let (root, _) = installation(temp.path(), "VS Code fixture", Some("version"));
    // The copied Rust test harness is a native CLI stand-in, not a desktop app.
    // cli.js becomes a nonmatching test filter, and --list returns successfully.
    fs::copy(std::env::current_exe().unwrap(), root.join("Code.exe")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "code --list"])
        .env("PATH", root.join("bin"))
        .env("PATHEXT", ".CMD;.EXE")
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("0 tests")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "code.cmd --list"])
        .env("PATH", root.join("bin"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(126));
}
