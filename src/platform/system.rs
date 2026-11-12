//! Read-only system snapshots. Windows APIs and registry access stay here;
//! command rendering does not depend on native types or execute other shells.

use crate::error::ShellError;

#[derive(Debug)]
pub struct Capacity {
    pub total: u64,
    pub available: u64,
}

#[derive(Debug)]
pub struct Volume {
    pub root: String,
    pub capacity: Result<Capacity, String>,
}

#[derive(Debug)]
pub struct OperatingSystem {
    pub name: String,
    pub release: Option<String>,
    pub build: u32,
    pub revision: Option<u32>,
}

#[derive(Debug)]
pub struct SystemSpecs {
    pub os: Result<OperatingSystem, String>,
    pub user: Result<String, String>,
    pub cpu: Result<String, String>,
    pub logical_processors: Option<usize>,
    pub gpus: Vec<String>,
    pub memory: Result<Capacity, String>,
    pub volumes: Result<Vec<Volume>, String>,
}

pub fn collect() -> Result<SystemSpecs, ShellError> {
    #[cfg(windows)]
    {
        Ok(windows::collect())
    }
    #[cfg(not(windows))]
    {
        Err(ShellError::new("sysinfo: supported on Windows only", 1))
    }
}

#[cfg(windows)]
mod windows {
    use std::{collections::BTreeSet, mem::size_of, ptr};

