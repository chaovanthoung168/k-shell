use std::ffi::OsStr;
use std::fs::Metadata;
use std::{fs, io, path::Path};

pub fn is_hidden(name: &OsStr, metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x2 != 0 {
            return true;
        }
    }
    let _ = metadata;
    name.to_str().is_some_and(|name| name.starts_with('.'))
}

/// Recursive utilities treat all Windows reparse points (including junctions)
/// as leaves. symlink_metadata must be used to obtain this metadata.
pub fn is_link_or_reparse_point(metadata: &Metadata) -> bool {
    if metadata.is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    false
}

/// Rename without replacing an existing destination. Unlike fs::rename on
/// Windows, this never requests replacement. Keep UTF-16 and Win32 here.
pub fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: both strings are owned, NUL-terminated and live through the call.
        if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        // The native Windows implementation provides the no-replace guarantee;
        // this portability fallback has a check/rename race on other platforms.
        if fs::symlink_metadata(destination).is_ok() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        fs::rename(source, destination)
    }
}

pub fn is_cross_volume(error: &io::Error) -> bool {
    #[cfg(windows)]
    {
        error.raw_os_error() == Some(17)
    } // ERROR_NOT_SAME_DEVICE
    #[cfg(not(windows))]
    {
        error.kind() == io::ErrorKind::CrossesDevices
    }
}

/// A link is removed as an entry, never traversed, including directory junctions.
pub fn remove_leaf(path: &Path, metadata: &Metadata) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x10 != 0 {
            return fs::remove_dir(path);
        }
    }
    if metadata.is_dir() && !is_link_or_reparse_point(metadata) {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

/// Open the final entry without following a Windows reparse point; the caller
/// must inspect handle metadata before reading any contents.
pub fn open_copy_source(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

/// Reject ambiguous drive-relative paths, devices, streams and names whose
/// Win32 interpretation differs from an ordinary filesystem entry.
pub fn validate_file_operand(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        for part in path.components() {
            match part {
                Component::Prefix(prefix) => match prefix.kind() {
                    Prefix::Disk(_) if path.has_root() => (),
                    Prefix::UNC(_, _) | Prefix::VerbatimDisk(_) | Prefix::VerbatimUNC(_, _) => (),
                    _ => {
                        return Err(io::Error::other(
                            "use an absolute drive/UNC path, not a device or drive-relative path",
                        ));
                    }
                },
                Component::Normal(name) => {
                    // Input words are UTF-8; OS-generated names that aren't
                    // Unicode remain OsStr throughout copying/traversal.
                    if let Some(name) = name.to_str() {
                        let stem = name.split('.').next().unwrap_or("").to_uppercase();
                        let numbered = stem
                            .strip_prefix("COM")
                            .or_else(|| stem.strip_prefix("LPT"));
                        let device = matches!(
                            stem.as_str(),
                            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
                        ) || numbered.is_some_and(|n| {
                            matches!(
                                n,
                                "1" | "2"
                                    | "3"
                                    | "4"
                                    | "5"
                                    | "6"
                                    | "7"
                                    | "8"
                                    | "9"
                                    | "¹"
                                    | "²"
                                    | "³"
                            )
                        });
                        if device
                            || name.ends_with(['.', ' '])
                            || name.chars().any(|c| c < ' ' || "<>:\"|?*".contains(c))
                        {
                            return Err(io::Error::other(
                                "reserved or unsupported Windows filename",
                            ));
                        }
                    }
                }
                _ => (),
            }
        }
    }
    let _ = path;
    Ok(())
}

/// Open without truncating, then verify the handle is a regular non-reparse
/// file. Text commands truncate only after this succeeds. Final-entry reparse
/// handling stays inside the Windows platform boundary.
pub fn open_text_target(path: &Path, append: bool) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options
        .write(true)
        .append(append)
        .create(true)
        .truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_link_or_reparse_point(&metadata) {
        return Err(io::Error::other(
            "expected a regular file, not a link or reparse point",
        ));
    }
    Ok(file)
}
