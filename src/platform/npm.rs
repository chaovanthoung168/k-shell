//! Native Node/npm CLI launching, without interpreting npm.cmd/npm.ps1.
//! npm itself owns package scripts, including their normal Windows script shell.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use crate::error::ShellError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageTool {
    Npm,
    Npx,
}

impl PackageTool {
    /// Only bare names use the adapter. Explicit paths and scripts keep the
    /// normal executable policy, including rejection of .cmd/.ps1 files.
    pub fn parse(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("npm") {
            Some(Self::Npm)
        } else if name.eq_ignore_ascii_case("npx") {
            Some(Self::Npx)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Npx => "npx",
        }
    }

    fn script(self) -> &'static str {
        match self {
            Self::Npm => "npm-cli.js",
            Self::Npx => "npx-cli.js",
        }
    }
}

#[derive(Debug)]
pub struct PackageLauncher {
    node: PathBuf,
    cli: PathBuf,
}

impl PackageLauncher {
    pub fn find(tool: PackageTool, cwd: &Path) -> Result<Self, ShellError> {
        let mut directories: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|path| {
                std::env::split_paths(&path)
                    .map(|path| cwd.join(path))
                    .collect()
            })
            .unwrap_or_default();
        // node.exe is explicit so discovery does not depend on PATHEXT order.
        let node = super::resolve_executable("node.exe", cwd).ok();
        if let Some(directory) = node.as_ref().and_then(|node| node.parent()) {
            directories.push(directory.to_path_buf());
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            directories.push(PathBuf::from(appdata).join("npm"));
        }
        for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(directory) = std::env::var_os(variable) {
                directories.push(PathBuf::from(directory).join("nodejs"));
            }
        }
        Self::find_in(tool, &directories, node.as_deref()).ok_or_else(|| ShellError::new(
            format!("{}: Node.js and its npm CLI were not found; install Node.js with npm or add its installation directory to PATH", tool.name()), 127,
        ))
    }

    /// Explicit inputs allow deterministic tests without editing process PATH
    /// or running code to discover the installation. PATH roots win in order.
    pub fn find_in(
        tool: PackageTool,
        directories: &[PathBuf],
        fallback_node: Option<&Path>,
    ) -> Option<Self> {
        for directory in directories {
            let cli = directory.join("node_modules/npm/bin").join(tool.script());
            if !cli.is_file() {
                continue;
            }
            let adjacent = directory.join("node.exe");
            let node = if adjacent.is_file() {
                adjacent
            } else {
                let Some(node) = fallback_node.filter(|node| {
                    node.is_file()
                        && node
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
                }) else {
                    continue;
                };
                node.to_path_buf()
            };
            return Some(Self { node, cli });
        }
        None
    }

    pub fn command(&self, args: &[OsString], cwd: &Path) -> Result<Command, ShellError> {
        let directory = self
            .node
            .parent()
            .ok_or_else(|| ShellError::new("npm: node.exe has no parent directory", 126))?;
        let inherited: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default();
        let path = std::env::join_paths(std::iter::once(directory.to_path_buf()).chain(inherited))
            .map_err(|error| {
                ShellError::new(format!("npm: cannot prepare Node.js PATH: {error}"), 126)
            })?;
        let mut command = Command::new(&self.node);
        command
            .arg(&self.cli)
            .args(args)
            .current_dir(cwd)
            .env("PATH", path);
        Ok(command)
    }
}
