use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{
    commands::{self, OutputFormat},
    error::ShellError,
    language::parser,
    platform,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellAction {
    Continue,
    Exit,
    ClearScreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub status: i32,
    pub action: ShellAction,
}

#[derive(Debug)]
pub struct ShellState {
    pub(crate) cwd: PathBuf,
    pub(crate) last_status: i32,
    pub(crate) appearance: crate::presentation::Appearance,
    /// Snapshot of the bounded editor history, populated only by the interactive
    /// host. Batch execution never reads or writes the user's history file.
    pub(crate) history: Vec<String>,
}

impl ShellState {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            last_status: 0,
            appearance: Default::default(),
            history: Vec::new(),
        }
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }
    pub fn with_appearance(mut self, appearance: crate::presentation::Appearance) -> Self {
        self.appearance = appearance;
        self
    }
    pub fn last_status(&self) -> i32 {
        self.last_status
    }
    pub fn appearance(&self) -> crate::presentation::Appearance {
        self.appearance
    }

    pub fn execute(&mut self, input: &str, output: &mut dyn Write) -> Result<Outcome, ShellError> {
        self.execute_with_format(input, output, OutputFormat::default())
    }

    pub fn execute_with_format(
        &mut self,
        input: &str,
        output: &mut dyn Write,
        format: OutputFormat,
    ) -> Result<Outcome, ShellError> {
        let result = self.execute_inner(input, output, format);
        self.last_status = match &result {
            Ok(outcome) => outcome.status,
            Err(error) => error.status,
        };
        result
    }

    fn execute_inner(
        &mut self,
        input: &str,
        output: &mut dyn Write,
        format: OutputFormat,
    ) -> Result<Outcome, ShellError> {
        let Some(command) = parser::parse(input)? else {
            return Ok(Outcome {
                status: self.last_status,
                action: ShellAction::Continue,
            });
        };
        let args: Vec<String> = command.words.iter().map(|word| word.literal()).collect();
        if let Some(outcome) = commands::run(&args, self, output, format)? {
            return Ok(outcome);
        }
        let mut process = match platform::resolve_executable(&args[0], &self.cwd) {
            Ok(executable) => {
                let mut process = Command::new(executable);
                process.args(&args[1..]).current_dir(&self.cwd);
                process
            }
            Err(error)
                if cfg!(windows)
                    && error.status == 127
                    && platform::npm::PackageTool::parse(&args[0]).is_some() =>
            {
                // The guard recognizes only bare npm/npx; native executables
                // and explicit-path errors retain normal lookup precedence.
                let tool = platform::npm::PackageTool::parse(&args[0])
                    .ok_or_else(|| ShellError::new("unrecognized Node.js package tool", 127))?;
                let launcher = platform::npm::PackageLauncher::find(tool, &self.cwd)?;
                let arguments: Vec<std::ffi::OsString> =
                    args[1..].iter().map(std::ffi::OsString::from).collect();
                launcher.command(&arguments, &self.cwd)?
            }
            Err(error)
                if cfg!(windows) && error.status == 127 && args[0].eq_ignore_ascii_case("code") =>
            {
                let launcher = platform::vscode::CodeLauncher::find(&self.cwd)?;
                let arguments: Vec<std::ffi::OsString> =
                    args[1..].iter().map(std::ffi::OsString::from).collect();
                launcher.command(&arguments, &self.cwd)
            }
            Err(error) => return Err(error),
        };
        output
            .flush()
            .map_err(|error| ShellError::new(format!("stdout: {error}"), 1))?;
        let status = process
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|error| {
                ShellError::new(format!("cannot execute {}: {error}", args[0]), 126)
            })?;
        Ok(Outcome {
            status: platform::interrupt::exit_code(status),
            action: ShellAction::Continue,
        })
    }
}
