use std::io::{self, Write};
use unicode_width::UnicodeWidthStr;

use super::OutputFormat;
use crate::{error::ShellError, presentation::Style};

struct Entry {
    group: &'static str,
    names: &'static [&'static str],
    usage: &'static str,
    summary: &'static str,
    notes: &'static [&'static str],
    examples: &'static [&'static str],
}

macro_rules! command {
    ($group:literal, [$($name:literal),+], $usage:literal, $summary:literal, [$($note:literal),*], [$($example:literal),*]) => {
        Entry { group: $group, names: &[$($name),+], usage: $usage, summary: $summary, notes: &[$($note),*], examples: &[$($example),*] }
    };
}

const GROUPS: &[(&str, &str)] = &[
    ("navigate", "Navigate"),
    ("files", "Read & write"),
    ("manage", "File actions"),
    ("session", "Session"),
    ("customize", "Customize"),
    ("system", "Computer"),
    ("tools", "Tools"),
];

const ENTRIES: &[Entry] = &[
    command!(
        "tools",
        ["kpm"],
        "kpm install|versions|installed|current|use|upgrade|uninstall|list|search|recover",
        "Install developer tools and switch managed versions.",
        [
            "Interactive install selects one tool and a stable version. Noninteractive: specify tool@version or tool@latest and --yes. Type kpm --help for providers and limitations."
        ],
        ["kpm list", "kpm install node", "kpm use java@21"]
    ),
    command!(
        "tools",
        ["kreq"],
        "kreq <METHOD> <URL> [options] [content|headers]",
        "Send HTTP requests with the native K Shell client.",
        [
            "StatusCode includes its description (200 OK). Select content or headers; -j formats Content. --raw shows only the body. Type kreq --help for all options."
        ],
        [
            "kreq get https://example.com",
            "kreq get https://example.com/api -j"
        ]
    ),
    command!(
        "navigate",
        ["pwd"],
        "pwd",
        "Show your current folder.",
        [],
        ["pwd"]
    ),
    command!(
        "navigate",
        ["cd"],
        "cd [folder]",
        "Change your current folder.",
        ["With no folder, return to your home folder. Quote paths that contain spaces."],
        ["cd Documents", "cd 'My Projects'"]
    ),
    command!(
        "navigate",
        ["ls"],
        "ls [-al] [path ...]",
        "List files and folders.",
        ["-a includes hidden files. -l shows file details instead of columns."],
        ["ls", "ls -la"]
    ),
    command!(
        "navigate",
        ["tree"],
        "tree [-ad] [-L depth] [folder]",
        "Show folders as a tree.",
        ["-a includes hidden entries. -d shows folders only. -L limits depth."],
        ["tree -L 2", "tree -d"]
    ),
    command!(
        "navigate",
        ["mkdir"],
        "mkdir [-p] folder ...",
        "Create folders.",
        ["-p also creates missing parent folders."],
        ["mkdir demo", "mkdir -p demo/assets"]
    ),
    command!(
        "files",
        ["cat"],
        "cat [file ...]",
        "Read file contents.",
        ["With no file, or with -, read standard input."],
        ["cat notes.txt"]
    ),
    command!(
        "files",
        ["create"],
        "create [--] file ...",
        "Create new empty files.",
        ["Existing files are preserved; this command never overwrites them."],
        ["create notes.txt"]
    ),
    command!(
        "files",
        ["touch"],
        "touch file ...",
        "Create files or update their timestamps.",
        [],
        ["touch notes.txt"]
    ),
    command!(
        "files",
        ["write"],
        "write [--] file text ...",
        "Create or replace a text file.",
        ["Replaces existing contents with UTF-8 text and a final newline."],
        ["write notes.txt 'Hello K Shell'"]
    ),
    command!(
        "files",
        ["append"],
        "append [--] file text ...",
        "Add text to the end of a file.",
        ["Creates the file if missing. Adds UTF-8 text and a final newline."],
        ["append notes.txt 'Another line'"]
    ),
    command!(
        "manage",
        ["cp", "copy"],
        "cp [-r] source destination",
        "Copy a file or folder.",
        ["Use -r for folders. An existing destination is never overwritten."],
        ["cp notes.txt backup.txt", "cp -r demo demo-backup"]
    ),
    command!(
        "manage",
        ["mv", "move"],
        "mv source destination",
        "Move a file or folder.",
        ["An existing destination is never overwritten."],
        ["mv notes.txt demo/notes.txt"]
    ),
    command!(
        "manage",
        ["rename", "update"],
        "rename source new-name",
        "Rename an item in the same folder.",
        ["The new name must be a filename, not a path to another folder."],
        ["rename notes.txt ideas.txt"]
    ),
    command!(
        "manage",
        ["rm", "delete"],
        "rm [-rf] path ...",
        "Permanently delete files or folders.",
        [
            "-r removes folders recursively. -f ignores missing paths. Deleted items do not go to the Recycle Bin."
        ],
        ["rm old-notes.txt", "rm -r old-demo"]
    ),
    command!(
        "session",
        ["echo"],
        "echo [text ...]",
        "Print text.",
        [],
        ["echo 'Hello K Shell'"]
    ),
    command!(
        "session",
        ["clear"],
        "clear",
        "Clear the screen and scrollback.",
        ["Ctrl+L also clears while keeping your current input."],
        ["clear"]
    ),
    command!(
        "session",
        ["history"],
        "history [count]",
        "List saved and current-session commands.",
        ["A count limits the list to your most recent commands."],
        ["history 10"]
    ),
    command!(
        "session",
        ["help"],
        "help [topic]",
        "Browse commands and examples.",
        ["Use a group name to focus on one task, or all to see every description."],
        ["help ls", "help files", "help shortcuts"]
    ),
    command!(
        "session",
        ["kshell"],
        "kshell [--welcome]",
        "Show the K Shell welcome banner again.",
        ["Displays the rainbow logo without starting another shell or changing your folder."],
        ["kshell"]
    ),
    command!(
        "session",
        ["exit"],
        "exit [code]",
        "Leave K Shell.",
        ["An optional exit code must be between 0 and 255."],
        ["exit"]
    ),
    command!(
        "customize",
        ["theme"],
        "theme [option]",
        "Choose your prompt and appearance.",
        [
            "Type theme for the picker: Up/Down previews, Enter installs and saves, Esc cancels.",
            "Designs: powerline, atomic, bubblesline, cert, negligible.",
            "theme install <name> installs a design; theme <name> saves it; theme default restores the original prompt.",
            "Session palettes: theme ocean|dusk|midnight. Icons: theme icons unicode|nerd|off. Suggestions: theme suggestions soft|bold|underline.",
            "Nerd icons require an installed Nerd Font. theme status shows your choices; theme reset resets this session."
        ],
        [
            "theme",
            "theme install atomic",
            "theme atomic",
            "theme default"
        ]
    ),
    command!(
        "system",
        ["sysinfo", "specs"],
        "sysinfo",
        "Show your computer's hardware and Windows details.",
        ["Shows OS, user, CPU, GPU, RAM and local drive space."],
        ["sysinfo"]
    ),
    command!(
        "system",
        ["open"],
        "open [--] file",
        "Open a document or media file in its default app.",
        ["Executable and batch files cannot be opened with this command."],
        ["open 'report.pdf'"]
    ),
    command!(
        "tools",
        ["git"],
        "git [arguments]",
        "Work with Git repositories.",
        ["External program: Git must be installed separately."],
        ["git status"]
    ),
    command!(
        "tools",
        ["ssh"],
        "ssh [options] user@host",
        "Connect to a remote computer.",
        ["Uses the native OpenSSH client. Type exit to leave a remote session."],
        ["ssh user@host", "ssh -p 2222 user@host"]
    ),
    command!(
        "tools",
        ["curl"],
        "curl [options] URL",
        "Make requests and download files.",
        ["Uses native curl. Quote URLs containing &."],
        ["curl --version", "curl -o page.html https://example.com"]
    ),
    command!(
        "tools",
        ["nano"],
        "nano [file]",
        "Edit text with GNU nano.",
        ["External editor: Nano must be available. Ctrl+O, then Enter saves; Ctrl+X exits."],
        ["nano notes.txt"]
    ),
    command!(
        "tools",
        ["code"],
        "code [path]",
        "Open a file or folder in VS Code.",
        ["External program: VS Code must be installed separately."],
        ["code ."]
    ),
    command!(
        "tools",
        ["npm"],
        "npm [arguments]",
        "Manage Node.js packages and scripts.",
        ["External program: Node.js and npm must be installed separately."],
        ["npm --version", "npm run dev"]
    ),
    command!(
        "tools",
        ["npx"],
        "npx [arguments]",
        "Run a tool through npm's package runner.",
        ["External program: requires Node.js and npm."],
        ["npx --version"]
    ),
];

