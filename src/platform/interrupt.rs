//! Protect the interactive shell from console interrupts without disabling
//! delivery to children. The line editor handles Ctrl+C as input at the prompt;
//! foreground programs handle the normal Windows console broadcast themselves.

use std::{io, process::ExitStatus};

/// Install once for the interactive session; restore the previous handler chain
/// when it ends. Do not install an inheritable ignore flag or a new child group.
pub struct ShellInterruptGuard {
    _private: (),
}

impl ShellInterruptGuard {
    pub fn install() -> io::Result<Self> {
        #[cfg(windows)]
        // SAFETY: this process-lifetime function has the required callback ABI.
        // Registration affects only this process; the callback owns no pointers.
        if unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(handle_interrupt), 1)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { _private: () })
    }
}

impl Drop for ShellInterruptGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        // SAFETY: remove precisely the static callback this guard registered.
        // No ignored-signal attribute is changed or inherited by children.
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(handle_interrupt), 0);
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn handle_interrupt(event: u32) -> i32 {
    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};
    // Runs on a Windows callback thread: no allocation, I/O, locks or shell
    // state access. Leave close/logoff/shutdown events to Windows' handler chain.
    i32::from(matches!(event, CTRL_C_EVENT | CTRL_BREAK_EVENT))
}

pub fn exit_code(status: ExitStatus) -> i32 {
    let code = status.code().unwrap_or(1);
    #[cfg(windows)]
    if code as u32 == 0xc000_013a {
        // STATUS_CONTROL_C_EXIT -> the conventional shell interruption status.
        return 130;
    }
    code
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::process::ExitStatusExt;
    use windows_sys::Win32::System::Console::{
        CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    };

    #[test]
    fn protects_only_keyboard_interrupts_and_leaves_shutdown_events_to_windows() {
        for event in [CTRL_C_EVENT, CTRL_BREAK_EVENT] {
            // SAFETY: the callback accepts a plain event code and owns no data.
            assert_eq!(unsafe { handle_interrupt(event) }, 1);
        }
        for event in [CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT, 99] {
            // SAFETY: as above; unknown event codes must be passed through.
            assert_eq!(unsafe { handle_interrupt(event) }, 0);
        }
    }

    #[test]
    fn normalizes_native_interrupt_exit_and_preserves_ordinary_child_statuses() {
        assert_eq!(exit_code(ExitStatus::from_raw(0xc000_013a)), 130);
        for status in [0, 1, 37, 130, 255] {
            assert_eq!(exit_code(ExitStatus::from_raw(status)), status as i32);
        }
    }
}
