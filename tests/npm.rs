#![cfg(windows)]

use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use kshell::platform::npm::{PackageLauncher, PackageTool};

mod common;
use common::TempDir;

fn installation(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    fs::create_dir_all(root.join("node_modules/npm/bin")).unwrap();
    fs::write(root.join("node.exe"), b"native stand-in, not executed").unwrap();
    for tool in ["npm", "npx"] {
        fs::write(
            root.join(format!("node_modules/npm/bin/{tool}-cli.js")),
            b"fixture",
        )
        .unwrap();
        fs::write(root.join(format!("{tool}.cmd")), b"exit /b 99\r\n").unwrap();
    }
    root
}

#[test]
fn only_bare_package_tool_names_use_the_adapter() {
    assert_eq!(PackageTool::parse("NPM"), Some(PackageTool::Npm));
    assert_eq!(PackageTool::parse("npx"), Some(PackageTool::Npx));
    for name in [
        "npm.cmd",
        "npm.ps1",
        "npm.exe",
        "./npm",
        r".\npm",
        r"C:\nodejs\npm",
        "not-npm",
    ] {
        assert_eq!(PackageTool::parse(name), None, "{name}");
    }
}

#[test]
fn discovery_respects_directory_order_and_requires_each_tools_cli() {
    let temp = TempDir::new();
    let first = installation(temp.path(), "Node first");
    let second = installation(temp.path(), "Node second");
    let directories = [first.clone(), second.clone()];
    let launcher = PackageLauncher::find_in(PackageTool::Npm, &directories, None).unwrap();
    let command = launcher.command(&[], temp.path()).unwrap();
    assert_eq!(command.get_program(), first.join("node.exe"));
    assert_eq!(
        Path::new(command.get_args().next().unwrap()),
        first.join("node_modules/npm/bin/npm-cli.js")
    );
    fs::remove_file(first.join("node_modules/npm/bin/npx-cli.js")).unwrap();
    let launcher = PackageLauncher::find_in(PackageTool::Npx, &directories, None).unwrap();
    assert_eq!(
        launcher.command(&[], temp.path()).unwrap().get_program(),
        second.join("node.exe")
    );
    fs::remove_file(first.join("node_modules/npm/bin/npm-cli.js")).unwrap();
    assert!(PackageLauncher::find_in(PackageTool::Npm, &[first], None).is_none());
}

#[test]
fn global_npm_layout_uses_native_node_fallback_and_prefers_adjacent_node() {
    let temp = TempDir::new();
    let root = installation(temp.path(), "global npm ឯក");
    let fallback = temp.path().join("fallback node.exe");
    fs::write(&fallback, b"not executed").unwrap();
    let launcher = PackageLauncher::find_in(
        PackageTool::Npm,
        std::slice::from_ref(&root),
        Some(&fallback),
    )
    .unwrap();
    assert_eq!(
        launcher.command(&[], temp.path()).unwrap().get_program(),
        root.join("node.exe")
    );
    fs::remove_file(root.join("node.exe")).unwrap();
    assert!(
        PackageLauncher::find_in(PackageTool::Npm, std::slice::from_ref(&root), None).is_none()
    );
    let launcher = PackageLauncher::find_in(
        PackageTool::Npm,
        std::slice::from_ref(&root),
        Some(&fallback),
    )
    .unwrap();
    assert_eq!(
        launcher.command(&[], temp.path()).unwrap().get_program(),
        fallback
    );
    let batch = temp.path().join("node.cmd");
    fs::write(&batch, b"not executed").unwrap();
    assert!(PackageLauncher::find_in(PackageTool::Npm, &[root], Some(&batch)).is_none());
}

#[test]
fn launcher_preserves_native_arguments_cwd_and_limits_path_changes_to_the_child() {
    let temp = TempDir::new();
    let root = installation(temp.path(), "Node tools ឯក & spaces");
    let launcher =
        PackageLauncher::find_in(PackageTool::Npm, std::slice::from_ref(&root), None).unwrap();
    let args: Vec<OsString> = [
        "run",
        "dev",
        "--",
        "a b",
        "",
        "a&b",
        "\"quotes\"",
        r"D:\my project\",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    let original_path = std::env::var_os("PATH");
    let command = launcher.command(&args, temp.path()).unwrap();
    let cli = root.join("node_modules/npm/bin/npm-cli.js");
    let mut received = command.get_args();
    assert_eq!(Path::new(received.next().unwrap()), cli);
    let expected: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
    assert_eq!(received.collect::<Vec<_>>(), expected);
    assert_eq!(command.get_current_dir(), Some(temp.path()));
    let changes: Vec<_> = command.get_envs().collect();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].0, "PATH");
    assert_eq!(
        std::env::split_paths(changes[0].1.unwrap()).next(),
        Some(root)
    );
    assert_eq!(std::env::var_os("PATH"), original_path);
}