pub(super) fn topics() -> impl Iterator<Item = &'static str> {
    ENTRIES
        .iter()
        .flat_map(|entry| entry.names.iter().copied())
        .chain(GROUPS.iter().map(|(key, _)| *key))
        .chain(["all", "shortcuts"])
}

pub(super) fn valid_syntax(args: &[String]) -> bool {
    match args {
        [] => true,
        [topic] => matches!(topic.as_str(), "-h" | "--help") || topics().any(|name| name == topic),
        _ => false,
    }
}

pub(super) fn run(
    args: &[String],
    output: &mut dyn Write,
    format: OutputFormat,
) -> Result<(), ShellError> {
    if !valid_syntax(args) {
        return Err(ShellError::new(
            "usage: help [command|group|all|shortcuts]; type help to browse topics",
            2,
        ));
    }
    let mut view = View {
        output,
        format,
        width: format.terminal_width.unwrap_or(80).max(20),
    };
    let topic = args.first().map(String::as_str).unwrap_or("");
    let result = match topic {
        "" | "-h" | "--help" => overview(&mut view),
        "shortcuts" => shortcuts(&mut view),
        "all" => groups(&mut view, None),
        key if GROUPS.iter().any(|(name, _)| *name == key) => groups(&mut view, Some(key)),
        name => match ENTRIES.iter().find(|entry| entry.names.contains(&name)) {
            Some(entry) => detail(&mut view, entry),
            None => {
                return Err(ShellError::new(
                    "help: unknown topic; type help to browse",
                    2,
                ));
            }
        },
    };
    result.map_err(|error| ShellError::new(format!("help: {error}"), 1))
}

