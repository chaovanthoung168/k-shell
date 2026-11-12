use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rustyline::completion::Pair;

use crate::{
    commands::COMMAND_NAMES,
    language::lexer::{TokenKind, lex_incomplete},
};

/// A grey hint needs text to extend. Explicit Tab may still offer candidates
/// for an empty argument, but merely completing a command must not add a path.
pub(crate) fn has_word_at_cursor(line: &str, pos: usize) -> bool {
    let Some(before_cursor) = line.get(..pos) else {
        return false;
    };
    lex_incomplete(before_cursor)
        .last()
        .is_some_and(|token| token.span.end == pos && matches!(token.kind, TokenKind::Word(_)))
}

/// One completion replaces an entire lexer word, including any quote fragments.
/// This shares the language's Windows backslash and quote rules.
pub fn complete(
    line: &str,
    pos: usize,
    cwd: &Path,
    native_commands: &[String],
) -> (usize, Vec<Pair>) {
    if !line.is_char_boundary(pos) {
        return (pos, Vec::new());
    }
    let tokens = lex_incomplete(&line[..pos]);
    if tokens
        .iter()
        .any(|token| matches!(token.kind, TokenKind::Operator(_)))
    {
        return (pos, Vec::new());
    }
    let (start, prefix, command_position) = match tokens.last() {
        Some(token) if token.span.end == pos => {
            let TokenKind::Word(word) = &token.kind else {
                return (pos, Vec::new());
            };
            (word.span.start, word.literal(), tokens.len() == 1)
        }
        _ => (pos, String::new(), tokens.is_empty()),
    };
    if !command_position
        && tokens.first().is_some_and(
            |token| matches!(&token.kind, TokenKind::Word(word) if word.literal() == "theme"),
        )
    {
        let argument = tokens
            .iter()
            .skip(1)
            .filter(|token| token.span.end <= start)
            .count();
        let choices: &[&str] = if argument == 0 {
            &["ocean", "dusk", "midnight", "icons", "suggestions", "reset"]
        } else if argument == 1
            && tokens.get(1).is_some_and(
                |token| matches!(&token.kind, TokenKind::Word(word) if word.literal() == "icons"),
            )
        {
            &["unicode", "nerd", "off"]
        } else if argument == 1 && tokens.get(1).is_some_and(
            |token| matches!(&token.kind, TokenKind::Word(word) if word.literal() == "suggestions"),
        ) {
            &["soft", "bold", "underline"]
        } else {
            &[]
        };
        return (
            start,
            choices
                .iter()
                .filter(|name| starts_with(name, &prefix))
                .map(|name| Pair {
                    display: (*name).to_owned(),
                    replacement: format!("{name} "),
                })
                .collect(),
        );
    }
    if !command_position && tokens.first().is_some_and(|token| {
        matches!(&token.kind, TokenKind::Word(word) if matches!(word.literal().as_str(), "clear" | "pwd" | "help" | "history" | "exit" | "sysinfo" | "specs"))
    }) {
        return (start, Vec::new());
    }
    if !command_position && tokens.first().is_some_and(|token| {
        matches!(&token.kind, TokenKind::Word(word) if matches!(word.literal().as_str(), "write" | "append"))
    }) {
        let before: Vec<_> = tokens.iter().skip(1).filter(|t| t.span.end <= start).collect();
        let marker = before.first().is_some_and(|t| matches!(&t.kind, TokenKind::Word(w) if w.literal() == "--"));
        if before.len() > usize::from(marker) {
            return (start, Vec::new()); // Content is text, not a filename.
        }
    }
    let directories_only = !command_position
        && tokens.first().is_some_and(
            |token| matches!(&token.kind, TokenKind::Word(word) if matches!(word.literal().as_str(), "cd" | "mkdir" | "tree")),
        );
    // `D:` is a drive prefix, not a filename prefix in the current directory.
    // Completing it inserts the explicit root separator; no drive-local current
    // directory or global process cwd is changed by completion.
    if cfg!(windows)
        && prefix.len() == 2
        && prefix.as_bytes()[0].is_ascii_alphabetic()
        && prefix.as_bytes()[1] == b':'
    {
        let root = format!("{prefix}\\");
        let candidates = if Path::new(&root).is_dir() {
            vec![Pair {
                display: root.clone(),
                replacement: root,
            }]
        } else {
            Vec::new()
        };
        return (start, candidates);
    }
    let mut candidates = BTreeSet::new();
    if command_position && !prefix.contains(['/', '\\']) {
        for name in COMMAND_NAMES {
            if starts_with(name, &prefix) {
                candidates.insert((0, format!("{} ", quote_word(name))));
            }
        }
        for name in native_commands {
            if starts_with(name, &prefix) {
                candidates.insert((1, format!("{} ", quote_word(name))));
            }
        }
    }
    let split = prefix
        .rfind(|c| c == '/' || (cfg!(windows) && c == '\\'))
        .map_or(0, |index| index + 1);
    let (parent, filename) = prefix.split_at(split);
    let separator = if cfg!(windows) && parent.ends_with('\\') {
        '\\'
    } else {
        '/'
    };
    let directory = cwd.join(parent);
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            // Unrepresentable names are skipped, never converted into a different path.
            let Some(name) = name.to_str() else {
                continue;
            };
            if !starts_with(name, filename) || (name.starts_with('.') && !filename.starts_with('.'))
            {
                continue;
            }
            let is_directory = entry.path().is_dir();
            if directories_only && !is_directory {
                continue;
            }
            let mut value = format!("{parent}{name}");
            if is_directory {
                value.push(separator);
            }
            candidates.insert((2, quote_word(&value)));
        }
    }
    let mut seen = BTreeSet::new();
    (
        start,
        candidates
            .into_iter()
            .map(|(_, replacement)| replacement)
            .filter(|replacement| seen.insert(replacement.clone()))
            .map(|replacement| Pair {
                display: replacement.clone(),
                replacement,
            })
            .collect(),
    )
}

