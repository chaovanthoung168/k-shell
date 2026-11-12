#![cfg(windows)]

//! Run real Ctrl+C broadcasts in a NEW, HIDDEN console, never the user's console.
//! A test-only job owns every probe process, including failure/timeout cleanup.

use std::{
    fs, io,
    io::IsTerminal,
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    process::{Command, Stdio},
    ptr, thread,
    time::{Duration, Instant},
};

use kshell::platform::{
    interrupt::ShellInterruptGuard,
    npm::{PackageLauncher, PackageTool},
};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::{
        Console::{
            CONSOLE_SCREEN_BUFFER_INFO, COORD, CTRL_C_EVENT, GenerateConsoleCtrlEvent,
            GetConsoleProcessList, GetConsoleScreenBufferInfo, GetConsoleTitleW, GetStdHandle,
            INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0,
            ReadConsoleOutputCharacterW, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
            SetConsoleCtrlHandler, WriteConsoleInputW,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Threading::{
            CREATE_NEW_CONSOLE, CREATE_SUSPENDED, CreateProcessW, GetExitCodeProcess,
            PROCESS_INFORMATION, ResumeThread, STARTF_USESHOWWINDOW, STARTUPINFOW,
            TerminateProcess, WaitForSingleObject,
        },
    },
};

mod common;
use common::TempDir;
use rustyline::history::{DefaultHistory, History};

fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}

fn checked(success: i32) -> io::Result<()> {
    if success == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn type_command(text: &str) -> io::Result<()> {
    let records: Vec<_> = text
        .encode_utf16()
        .map(|character| INPUT_RECORD {
            EventType: KEY_EVENT as u16,
            Event: INPUT_RECORD_0 {
                KeyEvent: KEY_EVENT_RECORD {
                    bKeyDown: 1,
                    wRepeatCount: 1,
                    wVirtualKeyCode: if character == 13 { 13 } else { 0 },
                    uChar: KEY_EVENT_RECORD_0 {
                        UnicodeChar: character,
                    },
                    ..Default::default()
                },
            },
        })
        .collect();
    let mut written = 0;
    // SAFETY: records owns initialized input events and written is a valid out
    // pointer. This helper is called only after checking its isolated console.
    unsafe {
        checked(WriteConsoleInputW(
            GetStdHandle(STD_INPUT_HANDLE),
            records.as_ptr(),
            records.len() as u32,
            &mut written,
        ))?;
    }
    if written != records.len() as u32 {
        return Err(io::Error::other("short console input write"));
    }
    Ok(())
}

fn wait_until(description: &str, predicate: impl Fn() -> bool) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !predicate() {
        if Instant::now() >= deadline {
            return Err(io::Error::other(format!("timeout: {description}")));
        }
        thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}

fn control_key(letter: u16) -> io::Result<()> {
    let record = INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: 1,
                wRepeatCount: 1,
                wVirtualKeyCode: letter,
                uChar: KEY_EVENT_RECORD_0 {
                    UnicodeChar: letter - u16::from(b'A') + 1,
                },
                dwControlKeyState: windows_sys::Win32::System::Console::LEFT_CTRL_PRESSED,
                ..Default::default()
            },
        },
    };
    let mut written = 0;
    // SAFETY: valid initialized event/result pointer; called only in the checked
    // private console. No events are injected into a user or test-runner console.
    unsafe {
        checked(WriteConsoleInputW(
            GetStdHandle(STD_INPUT_HANDLE),
            &record,
            1,
            &mut written,
        ))?;
    }
    if written == 1 {
        Ok(())
    } else {
        Err(io::Error::other("short control-key write"))
    }
}

fn screen_text() -> io::Result<String> {
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    // SAFETY: borrowed console handle and initialized writable out structures.
    // Called only in the verified private console, never in the user's terminal.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        checked(GetConsoleScreenBufferInfo(handle, &mut info))?;
        let cells = (info.dwSize.X as usize * info.dwSize.Y as usize).min(2_000_000);
        let mut text = vec![0u16; cells];
        let mut read = 0;
        checked(ReadConsoleOutputCharacterW(
            handle,
            text.as_mut_ptr(),
            text.len() as u32,
            COORD { X: 0, Y: 0 },
            &mut read,
        ))?;
        Ok(String::from_utf16_lossy(&text[..read as usize]))
    }
}

