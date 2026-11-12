use std::io::{self, IsTerminal, Write};

/// Clear the viewport AND scrollback, then leave the cursor at the top left.
/// Called only by the interactive host/key binding, never a utility stream.
/// Return false when VT is unavailable so the editor can use its native fallback.
pub fn clear_screen(output: &mut dyn Write) -> io::Result<bool> {
    if !io::stdout().is_terminal() {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
            SetConsoleMode,
        };
        let mut mode = 0;
        // SAFETY: borrowed stdout handle and writable mode pointer. A child may
        // have changed modes, so re-enable VT instead of trusting startup state.
        // The session's ConsoleOutput guard restores the original mode on exit.
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if GetConsoleMode(handle, &mut mode) == 0
                || SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) == 0
            {
                return Ok(false);
            }
        }
    }
    #[cfg(not(windows))]
    if std::env::var_os("TERM").is_some_and(|term| term == "dumb") {
        return Ok(false);
    }
    // Clear the viewport before scrollback: some terminals push the erased
    // viewport into scrollback during 2J. NO_COLOR does not disable screen control.
    output.write_all(b"\x1b[H\x1b[2J\x1b[3J")?;
    output.flush()?;
    Ok(true)
}

/// Keep ANSI output enabled between editor reads, when built-ins print colors.
/// The editor restores console modes after each read; enabling this before its
/// construction makes that restore preserve ANSI support for command output.
pub struct ConsoleOutput {
    ansi: bool,
    #[cfg(windows)]
    original: Option<(windows_sys::Win32::Foundation::HANDLE, u32)>,
}

impl ConsoleOutput {
    pub fn enable() -> Self {
        let mut result = Self {
            ansi: io::stdout().is_terminal(),
            #[cfg(windows)]
            original: None,
        };
        #[cfg(windows)]
        if result.ansi {
            use windows_sys::Win32::System::Console::{
                ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle,
                STD_OUTPUT_HANDLE, SetConsoleMode,
            };
            let mut mode = 0;
            // SAFETY: the borrowed stdout handle stays owned by the process;
            // mode is a valid out pointer. Every API result is checked.
            unsafe {
                let handle = GetStdHandle(STD_OUTPUT_HANDLE);
                result.ansi = GetConsoleMode(handle, &mut mode) != 0
                    && SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0;
                if result.ansi {
                    result.original = Some((handle, mode));
                }
            }
        }
        result
    }

    pub fn colors_enabled(&self) -> bool {
        self.ansi
            && !std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
            && std::env::var_os("TERM").is_none_or(|value| value != "dumb")
    }
}

impl Drop for ConsoleOutput {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some((handle, mode)) = self.original {
            // SAFETY: this is the same borrowed console handle and mode captured
            // at construction. Failure during shutdown is not recoverable here.
            unsafe {
                windows_sys::Win32::System::Console::SetConsoleMode(handle, mode);
            }
        }
    }
}

/// Own the application's interactive console title and restore the caller's
/// title on exit. Native Windows APIs also propagate to Windows Terminal tabs.
pub struct ConsoleTitle {
    #[cfg(windows)]
    original: Vec<u16>,
    #[cfg(windows)]
    title: Vec<u16>,
}

impl ConsoleTitle {
    pub fn new(title: &str) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
            use windows_sys::Win32::System::Console::GetConsoleTitleW;
            let mut title: Vec<u16> = title.encode_utf16().collect();
            if title.contains(&0) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "terminal title contains a NUL character",
                ));
            }
            title.push(0);
            let mut original = vec![0u16; 65_536];
            // SAFETY: the writable buffer has the advertised capacity. Clear
            // last-error first to distinguish a valid empty title from failure.
            let length = unsafe {
                SetLastError(0);
                let length = GetConsoleTitleW(original.as_mut_ptr(), original.len() as u32);
                if length == 0 && GetLastError() != 0 {
                    return Err(io::Error::last_os_error());
                }
                length as usize
            };
            original.truncate(length);
            original.push(0);
            let guard = Self { original, title };
            guard.refresh()?;
            Ok(guard)
        }
        #[cfg(not(windows))]
        {
            let _ = title;
            Ok(Self {})
        }
    }

    pub fn refresh(&self) -> io::Result<()> {
        #[cfg(windows)]
        {
            // SAFETY: title is an owned, NUL-terminated UTF-16 buffer.
            if unsafe { windows_sys::Win32::System::Console::SetConsoleTitleW(self.title.as_ptr()) }
                == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

impl Drop for ConsoleTitle {
    fn drop(&mut self) {
        #[cfg(windows)]
        // SAFETY: original is the owned, NUL-terminated title captured at startup.
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleTitleW(self.original.as_ptr());
        }
    }
}