fn starts_with(value: &str, prefix: &str) -> bool {
    if cfg!(windows) {
        value.to_lowercase().starts_with(&prefix.to_lowercase())
    } else {
        value.starts_with(prefix)
    }
}

fn quote_word(value: &str) -> String {
    if value
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '|' | '&' | ';' | '<' | '>'))
    {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    } else {
        value.to_owned()
    }
}

/// Snapshot native command names once at startup (and after cd), rather than
/// scanning PATH on every keypress. Actual execution still resolves each command.
pub fn native_commands(cwd: &Path) -> Vec<String> {
    let mut names = BTreeSet::new();
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let Ok(entries) = std::fs::read_dir(cwd.join(directory)) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.path().is_file() {
                    continue;
                }
                let path = entry.path();
                #[cfg(windows)]
                {
                    let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
                        continue;
                    };
                    if !extension.eq_ignore_ascii_case("exe")
                        && !extension.eq_ignore_ascii_case("com")
                    {
                        continue;
                    }
                    // Explicit extensions remain usable regardless of PATHEXT.
                    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                        names.insert(name.to_owned());
                    }
                    let extensions =
                        std::env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE".into());
                    if extensions
                        .to_string_lossy()
                        .split(';')
                        .any(|ext| ext.eq_ignore_ascii_case(&format!(".{extension}")))
                        && let Some(stem) = path.file_stem().and_then(|name| name.to_str())
                        && !stem.contains('.')
                    {
                        names.insert(stem.to_owned());
                    }
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if path
                        .metadata()
                        .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
                        && let Some(name) = path.file_name().and_then(|name| name.to_str())
                    {
                        names.insert(name.to_owned());
                    }
                }
            }
        }
    }
    #[cfg(windows)]
    for name in ["npm", "npx"] {
        if let Some(tool) = crate::platform::npm::PackageTool::parse(name)
            && crate::platform::npm::PackageLauncher::find(tool, cwd).is_ok()
        {
            names.insert(name.to_owned());
        }
    }
    #[cfg(windows)]
    if crate::platform::vscode::CodeLauncher::find(cwd).is_ok() {
        names.insert("code".to_owned());
    }
    #[cfg(windows)]
    if crate::platform::curl::default_client().is_some() {
        names.insert("curl".to_owned());
        names.insert("curl.exe".to_owned());
    }
    #[cfg(windows)]
    if crate::platform::ssh::default_client().is_some() {
        names.insert("ssh".to_owned());
        names.insert("ssh.exe".to_owned());
    }
    #[cfg(windows)]
    if crate::platform::nano::default_editor(cwd).is_some() {
        names.insert("nano".to_owned());
        names.insert("nano.exe".to_owned());
    }
    names.into_iter().collect()
}

pub struct ShellHelper {
    pub(crate) cwd: PathBuf,
    pub(crate) native_commands: Vec<String>,
    pub(crate) history_hinter: rustyline::hint::HistoryHinter,
    pub(crate) appearance: crate::presentation::Appearance,
    pub(crate) colors_enabled: bool,
    pub(super) suggestion: std::cell::RefCell<Option<super::highlight::Suggestion>>,
    pub(super) feedback: super::validation::Feedback,
}

impl ShellHelper {
    pub fn new(cwd: PathBuf) -> Self {
        let native_commands = native_commands(&cwd);
        Self {
            cwd,
            native_commands,
            history_hinter: rustyline::hint::HistoryHinter::new(),
            appearance: Default::default(),
            colors_enabled: false,
            suggestion: Default::default(),
            feedback: Default::default(),
        }
    }

    pub fn set_cwd(&mut self, cwd: &Path) {
        if self.cwd != cwd {
            self.suggestion.borrow_mut().take();
            self.native_commands = native_commands(cwd);
            self.cwd = cwd.to_path_buf();
        }
    }

    /// The interactive host supplies actual terminal/NO_COLOR capabilities.
    pub fn set_colors_enabled(&mut self, enabled: bool) {
        self.colors_enabled = enabled;
    }

    /// Remember actual outcomes without replaying commands to validate hints.
    pub fn record_command_result(&mut self, cwd: &Path, line: &str, status: i32) {
        self.feedback.record(cwd, line, status);
        self.suggestion.borrow_mut().take();
    }
}