struct View<'a> {
    output: &'a mut dyn Write,
    format: OutputFormat,
    width: usize,
}

impl View<'_> {
    fn blank(&mut self) -> io::Result<()> {
        writeln!(self.output)
    }
    fn text(&mut self, indent: usize, text: &str, style: Style) -> io::Result<()> {
        for line in wrap(text, self.width.saturating_sub(indent).max(1)) {
            writeln!(
                self.output,
                "{}{}",
                " ".repeat(indent),
                self.paint(&line, style)
            )?;
        }
        Ok(())
    }
    fn paint(&self, text: &str, style: Style) -> String {
        style.paint_in(
            text,
            self.format.color && self.format.terminal_width.is_some(),
            self.format.appearance.palette,
        )
    }
    fn heading(&mut self, text: &str) -> io::Result<()> {
        self.text(2, text, Style::Directory)
    }
    fn row(&mut self, label: &str, description: &str, column: usize) -> io::Result<()> {
        if self.width >= 68 && label.len() < column {
            let lines = wrap(description, self.width - column - 2);
            let painted = self.paint(label, Style::Success);
            for (index, line) in lines.iter().enumerate() {
                if index == 0 {
                    writeln!(
                        self.output,
                        "  {painted}{}{line}",
                        " ".repeat(column - label.len())
                    )?;
                } else {
                    writeln!(self.output, "{}{line}", " ".repeat(column + 2))?;
                }
            }
            Ok(())
        } else {
            self.text(2, label, Style::Success)?;
            self.text(4, description, Style::Plain)
        }
    }
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.width() + 1 + word.width() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn overview(view: &mut View<'_>) -> io::Result<()> {
    view.heading("K Shell / Help")?;
    view.text(
        2,
        "help <command> for details and examples. Try: help ls",
        Style::Plain,
    )?;
    view.blank()?;
    for (key, title) in GROUPS {
        let names = ENTRIES
            .iter()
            .filter(|entry| entry.group == *key)
            .map(|entry| entry.names.join("/"))
            .collect::<Vec<_>>()
            .join("  ");
        view.row(title, &names, 15)?;
    }
    view.blank()?;
    view.heading("Try these")?;
    view.row("ls -la", "List files with details", 15)?;
    view.row("theme", "Choose your prompt with Up/Down", 15)?;
    view.blank()?;
    view.text(
        2,
        "Tab: complete | Right: accept | Up/Down: history | Ctrl+L: clear",
        Style::Plain,
    )?;
    view.text(
        2,
        "More: help files | help tools | help shortcuts | help all",
        Style::Muted,
    )?;
    view.text(
        2,
        "Tools use external programs available on your computer.",
        Style::Muted,
    )?;
    view.text(
        2,
        "Pipes, redirection and variable expansion are not supported yet.",
        Style::Muted,
    )
}

