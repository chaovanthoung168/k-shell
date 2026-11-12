//! Locate Windows' optional native OpenSSH client when it is absent from PATH.
//! Protocol, authentication, host-key checks and terminal I/O belong to OpenSSH.

#[cfg(windows)]
pub fn default_client() -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

    let mut buffer = vec![0u16; 32_768];
    // SAFETY: buffer is writable for the advertised number of UTF-16 elements.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    let directory = std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]));
    client_in(&directory)
}

#[cfg(windows)]
fn client_in(system_directory: &std::path::Path) -> Option<std::path::PathBuf> {
    let client = system_directory.join("OpenSSH/ssh.exe");
    client.is_file().then_some(client)
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn fallback_requires_an_existing_native_client() {
        // A unique directory owned by this test, never the real Windows tree.
        let root = std::env::temp_dir().join(format!("kshell-ssh-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        assert!(super::client_in(&root).is_none());
        std::fs::create_dir(root.join("OpenSSH")).unwrap();
        std::fs::write(root.join("OpenSSH/ssh.cmd"), b"not executed").unwrap();
        assert!(super::client_in(&root).is_none());
        std::fs::write(root.join("OpenSSH/ssh.exe"), b"not executed").unwrap();
        assert_eq!(super::client_in(&root), Some(root.join("OpenSSH/ssh.exe")));
        std::fs::remove_file(root.join("OpenSSH/ssh.exe")).unwrap();
        std::fs::remove_file(root.join("OpenSSH/ssh.cmd")).unwrap();
        std::fs::remove_dir(root.join("OpenSSH")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
