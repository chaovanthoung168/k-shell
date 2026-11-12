//! Discover Windows' packaged curl without spawning a discovery process.
//! Requests, TLS, proxies, configuration and options belong to curl itself.

#[cfg(windows)]
pub fn default_client() -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

    let mut buffer = vec![0u16; 32_768];
    // SAFETY: buffer owns the advertised writable UTF-16 capacity. This obtains
    // the native system directory without trusting mutable SystemRoot values.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    let directory = std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]));
    let client = directory.join("curl.exe");
    client.is_file().then_some(client)
}
