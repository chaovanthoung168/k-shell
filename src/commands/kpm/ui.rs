use super::{Result, error, io_error, process};
use crate::{
    commands::OutputFormat,
    platform::menu::{self, MenuKey},
};
use std::{
    io::{self, IsTerminal, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
pub struct Ui {
    pub interactive: bool,
    pub width: usize,
    pub color: bool,
}
impl Ui {
    pub fn new(format: OutputFormat) -> Self {
        Self {
            interactive: format.terminal_width.is_some()
                && io::stdin().is_terminal()
                && io::stderr().is_terminal()
                && crate::platform::console::ConsoleOutput::enable().supports_ansi(),
            width: format.terminal_width.unwrap_or(80),
            color: format.color,
        }
    }
    pub fn choose(&self, title: &str, choices: &[String], default: usize) -> Result<Option<usize>> {
        if !self.interactive {
            return Err(error(
                "interactive selection requires a Windows terminal; specify tool@version and --yes",
                2,
            ));
        }
        if choices.is_empty() {
            return Err(error("no choices available", 1));
        }
        let mut selected = default.min(choices.len() - 1);
        let mut output = io::stderr();
        let _cursor = Cursor::hide(true)?;
        loop {
            let rows = choices.len() + 2;
            writeln!(output, "\r\x1b[2K{}", clip(title, self.width)).map_err(io_error)?;
            for (i, choice) in choices.iter().enumerate() {
                writeln!(
                    output,
                    "\r\x1b[2K{}",
                    clip(
                        &format!("{} {choice}", if i == selected { ">" } else { " " }),
                        self.width
                    )
                )
                .map_err(io_error)?;
            }
            writeln!(
                output,
                "\r\x1b[2K{}",
                clip(
                    "Up/Down: navigate | Enter: select | Esc/Ctrl+C: cancel",
                    self.width
                )
            )
            .map_err(io_error)?;
            output.flush().map_err(io_error)?;
            match menu::read_key().map_err(io_error)? {
                MenuKey::Accept => return Ok(Some(selected)),
                MenuKey::Cancel => return Ok(None),
                MenuKey::Up => selected = (selected + choices.len() - 1) % choices.len(),
                MenuKey::Down => selected = (selected + 1) % choices.len(),
            }
            write!(output, "\x1b[{rows}A\r").map_err(io_error)?;
        }
    }
    pub fn confirm(&self, prompt: &str, yes: bool) -> Result<bool> {
        if yes {
            return Ok(true);
        }
        if !self.interactive {
            return Err(error(
                "confirmation requires a terminal; pass --yes to authorize this operation",
                2,
            ));
        }
        Ok(self.choose(prompt, &["Confirm".into(), "Cancel".into()], 1)? == Some(0))
    }
    pub fn manual(&self) -> Result<Option<String>> {
        eprint!("Stable version (empty to cancel): ");
        io::stderr().flush().map_err(io_error)?;
        let generation = crate::platform::interrupt::generation();
        let mut line = String::new();
        io::stdin().read_line(&mut line).map_err(io_error)?;
        if generation != crate::platform::interrupt::generation() {
            return Err(error("selection cancelled", 130));
        }
        let version = line.trim().to_owned();
        Ok((!version.is_empty()).then_some(version))
    }
}
fn clip(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut cells = 0;
    for c in process::safe(text).replace('\n', "\\n").chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if cells + w >= width {
            break;
        }
        out.push(c);
        cells += w;
    }
    out
}
pub(super) struct Cursor<W: Write>(bool, W);
impl Cursor<io::Stderr> {
    fn hide(enabled: bool) -> Result<Self> {
        Self::new(enabled, io::stderr())
    }
}
impl<W: Write> Cursor<W> {
    pub(super) fn new(enabled: bool, mut output: W) -> Result<Self> {
        if enabled {
            output.write_all(b"\x1b[?25l").map_err(io_error)?;
        }
        Ok(Self(enabled, output))
    }
}
impl<W: Write> Drop for Cursor<W> {
    fn drop(&mut self) {
        if self.0 {
            let _ = self.1.write_all(b"\x1b[?25h");
        }
    }
}
pub fn frame(width: usize, tick: usize, color: bool) -> String {
    if width < 3 {
        let mut cells = vec![' '; width];
        if width > 0 {
            cells[tick % width] = '●';
        }
        return cells.into_iter().collect();
    }
    let span = width.saturating_sub(2).clamp(1, 50);
    let pos = tick % span;
    let mut bar = String::from("[");
    for i in 0..span {
        if i == pos {
            if color {
                bar.push_str("\x1b[36m●\x1b[0m");
            } else {
                bar.push('●');
            }
        } else {
            bar.push('─');
        }
    }
    bar.push(']');
    bar
}
pub struct Progress {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Progress {
    pub fn start(ui: &Ui, label: &str) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker = if ui.interactive {
            let done = stop.clone();
            let width = ui.width;
            let color = ui.color;
            let label = clip(label, width.saturating_sub(12));
            let fps = std::env::var("KSHELL_KPM_FPS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(30)
                .clamp(5, 60);
            Some(thread::spawn(move || {
                let _cursor = Cursor::hide(true).ok();
                let mut tick = 0;
                while !done.load(Ordering::Relaxed) {
                    let mut output = io::stderr();
                    let _ = write!(
                        output,
                        "\r\x1b[2K{label} {}",
                        frame(width.saturating_sub(label.chars().count() + 2), tick, color)
                    );
                    let _ = output.flush();
                    tick += 1;
                    thread::sleep(Duration::from_millis(1000 / fps));
                }
                let _ = io::stderr().write_all(b"\r\x1b[2K");
            }))
        } else {
            eprintln!("{label}");
            None
        };
        Self { stop, worker }
    }
}
impl Drop for Progress {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