fn groups(view: &mut View<'_>, selected: Option<&str>) -> io::Result<()> {
    view.heading("K Shell / Commands")?;
    for (key, title) in GROUPS
        .iter()
        .filter(|(key, _)| selected.is_none_or(|selected| *key == selected))
    {
        view.blank()?;
        view.heading(title)?;
        for entry in ENTRIES.iter().filter(|entry| entry.group == *key) {
            view.row(&entry.names.join(" / "), entry.summary, 21)?;
        }
    }
    view.blank()?;
    view.text(
        2,
        "Use help <command> for syntax and examples.",
        Style::Muted,
    )
}

fn detail(view: &mut View<'_>, entry: &Entry) -> io::Result<()> {
    view.heading(&format!("K Shell / {}", entry.names.join(" / ")))?;
    view.text(2, entry.summary, Style::Plain)?;
    view.blank()?;
    view.heading("Usage")?;
    view.text(4, entry.usage, Style::Success)?;
    for note in entry.notes {
        view.text(4, note, Style::Plain)?;
    }
    view.blank()?;
    view.heading("Examples")?;
    for example in entry.examples {
        view.text(4, example, Style::Success)?;
    }
    view.blank()?;
    view.text(
        2,
        &format!("Related: help {} | help", entry.group),
        Style::Muted,
    )
}

fn shortcuts(view: &mut View<'_>) -> io::Result<()> {
    view.heading("K Shell / Shortcuts")?;
    view.blank()?;
    for (key, description) in [
        (
            "Tab",
            "Complete a command or path; press again to cycle matches",
        ),
        (
            "Right arrow",
            "Accept the visible suggestion at the end of your input",
        ),
        ("Up / Down", "Browse command history"),
        ("Ctrl+L", "Clear the screen and scrollback; keep your input"),
        ("Ctrl+C", "Cancel input or interrupt a foreground program"),
        ("Ctrl+D", "Leave K Shell when the input line is empty"),
        (
            "Theme picker",
            "Up/Down previews, Enter selects, Esc cancels",
        ),
    ] {
        view.row(key, description, 18)?;
    }
    Ok(())
}
