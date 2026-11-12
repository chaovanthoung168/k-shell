use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::error::ShellError;

pub mod console;
pub mod curl;
pub mod filesystem;
pub mod interrupt;
pub mod nano;
pub mod npm;
pub mod open;
pub mod ssh;
pub mod system;
pub mod vscode;

// Resolve before spawning: Rust may otherwise implicitly run batch files via
// cmd.exe on Windows. Batch compatibility is intentionally not enabled.
pub fn resolve_executable(name: &str, cwd: &Path) -> Result<PathBuf, ShellError> {
    let path = Path::new(name);
    let extension = path.extension().and_then(|extension| extension.to_str());
    if extension
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bat") || ext.eq_ignore_ascii_case("cmd"))
    {
        return Err(ShellError::new(
            ".bat/.cmd compatibility is not implemented; use a native executable",
            126,
        ));
    }
    #[cfg(windows)]
    if extension
        .is_some_and(|ext| !ext.eq_ignore_ascii_case("exe") && !ext.eq_ignore_ascii_case("com"))
    {
        return Err(ShellError::new(
            "only native .exe and .com executables are supported",
            126,
        ));
    }

    let explicit = path.is_absolute() || path.components().count() > 1;
    let directories: Vec<PathBuf> = if explicit {
        vec![cwd.to_path_buf()]
    } else {
        env::var_os("PATH")
            .map(|value| env::split_paths(&value).collect())
            .unwrap_or_default()
    };
    for directory in directories {
        let candidate = cwd.join(directory).join(path);
        #[cfg(windows)]
        if extension.is_none() {
            let extensions = env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE"));
            for ext in extensions.to_string_lossy().split(';') {
                if ext.eq_ignore_ascii_case(".exe") || ext.eq_ignore_ascii_case(".com") {
                    let mut filename = candidate.as_os_str().to_os_string();
                    filename.push(ext);
                    let filename = PathBuf::from(filename);
                    if filename.is_file() {
                        return Ok(filename);
                    }
                }
            }
            continue;
        }
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    #[cfg(windows)]
    if !explicit && (name.eq_ignore_ascii_case("curl") || name.eq_ignore_ascii_case("curl.exe")) {
        return curl::default_client().ok_or_else(|| ShellError::new(
            "curl: curl.exe was not found; install curl for Windows and add its directory to PATH", 127,
        ));
    }
    #[cfg(windows)]
    if !explicit && (name.eq_ignore_ascii_case("ssh") || name.eq_ignore_ascii_case("ssh.exe")) {
        return ssh::default_client().ok_or_else(|| ShellError::new(
            "ssh: OpenSSH Client was not found; enable OpenSSH Client in Windows Settings > Optional features, or add a native ssh.exe to PATH", 127,
        ));
    }
    #[cfg(windows)]
    if !explicit && (name.eq_ignore_ascii_case("nano") || name.eq_ignore_ascii_case("nano.exe")) {
        return nano::default_editor(cwd).ok_or_else(|| ShellError::new(
            "nano: nano.exe was not found; install a Windows build of GNU nano and add its folder to PATH (Git for Windows' bundled Nano is also supported)", 127,
        ));
    }
    Err(ShellError::new(format!("command not found: {name}"), 127))
}

pub fn home_directory() -> Option<OsString> {
    env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
}

/// Windows canonical paths can have a verbatim prefix. Hide it only for
/// presentation; all filesystem and process operations retain the original path.
pub fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    #[cfg(windows)]
    {
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = text.strip_prefix(r"\\?\")
            && rest.as_bytes().get(1) == Some(&b':')
        {
            return rest.to_owned();
        }
    }
    text
}
