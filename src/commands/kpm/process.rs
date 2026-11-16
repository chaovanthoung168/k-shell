use super::{Result, error, io_error};
use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
pub struct Context {
    generation: Option<usize>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Context {
    pub fn new() -> Self {
        Self {
            generation: Some(crate::platform::interrupt::generation()),
            cancelled: Default::default(),
        }
    }
    pub fn check(&self) -> Result<()> {
        if self.cancelled() {
            Err(error(
                "operation cancelled; run kpm recover if installation had started",
                130,
            ))
        } else {
            Ok(())
        }
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
            || self
                .generation
                .is_some_and(|generation| crate::platform::interrupt::generation() != generation)
    }
    #[cfg(test)]
    pub fn testing() -> Self {
        Self {
            generation: None,
            cancelled: Default::default(),
        }
    }
    #[cfg(test)]
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
#[derive(Debug)]
pub struct Captured {
    pub status: i32,
    pub text: String,
}
fn drain(mut stream: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let keep = n.min((8 * 1024 * 1024usize).saturating_sub(bytes.len()));
        bytes.extend_from_slice(&chunk[..keep]);
    }
    Ok(bytes)
}
pub fn capture(command: &mut Command, timeout: Duration, context: &Context) -> Result<Captured> {
    context.check()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(io_error)?;
    let job = match crate::platform::package_process::Job::attach(&child) {
        Ok(job) => job,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error(format!("cannot contain installer process: {e}"), 1));
        }
    };
    let mut job = Some(job);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| error("missing provider stdout", 1))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| error("missing provider stderr", 1))?;
    let out = thread::spawn(move || drain(stdout));
    let err = thread::spawn(move || drain(stderr));
    let start = Instant::now();
    let mut failure = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break crate::platform::interrupt::exit_code(status),
            Ok(None) => {}
            Err(e) => {
                failure = Some(io_error(e));
                drop(job.take());
                let _ = child.kill();
                let _ = child.wait();
                break 1;
            }
        }
        if context.cancelled() || start.elapsed() > timeout {
            failure = Some(if context.cancelled() {
                error("operation cancelled; run kpm recover before retrying", 130)
            } else {
                error(
                    "provider process timed out; run kpm recover before retrying installation",
                    1,
                )
            });
            drop(job.take());
            let _ = child.kill();
            let _ = child.wait();
            break 1;
        }
        thread::sleep(Duration::from_millis(33));
    };
    drop(job); // Close descendants' inherited pipe writers before joining readers.
    let stdout = out
        .join()
        .map_err(|_| error("provider output reader failed", 1))?
        .map_err(io_error)?;
    let stderr = err
        .join()
        .map_err(|_| error("provider diagnostic reader failed", 1))?
        .map_err(io_error)?;
    if let Some(error) = failure {
        return Err(error);
    }
    if status == 130 {
        return Err(error(
            "provider cancelled; run kpm recover before retrying",
            130,
        ));
    }
    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&stderr));
    Ok(Captured { status, text })
}
pub fn succeeded(result: Captured, label: &str) -> Result<String> {
    if result.status != 0 {
        return Err(error(
            format!(
                "{label} failed with exit status {}. {}",
                result.status,
                safe(&result.text.chars().take(1200).collect::<String>())
            ),
            1,
        ));
    }
    Ok(result.text)
}
pub fn safe(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() && c != '\n' {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
