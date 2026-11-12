use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use kshell::runtime::ShellState;

mod common;
use common::TempDir;

fn shell(command: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", command])
        .output()
        .unwrap()
}

#[test]
fn executable_help_usage_and_diagnostics_use_k_shell_branding() {
    let help = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.starts_with("K Shell\nUsage: kshell "));
    assert!(!help.contains("MyShell"));
    let usage = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .arg("--invalid-option")
        .output()
        .unwrap();
    assert_eq!(usage.status.code(), Some(2));
    assert!(
        String::from_utf8(usage.stderr)
            .unwrap()
            .starts_with("kshell: usage: kshell ")
    );
    let missing = shell("kshell-unknown-command-46731");
    assert_eq!(missing.status.code(), Some(127));
    assert!(
        String::from_utf8(missing.stderr)
            .unwrap()
            .starts_with("kshell: command not found:")
    );
    assert!(
        String::from_utf8(shell("help").stdout)
            .unwrap()
            .starts_with("K Shell\nCommands:")
    );
}

#[test]
fn startup_appearance_flags_work_in_profiles_and_remain_plain_in_command_mode() {
    let temp = TempDir::new();
    fs::write(temp.path().join("example.txt"), b"example").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args([
            "--theme",
            "midnight",
            "--icons",
            "nerd",
            "--suggestions",
            "soft",
            "-c",
            "theme",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        output.stdout,
        b"Theme: midnight\nIcons: nerd\nSuggestions: soft\n"
    );
    let listing = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .current_dir(temp.path())
        .args(["--theme", "midnight", "--icons", "nerd", "-c", "ls"])
        .output()
        .unwrap();
    assert!(listing.status.success());
    assert_eq!(listing.stdout, b"example.txt\n");
    for args in [
        vec!["--theme"],
        vec!["--theme", "missing"],
        vec!["--icons", "missing"],
        vec!["--suggestions", "missing"],
        vec!["-c", "echo wrong", "--theme", "midnight"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn builtins_write_to_the_provided_stream() {
    let mut state = ShellState::new(std::env::current_dir().unwrap());
    let mut output = Vec::new();
    assert_eq!(
        state
            .execute("echo \"hello world\" \"\" end", &mut output)
            .unwrap()
            .status,
        0
    );
    assert_eq!(output, b"hello world  end\n");
}

#[test]
fn cd_persists_and_failed_cd_preserves_directory() {
    let temp = TempDir::new();
    fs::create_dir(temp.path().join("child ឯក with spaces")).unwrap();
    let mut state = ShellState::new(temp.path().canonicalize().unwrap());
    state
        .execute("cd 'child ឯក with spaces'", &mut Vec::new())
        .unwrap();
    let expected = temp
        .path()
        .join("child ឯក with spaces")
        .canonicalize()
        .unwrap();
    assert_eq!(state.cwd(), expected);
    assert!(state.execute("cd missing", &mut Vec::new()).is_err());
    assert_eq!(state.cwd(), expected);
    let mut output = Vec::new();
    state.execute("pwd", &mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        format!("{}\n", kshell::platform::display_path(&expected))
    );
    assert!(state.execute("cd ''", &mut Vec::new()).is_err());
}

#[test]
fn rejected_syntax_does_not_dispatch_any_command() {
    let mut state = ShellState::new(std::env::current_dir().unwrap());
    let mut output = Vec::new();
    assert_eq!(
        state
            .execute("echo must-not-run > out.txt", &mut output)
            .unwrap_err()
            .status,
        2
    );
    assert!(output.is_empty());
    assert_eq!(state.last_status(), 2);
    assert_eq!(state.execute("exit", &mut output).unwrap().status, 2);
}

#[test]
fn cli_separates_diagnostics_and_output_and_reports_status() {
    let output = shell("echo 'hello world'");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"hello world\n");
    assert!(output.stderr.is_empty());
    let output = shell("kshell-this-command-does-not-exist-80934");
    assert_eq!(output.status.code(), Some(127));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("command not found")
    );
    assert_eq!(shell("echo 'unterminated").status.code(), Some(2));
    assert_eq!(shell("exit 42").status.code(), Some(42));
    assert_eq!(shell("exit 256").status.code(), Some(2));
}

#[test]
fn line_mode_keeps_state_and_never_prints_a_prompt() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"echo first\r\necho second\nexit 7\necho unreachable\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"first\nsecond\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn batch_scripts_are_never_implicitly_executed() {
    for name in ["example.cmd", "example.BAT"] {
        let output = shell(name);
        assert_eq!(output.status.code(), Some(126));
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("compatibility is not implemented")
        );
    }
}

#[test]
fn native_executable_and_arguments_with_spaces_are_preserved() {
    let temp = TempDir::new();
    let probe = temp.path().join(if cfg!(windows) {
        "native ឯក probe.exe"
    } else {
        "native ឯក probe"
    });
    fs::copy(env!("CARGO_BIN_EXE_kshell"), &probe).unwrap();
    // A second kshell process verifies what actually survives the native argument
    // boundary, including spaces, embedded quotes, and trailing backslashes.
    let input = format!(
        r#"'{}' -c 'echo "a b" "" "C:\Program Files\"'"#,
        probe.display()
    );
    let output = shell(&input);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"a b  C:\\Program Files\\\n");
    assert!(
        shell(&format!("'{}' -c ''", probe.display()))
            .status
            .success()
    );
    assert_eq!(
        shell(&format!("'{}' -c 'exit 37'", probe.display()))
            .status
            .code(),
        Some(37)
    );
}

#[test]
fn native_child_inherits_persistent_shell_directory() {
    let temp = TempDir::new();
    let expected = temp.path().canonicalize().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let script = format!(
        "cd '{}'\n'{}' -c pwd\nexit\n",
        temp.path().display(),
        env!("CARGO_BIN_EXE_kshell")
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8(output.stdout).unwrap();
    // Windows current_dir may omit the extended-length prefix returned by
    // canonicalize; compare directory identity instead of that spelling.
    assert_eq!(
        Path::new(printed.trim_end_matches('\n'))
            .canonicalize()
            .unwrap(),
        expected
    );
}

#[cfg(windows)]
#[test]
fn windows_path_and_pathext_lookup_use_native_executables() {
    let temp = TempDir::new();
    fs::copy(env!("CARGO_BIN_EXE_kshell"), temp.path().join("probe.exe")).unwrap();
    fs::write(temp.path().join("probe.cmd"), b"exit /b 99").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "probe -c 'echo native lookup'"])
        .env("PATH", temp.path())
        .env("PATHEXT", ".CMD;.EXE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"native lookup\n");
}
