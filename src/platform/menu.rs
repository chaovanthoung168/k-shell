//! Console key input for a short selection menu, not a second line editor.
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    Up,
    Down,
    Accept,
    Cancel,
}

#[cfg(windows)]
pub fn read_key() -> io::Result<MenuKey> {
    use windows_sys::Win32::{Foundation::HANDLE, System::Console::*};
    struct InputMode(HANDLE, u32);
    impl Drop for InputMode {
        fn drop(&mut self) {
            // SAFETY: borrowed input handle and its captured original mode.
            unsafe {
                SetConsoleMode(self.0, self.1);
            }
        }
    }
    // SAFETY: borrowed stdin handle and valid out pointers. The guard restores
    // every original input flag on success, cancellation, or a read failure.
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        let mut original = 0;
        if GetConsoleMode(handle, &mut original) == 0 {
            return Err(io::Error::last_os_error());
        }
        if SetConsoleMode(
            handle,
            (original | ENABLE_EXTENDED_FLAGS)
                & !(ENABLE_LINE_INPUT
                    | ENABLE_ECHO_INPUT
                    | ENABLE_PROCESSED_INPUT
                    | ENABLE_QUICK_EDIT_MODE
                    | ENABLE_VIRTUAL_TERMINAL_INPUT),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let _mode = InputMode(handle, original);
        loop {
            let mut event: INPUT_RECORD = std::mem::zeroed();
            let mut count = 0;
            if ReadConsoleInputW(handle, &mut event, 1, &mut count) == 0 {
                return Err(io::Error::last_os_error());
            }
            if count != 1 || event.EventType != KEY_EVENT as u16 {
                continue;
            }
            let key = event.Event.KeyEvent;
            if key.bKeyDown == 0 {
                continue;
            }
            if key.uChar.UnicodeChar == 3 {
                return Ok(MenuKey::Cancel);
            }
            match key.wVirtualKeyCode {
                0x26 => return Ok(MenuKey::Up),
                0x28 => return Ok(MenuKey::Down),
                0x0d => return Ok(MenuKey::Accept),
                0x1b => return Ok(MenuKey::Cancel),
                _ => {}
            }
        }
    }
}

#[cfg(not(windows))]
pub fn read_key() -> io::Result<MenuKey> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the arrow-key theme picker requires a Windows console; use theme list / theme install <name> / theme <name>",
    ))
}