    use windows_sys::{
        Win32::{
            Graphics::Gdi::{
                DISPLAY_DEVICE_MIRRORING_DRIVER, DISPLAY_DEVICEW, EnumDisplayDevicesW,
            },
            Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives},
            System::{
                Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW},
                SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
                WindowsProgramming::{DRIVE_FIXED, GetUserNameW},
            },
        },
        core::w,
    };

    use super::{Capacity, OperatingSystem, SystemSpecs, Volume};

    fn last_error() -> String {
        std::io::Error::last_os_error().to_string()
    }

    pub(super) fn collect() -> SystemSpecs {
        SystemSpecs {
            os: operating_system(),
            user: user_name(),
            cpu: cpu_name(),
            logical_processors: std::thread::available_parallelism().ok().map(usize::from),
            gpus: gpu_names(),
            memory: memory(),
            volumes: volumes(),
        }
    }

    fn user_name() -> Result<String, String> {
        // UNLEN + 1: maximum Windows logon name plus its NUL terminator.
        let mut buffer = [0u16; 257];
        let mut length = buffer.len() as u32;
        // SAFETY: buffer owns the advertised UTF-16 capacity; length is a valid
        // in/out pointer. Read the security identity, not mutable USERNAME env.
        if unsafe { GetUserNameW(buffer.as_mut_ptr(), &mut length) } == 0 {
            return Err(last_error());
        }
        let name = from_wide(&buffer);
        if name.is_empty() {
            Err("user name was not reported".into())
        } else {
            Ok(name)
        }
    }

    fn os_registry_text(value: &str) -> Result<String, String> {
        let value: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let mut buffer = [0u16; 512];
        let mut bytes = size_of_val(&buffer) as u32;
        // SAFETY: both names are NUL terminated; the output buffer is aligned
        // UTF-16 with the advertised byte capacity. No registry handle is owned.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != 0 {
            return Err(std::io::Error::from_raw_os_error(status as i32).to_string());
        }
        let text = from_wide(&buffer).trim().to_owned();
        if text.is_empty() {
            Err("OS value was not reported".into())
        } else {
            Ok(text)
        }
    }

    fn os_revision() -> Option<u32> {
        let mut revision = 0u32;
        let mut bytes = size_of::<u32>() as u32;
        // SAFETY: constant NUL-terminated names, aligned DWORD out pointer and
        // exact byte capacity. Missing optional revision is simply omitted.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
                w!("UBR"),
                RRF_RT_REG_DWORD,
                ptr::null_mut(),
                (&mut revision as *mut u32).cast(),
                &mut bytes,
            )
        };
        (status == 0 && bytes == size_of::<u32>() as u32).then_some(revision)
    }

    fn product_name(name: String, build: u32) -> String {
        // Windows 11 can retain a Windows 10 ProductName for compatibility.
        // Correct only that client label; never relabel Windows Server products.
        if build >= 22_000
            && let Some(edition) = name.strip_prefix("Windows 10")
            && (edition.is_empty() || edition.starts_with(' '))
        {
            return format!("Windows 11{edition}");
        }
        name
    }

    fn operating_system() -> Result<OperatingSystem, String> {
        let name = os_registry_text("ProductName")?;
        let build = os_registry_text("CurrentBuildNumber")?
            .parse::<u32>()
            .map_err(|_| "invalid OS build number".to_owned())?;
        Ok(OperatingSystem {
            name: product_name(name, build),
            release: os_registry_text("DisplayVersion").ok(),
            build,
            revision: os_revision(),
        })
    }

    fn cpu_name() -> Result<String, String> {
        let mut buffer = [0u16; 512];
        let mut bytes = size_of_val(&buffer) as u32;
        // SAFETY: constant strings are NUL-terminated. The aligned UTF-16
        // buffer owns the advertised byte capacity; no registry handle is owned.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0"),
                w!("ProcessorNameString"),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != 0 {
            return Err(std::io::Error::from_raw_os_error(status as i32).to_string());
        }
        let name = from_wide(&buffer).trim().to_owned();
        if name.is_empty() {
            Err("processor name was not reported".into())
        } else {
            Ok(name)
        }
    }

    fn from_wide(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    fn gpu_names() -> Vec<String> {
        let mut names = BTreeSet::new();
        let mut index = 0;
        loop {
            let mut device = DISPLAY_DEVICEW {
                cb: size_of::<DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            // SAFETY: null asks for adapters rather than monitors. device has
            // the required size and is writable for the duration of the call.
            if unsafe { EnumDisplayDevicesW(ptr::null(), index, &mut device, 0) } == 0 {
                break;
            }
            if device.StateFlags & DISPLAY_DEVICE_MIRRORING_DRIVER == 0 {
                let name = from_wide(&device.DeviceString).trim().to_owned();
                if !name.is_empty() {
                    names.insert(name);
                }
            }
            index += 1;
        }
        names.into_iter().collect()
    }

    fn memory() -> Result<Capacity, String> {
        let mut memory = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: correctly sized initialized native structure and valid pointer.
        if unsafe { GlobalMemoryStatusEx(&mut memory) } == 0 {
            return Err(last_error());
        }
        Ok(Capacity {
            total: memory.ullTotalPhys,
            available: memory.ullAvailPhys,
        })
    }

    fn volumes() -> Result<Vec<Volume>, String> {
        // Only fixed local drive letters: no network requests or empty optical
        // or removable-media prompts. Capacities describe volumes, not devices.
        // SAFETY: GetLogicalDrives takes no pointers or handles.
        let drives = unsafe { GetLogicalDrives() };
        if drives == 0 {
            return Err(last_error());
        }
        let mut volumes = Vec::new();
        for drive in 0..26 {
            if drives & (1 << drive) == 0 {
                continue;
            }
            let root = [
                u16::from(b'A') + drive,
                u16::from(b':'),
                u16::from(b'\\'),
                0,
            ];
            // SAFETY: root is a valid NUL-terminated drive-root string.
            if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
                continue;
            }
            let mut total = 0;
            let mut available = 0;
            // SAFETY: root remains alive and both byte-count pointers are valid.
            let success = unsafe {
                GetDiskFreeSpaceExW(root.as_ptr(), &mut available, &mut total, ptr::null_mut())
            };
            let capacity = if success == 0 {
                Err(last_error())
            } else {
                Ok(Capacity { total, available })
            };
            volumes.push(Volume {
                root: from_wide(&root),
                capacity,
            });
        }
        Ok(volumes)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn windows_11_client_label_uses_build_without_relabeling_servers() {
            for (name, build, expected) in [
                ("Windows 10 Pro", 19_045, "Windows 10 Pro"),
                ("Windows 10 Pro", 22_000, "Windows 11 Pro"),
                (
                    "Windows 10 Home Single Language",
                    26_100,
                    "Windows 11 Home Single Language",
                ),
                ("Windows 11 Enterprise", 26_100, "Windows 11 Enterprise"),
                (
                    "Windows Server 2025 Standard",
                    26_100,
                    "Windows Server 2025 Standard",
                ),
                ("Windows 100", 26_100, "Windows 100"),
            ] {
                assert_eq!(super::product_name(name.to_owned(), build), expected);
            }
        }
    }
}