#[test]
fn shell_dispatches_both_tools_through_native_node_and_rejects_explicit_wrappers() {
    let temp = TempDir::new();
    let root = installation(temp.path(), "Node CLI fixture");
    // The native Rust test harness accepts a nonmatching CLI-path test filter
    // and --list, so the wrapper's exit 99 can never masquerade as success.
    fs::copy(std::env::current_exe().unwrap(), root.join("node.exe")).unwrap();
    for tool in ["npm", "npx"] {
        let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
            .args(["-c", &format!("{tool} --list")])
            .env("PATH", &root)
            .env("PATHEXT", ".CMD;.EXE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("0 tests"));
        for suffix in ["cmd", "ps1"] {
            let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
                .args(["-c", &format!("{tool}.{suffix} --version")])
                .env("PATH", &root)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(126));
        }
    }
}

#[test]
fn native_npm_executable_still_wins_before_the_adapter() {
    let temp = TempDir::new();
    let root = installation(temp.path(), "Node");
    fs::copy(env!("CARGO_BIN_EXE_kshell"), root.join("npm.exe")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "npm -c 'echo native executable'"])
        .env("PATH", root)
        .env("PATHEXT", ".CMD;.EXE")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"native executable\n");
}

#[test]
fn installed_npm_runs_a_fixture_package_script_with_arguments_cwd_and_exit_status() {
    let temp = TempDir::new();
    let Ok(launcher) = PackageLauncher::find(PackageTool::Npm, temp.path()) else {
        eprintln!("Node.js/npm not installed; skipping offline npm integration test");
        return;
    };
    let node = launcher
        .command(&[], temp.path())
        .unwrap()
        .get_program()
        .to_owned();
    assert!(
        kshell::interactive::completion::native_commands(temp.path())
            .iter()
            .any(|name| name == "npm")
    );
    fs::write(temp.path().join("package.json"), r#"{"name":"kshell-offline-probe","version":"1.0.0","private":true,"scripts":{"probe":"node probe.cjs"}}"#).unwrap();
    fs::write(
        temp.path().join("probe.cjs"),
        r#"
        require('node:fs').writeFileSync('probe-cwd.txt', process.cwd());
        console.log('ARGS=' + JSON.stringify(process.argv.slice(2)));
        if (process.argv[2] === '--fail') process.exit(37);
    "#,
    )
    .unwrap();
    let userconfig = temp.path().join("user.npmrc");
    let globalconfig = temp.path().join("global.npmrc");
    fs::write(&userconfig, b"").unwrap();
    fs::write(&globalconfig, b"").unwrap();
    let shell = |input: &str| {
        Command::new(env!("CARGO_BIN_EXE_kshell"))
            .args(["-c", input])
            .current_dir(temp.path())
            .env("PATH", Path::new(&node).parent().unwrap())
            .env("PATHEXT", ".CMD;.EXE")
            .env("npm_config_cache", temp.path().join("cache"))
            .env("npm_config_userconfig", &userconfig)
            .env("npm_config_globalconfig", &globalconfig)
            .env("npm_config_update_notifier", "false")
            .env("npm_config_offline", "true")
            .output()
            .unwrap()
    };
    let output = shell("npm run probe -- 'hello world' 'ឯក'");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("ARGS=[\"hello world\",\"ឯក\"]"));
    let cwd = fs::read_to_string(temp.path().join("probe-cwd.txt")).unwrap();
    assert_eq!(
        Path::new(&cwd).canonicalize().unwrap(),
        temp.path().canonicalize().unwrap()
    );
    assert_eq!(shell("npm run probe -- --fail").status.code(), Some(37));
    for command in ["npm --version", "npx --version"] {
        if command.starts_with("npx")
            && PackageLauncher::find(PackageTool::Npx, temp.path()).is_err()
        {
            continue;
        }
        let output = shell(command);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.')
        );
    }
}
