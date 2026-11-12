use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::error::ShellError;

/// Native equivalent of VS Code's Windows code.cmd wrapper. The wrapper is
/// never executed; only its known, quoted installation-relative CLI path is
/// treated as metadata so versioned installation directories remain supported.
#[derive(Debug)]
pub struct CodeLauncher {
    executable: PathBuf,
    cli: PathBuf,
}

impl CodeLauncher {
    pub fn from_installation(root: &Path) -> Option<Self> {
        let executable = root.join("Code.exe");
        if !executable.is_file() {
            return None;
        }
        let shim = std::fs::read_to_string(root.join("bin/code.cmd")).unwrap_or_default();
        let active_cli = shim.split('"').find_map(|quoted| {
            let relative = Path::new(quoted.strip_prefix(r"%~dp0..\")?);
            if !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
                || !relative.ends_with("resources/app/out/cli.js")
            {
                return None;
            }
            let cli = root.join(relative);
            cli.is_file().then_some(cli)
        });
        let cli = active_cli.or_else(|| {
            let cli = root.join("resources/app/out/cli.js");
            cli.is_file().then_some(cli)
        })?;
        Some(Self { executable, cli })
    }

    pub fn find(cwd: &Path) -> Result<Self, ShellError> {
        let paths: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|value| {
                std::env::split_paths(&value)
                    .map(|path| cwd.join(path))
                    .collect()
            })
            .unwrap_or_default();
        let mut roots = Vec::new();
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("Programs/Microsoft VS Code"));
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            roots.push(PathBuf::from(home).join("AppData/Local/Programs/Microsoft VS Code"));
        }
        for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(base) = std::env::var_os(variable) {
                roots.push(PathBuf::from(base).join("Microsoft VS Code"));
            }
        }
        Self::find_in(&paths, &roots).ok_or_else(|| ShellError::new(
            "code: VS Code's native CLI was not found; install VS Code or add its bin directory to PATH", 127,
        ))
    }

    /// Explicit search inputs keep discovery testable without changing process
    /// environment variables or launching the user's installed desktop editor.
    pub fn find_in(path_directories: &[PathBuf], installation_roots: &[PathBuf]) -> Option<Self> {
        for directory in path_directories {
            if directory.join("code.cmd").is_file()
                && let Some(launcher) = directory.parent().and_then(Self::from_installation)
            {
                return Some(launcher);
            }
        }
        installation_roots
            .iter()
            .find_map(|root| Self::from_installation(root))
    }

    pub fn command(&self, args: &[OsString], cwd: &Path) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .arg(&self.cli)
            .args(args)
            .current_dir(cwd)
            .env_remove("VSCODE_DEV")
            .env("ELECTRON_RUN_AS_NODE", "1");
        command
    }
}
