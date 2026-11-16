use serde_json::json;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
mod common;
use common::TempDir;
fn shell(root: &Path, command: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_kshell"))
        .env("KSHELL_KPM_HOME", root)
        .args(["-c", command])
        .output()
        .unwrap()
}
#[test]
fn catalog_help_and_noninteractive_errors_never_create_installations_or_contact_providers() {
    let temp = TempDir::new();
    let root = temp.path().join("empty-store");
    for command in [
        "kpm list",
        "kpm search java",
        "kpm --help",
        "help kpm",
        "kpm installed",
    ] {
        let output = shell(&root, command);
        assert!(output.status.success(), "{command}: {:?}", output.stderr);
        assert!(!output.stdout.contains(&27));
    }
    for command in [
        "kpm install node python git",
        "kpm install node",
        "kpm install node@1.0.0",
        "kpm install node --yes",
        "kpm install",
        "kpm use node",
        "kpm use node@1.0.0",
        "kpm uninstall node@1.0.0",
        "kpm recover",
    ] {
        let output = shell(&root, command);
        assert!(!output.status.success(), "{command}");
        assert!(!output.stderr.contains(&27));
    }
    assert!(!root.exists());
}
fn fixture_home(root: &Path, tool: &str, version: &str, fixture: &Path) -> serde_json::Value {
    let relative = if tool == "rust" {
        format!("rustup/toolchains/{version}-x86_64-pc-windows-msvc")
    } else {
        format!("packages/{tool}/{version}")
    };
    let home = root.join(&relative);
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("fixture-version.txt"), version).unwrap();
    let names: &[&str] = match tool {
        "node" => &["node.exe"],
        "java" => &["bin/java.exe", "bin/javac.exe"],
        "rust" => &["bin/rustc.exe", "bin/cargo.exe"],
        _ => unreachable!(),
    };
    for name in names {
        let path = home.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(fixture, path).unwrap();
    }
    if tool == "node" {
        fs::create_dir_all(home.join("node_modules/npm/bin")).unwrap();
        fs::write(home.join("node_modules/npm/bin/npm-cli.js"), "fixture").unwrap();
    }
    fs::write(
        home.join(".kpm-owned"),
        format!("KPM1\n{tool}\n{version}\n"),
    )
    .unwrap();
    json!({"tool":tool,"version":version,"home":relative})
}
#[test]
fn real_native_commands_follow_switches_and_saved_java_and_rust_environment() {
    let temp = TempDir::new();
    let root = temp.path().join("managed tools ឯក");
    fs::create_dir(&root).unwrap();
    fs::write(root.join(".kpm-root"), "KPM managed store v1\n").unwrap();
    let source = temp.path().join("probe.rs");
    let fixture = temp.path().join("probe.exe");
    fs::write(&source, include_str!("fixtures/kpm_probe.rs")).unwrap();
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compiled = Command::new(compiler)
        .arg("--edition=2024")
        .arg(&source)
        .arg("-o")
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{:?}", compiled.stderr);
    let records = vec![
        fixture_home(&root, "node", "1.0.0", &fixture),
        fixture_home(&root, "node", "2.0.0", &fixture),
        fixture_home(&root, "java", "21.0.6+7", &fixture),
        fixture_home(&root, "java", "21.0.7+8", &fixture),
        fixture_home(&root, "rust", "1.85.0", &fixture),
    ];
    fs::write(
        root.join("installed.json"),
        serde_json::to_vec(&json!({"records":records,"active":{}})).unwrap(),
    )
    .unwrap();
    let quote = |path: PathBuf| format!("'{}'", path.display());
    let p1 = temp.path().join("node-one.txt");
    let p2 = temp.path().join("node-two.txt");
    let pj = temp.path().join("java.txt");
    let pc = temp.path().join("javac.txt");
    let pr = temp.path().join("cargo.txt");
    let script = format!(
        "kpm use node@1.0.0\nnode --probe {}\nkpm use node@2.0.0\nnode --probe {}\nkpm use java@21\njava --probe {}\njavac --probe {}\nkpm use rust@1.85.0\ncargo --probe {}\n",
        quote(p1.clone()),
        quote(p2.clone()),
        quote(pj.clone()),
        quote(pc.clone()),
        quote(pr.clone())
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .env("KSHELL_KPM_HOME", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
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
    assert!(fs::read_to_string(p1).unwrap().contains("version=1.0.0"));
    assert!(fs::read_to_string(p2).unwrap().contains("version=2.0.0"));
    let java = fs::read_to_string(pj).unwrap();
    let javac = fs::read_to_string(pc).unwrap();
    let home = root.join("packages/java/21.0.7+8");
    assert!(java.contains(&format!("JAVA_HOME={}", home.display())));
    assert!(javac.contains(&format!("JAVA_HOME={}", home.display())));
    assert!(java.contains("version=21.0.7+8") && javac.contains("version=21.0.7+8"));
    let rust = fs::read_to_string(pr).unwrap();
    assert!(rust.contains(&format!("RUSTUP_HOME={}", root.join("rustup").display())));
    assert!(rust.contains(&format!("CARGO_HOME={}", root.join("cargo").display())));
    let saved = shell(&root, "node --version");
    assert!(saved.status.success());
    assert_eq!(saved.stdout, b"v2.0.0\n");
    let current = shell(&root, "kpm current java");
    assert!(current.status.success(), "{:?}", current.stderr);
    assert!(
        String::from_utf8(current.stdout)
            .unwrap()
            .contains("java@21.0.7+8")
    );
    let listed = shell(&root, "kpm installed node");
    assert!(
        String::from_utf8(listed.stdout)
            .unwrap()
            .contains("node@2.0.0 (active)")
    );
}
