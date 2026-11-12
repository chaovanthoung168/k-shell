use std::path::{Path, PathBuf};

use crate::error::ShellError;

/// A validated file-opening request. Preparing a request never launches an app;
/// launching delegates the one file path to Windows' registered default handler.
#[derive(Debug)]
pub struct OpenRequest {
    file: PathBuf,
    directory: PathBuf,
}

impl OpenRequest {
    pub fn new(target: &Path, cwd: &Path) -> Result<Self, ShellError> {
        if target.as_os_str().is_empty() {
            return Err(ShellError::new("open: file path must not be empty", 2));
        }
        let file = cwd
            .join(target)
            .canonicalize()
            .map_err(|error| ShellError::new(format!("open: {}: {error}", target.display()), 1))?;
        if !file.is_file() {
            return Err(ShellError::new(
                "open: expected a file; use cd to enter a directory",
                1,
            ));
        }
        if file
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                ["cmd", "bat", "exe", "com"]
                    .iter()
                    .any(|blocked| ext.eq_ignore_ascii_case(blocked))
            })
        {
            return Err(ShellError::new(
                "open: use a program name to execute applications; batch-script compatibility is not implemented",
                126,
            ));
        }
        let directory = cwd
            .canonicalize()
            .map_err(|error| ShellError::new(format!("open: working directory: {error}"), 1))?;
        #[cfg(windows)]
        let (file, directory) = (
            windows::shell_path(&file)?,
            windows::shell_path(&directory)?,
        );
        Ok(Self { file, directory })
    }

    pub fn file(&self) -> &Path {
        &self.file
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn launch(&self) -> Result<(), ShellError> {
        #[cfg(windows)]
        {
            windows::launch(self)
        }
        #[cfg(not(windows))]
        {
            Err(ShellError::new(
                "open: default-app opening is currently supported only on Windows",
                1,
            ))
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Path, PathBuf, Prefix};

    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::{OpenRequest, ShellError};

    // Windows canonicalize returns verbatim paths. The desktop Shell expects
    // normal drive/UNC paths. Rebuild using OsStr components without lossy UTF-8.
    pub(super) fn shell_path(path: &Path) -> Result<PathBuf, ShellError> {
        if path.components().any(|part| matches!(part, Component::Normal(name)
            if name.encode_wide().last().is_some_and(|last| last == u16::from(b'.') || last == u16::from(b' ')))) {
            return Err(ShellError::new("open: the Windows app launcher cannot preserve paths ending with a dot or space", 1));
        }
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return Ok(path.to_path_buf());
        };
        let mut result = match prefix.kind() {
            Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
            Prefix::VerbatimUNC(server, share) => {
                let mut root = OsString::from(r"\\");
                root.push(server);
                root.push(r"\");
                root.push(share);
                root.push(r"\");
                PathBuf::from(root)
            }
            _ => return Ok(path.to_path_buf()),
        };
        for part in path.components().skip(1) {
            if !matches!(part, Component::RootDir) {
                result.push(part.as_os_str());
            }
        }
        Ok(result)
    }

    fn wide_path(path: &Path) -> Result<Vec<u16>, ShellError> {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(ShellError::new(
                "open: file path contains a NUL character",
                2,
            ));
        }
        wide.push(0);
        Ok(wide)
    }

    struct ComApartment;
    impl ComApartment {
        fn initialize() -> Result<Self, ShellError> {
            // SAFETY: null is required for pvReserved. COM is initialized on the
            // current thread and every successful call is balanced by Drop.
            let status = unsafe {
                CoInitializeEx(
                    std::ptr::null(),
                    (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
                )
            };
            if status < 0 {
                return Err(ShellError::new(
                    format!(
                        "open: cannot initialize Windows file opening (0x{:08X})",
                        status as u32
                    ),
                    1,
                ));
            }
            Ok(Self)
        }
    }
    impl Drop for ComApartment {
        fn drop(&mut self) {
            // SAFETY: runs on the thread where our successful init occurred.
            unsafe {
                CoUninitialize();
            }
        }
    }

    pub(super) fn launch(request: &OpenRequest) -> Result<(), ShellError> {
        let file = wide_path(&request.file)?;
        let directory = wide_path(&request.directory)?;
        let _com = ComApartment::initialize()?;
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            // The calling thread has no Windows message loop. Finish handing
            // the file off before returning, without waiting for playback.
            fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: windows_sys::core::w!("open"),
            lpFile: file.as_ptr(),
            lpParameters: std::ptr::null(),
            lpDirectory: directory.as_ptr(),
            nShow: SW_SHOWNORMAL,
            ..Default::default()
        };
        // SAFETY: info has the required size and live, NUL-terminated UTF-16
        // buffers. No command-line concatenation, shell parser, or process handle
        // ownership is involved; SEE_MASK_NOCLOSEPROCESS is deliberately unset.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(1155) {
                return Err(ShellError::new(
                    "open: no default app is configured for this file type; choose one in Windows Settings > Apps > Default apps",
                    1,
                ));
            }
            return Err(ShellError::new(
                format!("open: {}: {error}", request.file.display()),
                1,
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::windows::ffi::OsStringExt;

        #[test]
        fn desktop_shell_paths_preserve_drive_unc_unicode_and_native_code_units() {
            assert_eq!(
                shell_path(Path::new(r"\\?\D:\Music\ឯក song.mp3")).unwrap(),
                Path::new(r"D:\Music\ឯក song.mp3")
            );
            assert_eq!(
                shell_path(Path::new(r"\\?\UNC\server\share\movie.mp4")).unwrap(),
                Path::new(r"\\server\share\movie.mp4")
            );
            let mut units: Vec<u16> = r"\\?\D:\Music\".encode_utf16().collect();
            units.push(0xD800); // A native filename need not be valid Unicode.
            units.extend(".mp3".encode_utf16());
            let path = PathBuf::from(OsString::from_wide(&units));
            assert_eq!(
                wide_path(&shell_path(&path).unwrap()).unwrap(),
                units[4..].iter().copied().chain([0]).collect::<Vec<_>>()
            );
        }

        #[test]
        fn reject_paths_the_desktop_shell_could_reinterpret() {
            assert!(shell_path(Path::new(r"\\?\D:\song.mp3.")).is_err());
            assert!(shell_path(Path::new(r"\\?\D:\folder \song.mp3")).is_err());
            assert!(wide_path(Path::new("D:\\song\0.mp3")).is_err());
        }
    }
}