#[test]
fn native_ctrl_c_stops_npm_descendants_and_shell_accepts_another_command() {
    let temp = TempDir::new();
    if PackageLauncher::find(PackageTool::Npm, temp.path()).is_err() {
        eprintln!("Node/npm unavailable; skipping isolated-console npm regression");
        return;
    }
    fs::write(
        temp.path().join("console-fixture"),
        b"owned Ctrl+C test console",
    )
    .unwrap();
    fs::write(temp.path().join("package.json"), r#"{"name":"kshell-interrupt-probe","version":"1.0.0","private":true,"scripts":{"dev":"kshell-dev.cmd"}}"#).unwrap();
    // Match Next.js' Windows .bin wrapper inside npm's own script shell.
    fs::write(
        temp.path().join("kshell-dev.cmd"),
        b"@echo off\r\nnode parent.cjs\r\n",
    )
    .unwrap();
    fs::write(temp.path().join("parent.cjs"), r#"
        const child = require('node:child_process').spawn(process.execPath, ['worker.cjs'], {stdio:'inherit'});
        process.on('SIGINT', () => {});
        child.on('exit', () => process.exit(130));
    "#).unwrap();
    fs::write(temp.path().join("worker.cjs"), r#"
        const fs = require('node:fs');
        const server = require('node:http').createServer((req,res) => res.end('probe'));
        server.listen(0, '127.0.0.1', () => fs.writeFileSync('server-ready', String(server.address().port)));
        process.on('SIGINT', () => server.close(() => {
            fs.writeFileSync('server-stopped', 'received Ctrl+C and closed listener');
            process.exit(130);
        }));
    "#).unwrap();
    fs::write(temp.path().join("user.npmrc"), b"").unwrap();
    fs::write(temp.path().join("global.npmrc"), b"").unwrap();

    // SAFETY: native structures have their size fields set; all input buffers
    // are owned/NUL-terminated. Unnamed non-inheritable handles are adopted
    // exactly once. Start suspended so the job is assigned BEFORE descendants
    // can be created. Hidden/new console never changes or signals the user UI.
    unsafe {
        let raw_job = CreateJobObjectW(ptr::null(), ptr::null());
        assert!(!raw_job.is_null(), "{}", io::Error::last_os_error());
        let job = OwnedHandle::from_raw_handle(raw_job);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        checked(SetInformationJobObject(
            raw_job,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ))
        .unwrap();

        let exe = std::env::current_exe().unwrap();
        let application = wide(exe.as_os_str());
        let mut command = wide(
            format!(
                "\"{}\" --ignored --exact isolated_console_probe --nocapture",
                exe.display()
            )
            .as_ref(),
        );
        let cwd = wide(temp.path().as_os_str());
        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            dwFlags: STARTF_USESHOWWINDOW,
            wShowWindow: 0,
            ..Default::default()
        };
        let mut info = PROCESS_INFORMATION::default();
        checked(CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            CREATE_NEW_CONSOLE | CREATE_SUSPENDED,
            ptr::null(),
            cwd.as_ptr(),
            &startup,
            &mut info,
        ))
        .unwrap();
        let process = OwnedHandle::from_raw_handle(info.hProcess);
        let primary_thread = OwnedHandle::from_raw_handle(info.hThread);
        if AssignProcessToJobObject(raw_job, info.hProcess) == 0 {
            let error = io::Error::last_os_error();
            TerminateProcess(info.hProcess, 1);
            panic!("cannot contain console probe: {error}");
        }
        assert_ne!(ResumeThread(primary_thread.as_raw_handle()), u32::MAX);
        let wait = WaitForSingleObject(process.as_raw_handle(), 50_000);
        let report = fs::read_to_string(temp.path().join("probe-result.txt")).unwrap_or_default();
        let screen = fs::read_to_string(temp.path().join("probe-screen.txt")).unwrap_or_default();
        assert_eq!(
            wait, WAIT_OBJECT_0,
            "console probe did not finish: {report}"
        );
        let mut status = 1;
        checked(GetExitCodeProcess(process.as_raw_handle(), &mut status)).unwrap();
        assert_eq!(status, 0, "console probe failed: {report}\n{screen}");
        assert_eq!(
            fs::read_to_string(temp.path().join("after-interrupt")).unwrap(),
            "alive"
        );
        assert_eq!(report, "Ok(())");
        drop(job);
    }
}

#[test]
#[ignore = "helper invoked by native_ctrl_c_stops_npm_descendants_and_shell_accepts_another_command"]
fn isolated_console_probe() {
    // Prevent accidental manual --ignored runs from signalling an existing UI.
    if fs::read("console-fixture").ok().as_deref() != Some(b"owned Ctrl+C test console") {
        return;
    }
    let result = probe();
    if result.is_err() {
        let _ = fs::write(
            "probe-screen.txt",
            screen_text().unwrap_or_default().trim_end(),
        );
    }
    let _ = fs::write("probe-result.txt", format!("{result:?}"));
    result.unwrap();
}

fn probe() -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "probe needs real console streams for the interactive REPL",
        ));
    }
    let mut processes = [0u32; 2];
    // SAFETY: writable fixed-size PID buffer. Do not send any signal unless this
    // new console initially contains ONLY this helper process.
    let count = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 2) };
    if count != 1 || processes[0] != std::process::id() {
        return Err(io::Error::other("probe console is not isolated"));
    }
    // SAFETY: only this isolated test process clears an inherited ignore flag.
    unsafe {
        checked(SetConsoleCtrlHandler(None, 0))?;
    }
    let _protect_helper = ShellInterruptGuard::install()?;
    let cwd = std::env::current_dir()?;
    fs::write("legacy-history.txt", b"existing history stays untouched\n")?;
    let mut shell = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        // Test the actual editor, not rustyline's TERM=dumb line-input fallback.
        .env("TERM", "xterm-256color")
        .env("NO_COLOR", "1")
        .env("KSHELL_HISTORY_FILE", "")
        .env("MYSH_HISTORY_FILE", cwd.join("legacy-history.txt"))
        .env_remove("KSHELL_THEME")
        .env("MYSH_THEME", "dusk")
        .env("MYSH_ICONS", "nerd")
        .env("KSHELL_ICONS", "off")
        .env("MYSH_SUGGESTION_STYLE", "bold")
        .env("KSHELL_SUGGESTION_STYLE", "underline")
        .env("npm_config_cache", cwd.join("npm-cache"))
        .env("npm_config_userconfig", cwd.join("user.npmrc"))
        .env("npm_config_globalconfig", cwd.join("global.npmrc"))
        .env("npm_config_update_notifier", "false")
        .env("npm_config_offline", "true")
        .spawn()?;
    type_command("theme\r")?;
    wait_until(
        "legacy settings and new-setting precedence are applied",
        || {
            screen_text().is_ok_and(|s| {
                s.contains("Theme: dusk")
                    && s.contains("Icons: off")
                    && s.contains("Suggestions: underline")
            })
        },
    )?;
    let mut title = [0u16; 512];
    // SAFETY: writable UTF-16 buffer, in this test's verified private console.
    let length = unsafe { GetConsoleTitleW(title.as_mut_ptr(), title.len() as u32) } as usize;
    if String::from_utf16_lossy(&title[..length]) != "K Shell" {
        return Err(io::Error::other(
            "interactive title was not renamed to K Shell",
        ));
    }
    type_command("npm run dev\r")?;
    wait_until("npm server ready", || Path::new("server-ready").is_file())?;
    // SAFETY: this console was checked as private before launching descendants;
    // group zero broadcasts ONLY within that console, never the user/tool host.
    unsafe {
        checked(GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0))?;
    }
    wait_until("grandchild received Ctrl+C and closed listener", || {
        Path::new("server-stopped").is_file()
    })?;
    type_command("node -e 'require(\"fs\").writeFileSync(\"after-interrupt\",\"alive\")'\r")?;
    wait_until("K Shell accepted a command after Ctrl+C", || {
        Path::new("after-interrupt").is_file()
    })?;
    type_command("echo OLD-OUTPUT-TO-CLEAR\r")?;
    wait_until("old terminal text is visible", || {
        screen_text().is_ok_and(|text| text.contains("OLD-OUTPUT-TO-CLEAR"))
    })?;
    type_command("clear\r")?;
    wait_until(
        "clear removed old terminal text and redrew the prompt",
        || {
            screen_text()
                .is_ok_and(|text| !text.contains("OLD-OUTPUT-TO-CLEAR") && text.contains('›'))
        },
    )?;

    type_command("echo OLD-OUTPUT-FOR-CTRL-L\r")?;
    wait_until("text before Ctrl+L is visible", || {
        screen_text().is_ok_and(|text| text.contains("OLD-OUTPUT-FOR-CTRL-L"))
    })?;
    type_command("node -e 'require(\"fs\").writeFileSync(\"after-clear-key\",\"kept\")'")?;
    let control_l = INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: 1,
                wRepeatCount: 1,
                wVirtualKeyCode: 0x4c,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: 12 },
                dwControlKeyState: windows_sys::Win32::System::Console::LEFT_CTRL_PRESSED,
                ..Default::default()
            },
        },
    };
    let mut written = 0;
    // SAFETY: initialized Ctrl+L key event in the verified private console.
    unsafe {
        checked(WriteConsoleInputW(
            GetStdHandle(STD_INPUT_HANDLE),
            &control_l,
            1,
            &mut written,
        ))?;
    }
    wait_until(
        "Ctrl+L cleared old output and preserved the typed command",
        || {
            screen_text().is_ok_and(|text| {
                !text.contains("OLD-OUTPUT-FOR-CTRL-L") && text.contains("after-clear-key")
            })
        },
    )?;
    type_command("\r")?;
    wait_until("command preserved by Ctrl+L executed", || {
        fs::read("after-clear-key").ok().as_deref() == Some(b"kept")
    })?;
    if kshell::platform::nano::default_editor(&cwd).is_some() {
        // Ignore all user configuration. Edit only a new file inside this
        // test-owned cwd, under the existing timeout/descendant cleanup job.
        let prompts_before = screen_text()?.matches('›').count();
        type_command("nano -I -R 'nano fixture.txt'\r")?;
        wait_until("Nano opened the test file", || {
            screen_text().is_ok_and(|s| s.contains("GNU nano") && s.contains("nano fixture.txt"))
        })?;
        type_command("Nano saved this text.")?;
        control_key(u16::from(b'O'))?;
        wait_until("Nano asks for the save filename", || {
            screen_text()
                .is_ok_and(|s| s.contains("File Name to Write") || s.contains("Write to File:"))
        })?;
        type_command("\r")?;
        wait_until("Nano saved the test file", || {
            fs::read("nano fixture.txt").ok().as_deref() == Some(b"Nano saved this text.\n")
        })?;
        control_key(u16::from(b'X'))?;
        // Wait for the new editor prompt, rather than queuing keys while Nano
        // is still restoring the terminal (those keys can be consumed/flushed).
        wait_until("K Shell redrew its prompt after Nano", || {
            screen_text().is_ok_and(|s| s.matches('›').count() > prompts_before)
        })?;
        type_command("touch after-nano\r")?;
        wait_until("K Shell accepted a command after Nano exited", || {
            Path::new("after-nano").is_file()
        })?;
    }
    if fs::read("legacy-history.txt")? != b"existing history stays untouched\n" {
        return Err(io::Error::other(
            "empty KSHELL_HISTORY_FILE did not override legacy persistence",
        ));
    }
    type_command("exit 0\r")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = shell.try_wait()? {
            if status.success() {
                return history_roundtrip_probe();
            }
            return Err(io::Error::other(format!("shell exited with {status}")));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other("shell did not exit on explicit exit"));
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn history_roundtrip_probe() -> io::Result<()> {
    // Continue in the same verified hidden console and test-owned cleanup job.
    // No installed history file or user's Terminal session is accessed here.
    let cwd = std::env::current_dir()?;
    let history_file = cwd.join("saved-history-fixture.txt");
    let mut saved = DefaultHistory::new();
    saved
        .add("echo RESTORED-HISTORY-FIXTURE")
        .map_err(io::Error::other)?;
    saved.save(&history_file).map_err(io::Error::other)?;
    let prompts_before = screen_text()?.matches('›').count();
    let mut shell = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("TERM", "xterm-256color")
        .env("NO_COLOR", "1")
        .env("KSHELL_ICONS", "off")
        .env("KSHELL_HISTORY_FILE", &history_file)
        .spawn()?;
    wait_until("history test shell is ready", || {
        screen_text().is_ok_and(|s| s.matches('›').count() > prompts_before)
    })?;
    type_command("history 10\r")?;
    wait_until(
        "history command displays loaded saved entries and returns to prompt",
        || {
            screen_text().is_ok_and(|s| {
                s.contains("    1  echo RESTORED-HISTORY-FIXTURE")
                    && s.contains("    2  history 10")
                    && s.matches('›').count() > prompts_before + 1
            })
        },
    )?;
    let prompts_before_private = screen_text()?.matches('›').count();
    type_command(" echo PRIVATE-HISTORY-FIXTURE\r")?;
    wait_until("private leading-space command completed", || {
        screen_text().is_ok_and(|s| s.matches('›').count() > prompts_before_private)
    })?;
    if fs::read_to_string(&history_file)?.contains("PRIVATE-HISTORY-FIXTURE") {
        return Err(io::Error::other(
            "ignored leading-space command was persisted",
        ));
    }
    type_command("exit 0\r")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = shell.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "history test shell exited with {status}"
                )))
            };
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other("history test shell did not exit"));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
