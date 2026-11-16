//! Read-only current-user installer identity. Never elevate or alter registry keys.
use std::{io, path::Path};
#[cfg(windows)]
pub fn matches_user_installation(home: &Path, label: &str) -> io::Result<bool> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};
    use windows_sys::Win32::{
        Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS},
        System::Registry::*,
    };
    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    unsafe fn value(key: HKEY, name: &str) -> Option<OsString> {
        let name = wide(name);
        let mut kind = 0;
        let mut size = 0;
        // SAFETY: borrowed open key; query size, then read into an owned buffer.
        if unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                ptr::null(),
                &mut kind,
                ptr::null_mut(),
                &mut size,
            )
        } != ERROR_SUCCESS
            || kind != REG_SZ
            || size > 65536
        {
            return None;
        }
        let mut data = vec![0u16; (size as usize).div_ceil(2)];
        if unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                ptr::null(),
                &mut kind,
                data.as_mut_ptr() as *mut u8,
                &mut size,
            )
        } != ERROR_SUCCESS
        {
            return None;
        }
        while data.last() == Some(&0) {
            data.pop();
        }
        Some(OsString::from_wide(&data))
    }
    let expected = home.canonicalize()?;
    let root = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall");
    let mut key = ptr::null_mut();
    // SAFETY: constant HKCU root, valid UTF-16 name and out pointer, read-only access.
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, root.as_ptr(), 0, KEY_READ, &mut key) };
    if status != ERROR_SUCCESS {
        return Ok(false);
    }
    let root = Key(key);
    for index in 0..10000 {
        let mut name = vec![0u16; 16384];
        let mut length = name.len() as u32;
        let status = unsafe {
            RegEnumKeyExW(
                root.0,
                index,
                name.as_mut_ptr(),
                &mut length,
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if status == ERROR_NO_MORE_ITEMS {
            return Ok(false);
        }
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        name[length as usize] = 0;
        let mut handle = ptr::null_mut();
        if unsafe { RegOpenKeyExW(root.0, name.as_ptr(), 0, KEY_READ, &mut handle) }
            != ERROR_SUCCESS
        {
            continue;
        }
        let entry = Key(handle);
        let (Some(location), Some(display)) =
            (unsafe { value(entry.0, "InstallLocation") }, unsafe {
                value(entry.0, "DisplayName")
            })
        else {
            continue;
        };
        if !display
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains(&label.to_ascii_lowercase())
        {
            continue;
        }
        let location = std::path::PathBuf::from(location);
        if !location.is_absolute() {
            continue;
        }
        if let Ok(actual) = location.canonicalize()
            && actual == expected
        {
            return Ok(true);
        }
    }
    Err(io::Error::other("too many user installer registry entries"))
}
#[cfg(not(windows))]
pub fn matches_user_installation(_home: &Path, _label: &str) -> io::Result<bool> {
    Ok(false)
}
