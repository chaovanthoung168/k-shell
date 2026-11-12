//! Locate the installed editor without invoking Git Bash, wrappers, or discovery
//! subprocesses. Nano is an external application and owns editing/configuration.
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

/// Normal PATH resolution has precedence. Bare nano/nano.exe can additionally
/// use Git for Windows' packaged editor; explicit paths never use this fallback.
pub fn default_editor(cwd: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let mut roots = Vec::new();
        if let Some(path) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&path) {
                let directory = cwd.join(directory);
                if directory.join("git.exe").is_file() {
                    // Git/cmd, Git/bin, and Git/mingw64/bin layouts.
                    if let Some(parent) = directory.parent() {
                        roots.push(parent.to_path_buf());
                        if let Some(grandparent) = parent.parent() {
                            roots.push(grandparent.to_path_buf());
                        }
                    }
                }
            }
        }
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(directory) = std::env::var_os(variable) {
                roots.push(PathBuf::from(directory).join("Git"));
            }
        }
        if let Some(directory) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(directory).join("Programs/Git"));
        }
        editor_in_git_roots(&roots)
    }
    #[cfg(not(windows))]
    {
        let _ = cwd;
        None
    }
}

#[cfg(windows)]
fn editor_in_git_roots(roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .map(|root| root.join("usr/bin/nano.exe"))
        .find(|editor| editor.is_file())
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn discovery_requires_an_executable_and_keeps_installation_order() {
        // All entries are test fixtures; no installed program is modified/run.
        let root = std::env::temp_dir().join(format!("kshell-nano-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let installs = [root.join("first"), root.join("second")];
        for install in &installs {
            std::fs::create_dir_all(install.join("usr/bin")).unwrap();
        }
        assert!(super::editor_in_git_roots(&installs).is_none());
        std::fs::write(installs[0].join("usr/bin/nano.cmd"), b"never executed").unwrap();
        let second = installs[1].join("usr/bin/nano.exe");
        std::fs::write(&second, b"never executed").unwrap();
        assert_eq!(super::editor_in_git_roots(&installs), Some(second));
        let first = installs[0].join("usr/bin/nano.exe");
        std::fs::write(&first, b"never executed").unwrap();
        assert_eq!(super::editor_in_git_roots(&installs), Some(first));
        // The checked absolute root was created exclusively by this test.
        std::fs::remove_dir_all(root).unwrap();
    }
}
