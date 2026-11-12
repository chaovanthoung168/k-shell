use std::borrow::Cow;
use std::io::{self, Write};
use std::path::PathBuf;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, Editor, EventHandler, Helper, KeyEvent};

use crate::{
    commands::OutputFormat,
    error::ShellError,
    platform,
    runtime::{ShellAction, ShellState},
};

pub mod completion;
mod highlight;
pub mod prompt;
mod screen;
mod validation;
use completion::ShellHelper;

impl Helper for ShellHelper {}
impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let (start, mut candidates) =
            completion::complete(line, pos, &self.cwd, &self.native_commands);
        // Tab and Right must agree about the visible suggestion. In particular,
        // a history hint of `:\` after `cd D` must win over a local `db/` folder.
        // Keep the other candidates so repeated Tab can still cycle alternatives.
        if let Some(hint) = self.hint(line, pos, ctx) {
            let replacement = format!("{}{hint}", &line[start..pos]);
            candidates.retain(|candidate| candidate.replacement != replacement);
            candidates.insert(
                0,
                Pair {
                    display: replacement.clone(),
                    replacement,
                },
            );
        }
        Ok((start, candidates))
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        self.suggestion.borrow_mut().take();
        if line.trim().is_empty() || pos != line.len() || !completion::has_word_at_cursor(line, pos)
        {
            return None;
        }
        let hint = self.history_hinter.hint(line, pos, ctx).or_else(|| {
            let (start, candidates) =
                completion::complete(line, pos, &self.cwd, &self.native_commands);
            let typed = &line[start..];
            candidates.iter().find_map(|candidate| {
                candidate
                    .replacement
                    .strip_prefix(typed)
                    .filter(|suffix| !suffix.is_empty())
                    .map(str::to_owned)
            })
        });
        if self.colors_enabled
            && let Some(suffix) = &hint
        {
            self.suggestion.replace(Some(highlight::Suggestion::new(
                line,
                suffix,
                &self.cwd,
                &self.native_commands,
                &self.feedback,
            )));
        }
        hint
    }
}

impl Highlighter for ShellHelper {
    fn highlight_char(&self, _: &str, _: usize, _: CmdKind) -> bool {
        // Repaint on edits/deletions, cursor changes and final refresh so a
        // suggested command can switch colors immediately in either direction.
        self.colors_enabled
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        highlight::hint(
            hint,
            self.suggestion.borrow().as_ref(),
            self.appearance,
            self.colors_enabled,
        )
    }
}

fn history_path() -> Option<PathBuf> {
    if let Some(path) = crate::startup_setting("HISTORY_FILE") {
        return if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        };
    }
    std::env::var_os("APPDATA")
        .map(|path| PathBuf::from(path).join("mysh/history.txt"))
        .or_else(|| {
            platform::home_directory().map(|path| PathBuf::from(path).join(".mysh/history.txt"))
        })
}

/// Only called for terminal stdin AND stdout. Batch/-c execution never creates
/// an editor, emits escape sequences, or reads/writes command history.
pub fn run(state: &mut ShellState) -> Result<i32, ShellError> {
    let _interrupts = platform::interrupt::ShellInterruptGuard::install().map_err(|error| {
        ShellError::new(format!("cannot handle console interrupts: {error}"), 1)
    })?;
    // The CLI initializes startup appearance, including explicit profile flags.
    let console = platform::console::ConsoleOutput::enable();
    let title = platform::console::ConsoleTitle::new("K Shell")
        .inspect_err(|error| {
            let _ = writeln!(io::stderr(), "kshell: cannot set terminal title: {error}");
        })
        .ok();
    let config = Config::builder()
        .max_history_size(1000)
        .map_err(|error| ShellError::new(format!("editor: {error}"), 1))?
        .history_ignore_space(true)
        .completion_type(CompletionType::Circular)
        .build();
    let mut editor = Editor::<ShellHelper, DefaultHistory>::with_config(config)
        .map_err(|error| ShellError::new(format!("editor: {error}"), 1))?;
    editor.set_helper(Some(ShellHelper::new(state.cwd().to_path_buf())));
    editor.bind_sequence(
        KeyEvent::ctrl('l'),
        EventHandler::Conditional(Box::new(screen::ClearScreenBinding)),
    );
    let mut history = history_path().map(|path| state.cwd().join(path));
    if let Some(path) = &history
        && let Err(error) = editor.load_history(path)
        && !matches!(&error, ReadlineError::Io(error) if error.kind() == io::ErrorKind::NotFound)
    {
        let _ = writeln!(io::stderr(), "kshell: cannot load history: {error}");
        history = None;
    }
    let mut output = io::stdout();
    loop {
        // A foreground program may change the title. Reclaim it at the prompt.
        if let Some(title) = &title {
            let _ = title.refresh();
        }
        if let Some(helper) = editor.helper_mut() {
            helper.set_cwd(state.cwd());
            helper.appearance = state.appearance();
            helper.set_colors_enabled(console.colors_enabled());
        }
        let prompt = prompt::render_with_appearance(
            state.cwd(),
            state.last_status(),
            console.colors_enabled(),
            state.appearance(),
        );
        let line = match editor.readline(&prompt) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                state.last_status = 130;
                continue;
            }
            Err(ReadlineError::Eof) => return Ok(state.last_status()),
            Err(error) => return Err(ShellError::new(format!("editor: {error}"), 1)),
        };
        if let Err(error) = editor.add_history_entry(line.as_str()) {
            let _ = writeln!(io::stderr(), "kshell: cannot update history: {error}");
        }
        state.history.clear();
        state.history.extend(editor.history().iter().cloned());
        if let Some(path) = &history {
            let result = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .map_err(ReadlineError::Io)
                .and_then(|()| editor.append_history(path));
            if let Err(error) = result {
                let _ = writeln!(io::stderr(), "kshell: cannot save history: {error}");
                history = None;
            }
        }
        let format = OutputFormat {
            terminal_width: Some(
                editor
                    .dimensions()
                    .map_or(80, |(columns, _)| columns as usize),
            ),
            color: console.colors_enabled(),
            appearance: state.appearance(),
        };
        let command_cwd = state.cwd().to_path_buf();
        match state.execute_with_format(&line, &mut output, format) {
            Ok(outcome) if outcome.action == ShellAction::Exit => return Ok(outcome.status),
            Ok(outcome) if outcome.action == ShellAction::ClearScreen => {
                output
                    .flush()
                    .map_err(|error| ShellError::new(format!("stdout: {error}"), 1))?;
                let result = match platform::console::clear_screen(&mut output) {
                    Ok(true) => Ok(()),
                    Ok(false) => editor.clear_screen().map_err(io::Error::other),
                    Err(error) => Err(error),
                };
                if let Err(error) = result {
                    state.last_status = 1;
                    let _ = writeln!(io::stderr(), "kshell: clear: {error}");
                }
            }
            Ok(_) => {}
            Err(error) => {
                let _ = writeln!(io::stderr(), "kshell: {error}");
            }
        }
        if let Some(helper) = editor.helper_mut() {
            helper.record_command_result(&command_cwd, &line, state.last_status());
        }
    }
}
