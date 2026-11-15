use crate::presentation::{Appearance, Icon, Style};
use std::path::Path;

/// Render only the prompt. Shell state and command execution stay independent.
pub fn render(cwd: &Path) -> String {
    render_with_status(cwd, 0, false).0
}

/// Raw/styled prompts have identical visible content, as required by the editor.
/// Keep the path on its own line so a long Windows path does not crowd input.
pub fn render_with_status(cwd: &Path, status: i32, color: bool) -> (String, String) {
    render_with_appearance(cwd, status, color, Appearance::default())
}

pub fn render_with_appearance(
    cwd: &Path,
    status: i32,
    color: bool,
    appearance: Appearance,
) -> (String, String) {
    if appearance.prompt != crate::themes::PromptTheme::Default {
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "user".into());
        let branch = git_branch(cwd);
        return render_design(cwd, &user, branch.as_deref(), status, color, appearance);
    }
    let directory = crate::platform::display_path(cwd);
    let mut path = String::new();
    for character in directory.chars() {
        if character.is_control() {
            path.extend(character.escape_default());
        } else {
            path.push(character);
        }
    }
    let path = appearance.decorate(Icon::Folder, &path, true);
    let marker = "› ";
    let raw = format!("{path}\n{marker}");
    let styled = format!(
        "{}\n{}",
        Style::Directory.paint_in(&path, color, appearance.palette),
        if status == 0 {
            Style::Success
        } else {
            Style::Error
        }
        .paint_in(marker, color, appearance.palette)
    );
    (raw, styled)
}

/// Escape terminal controls in paths, usernames and Git refs before styling.
fn safe(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

/// Read HEAD locally; prompt rendering never spawns Git or scans repository
/// status. Worktrees' .git pointer files and detached HEADs are supported.
fn git_branch(cwd: &Path) -> Option<String> {
    use std::io::Read;
    fn read(path: &Path) -> Option<String> {
        let mut text = String::new();
        std::fs::File::open(path)
            .ok()?
            .take(4096)
            .read_to_string(&mut text)
            .ok()?;
        Some(text)
    }
    for parent in cwd.ancestors() {
        let dot_git = parent.join(".git");
        let git = if dot_git.is_file() {
            let pointer = read(&dot_git)?;
            parent.join(pointer.trim().strip_prefix("gitdir: ")?)
        } else if dot_git.is_dir() {
            dot_git
        } else {
            continue;
        };
        let head = read(&git.join("HEAD"))?;
        let head = head.trim();
        return Some(
            head.strip_prefix("ref: refs/heads/")
                .unwrap_or_else(|| head.get(..8).unwrap_or(head))
                .to_owned(),
        );
    }
    None
}

/// Explicit context makes previews deterministic and keeps rendering separate
/// from discovery. Raw and styled strings always have identical visible text.
pub fn render_design(
    cwd: &Path,
    user: &str,
    branch: Option<&str>,
    status: i32,
    color: bool,
    appearance: Appearance,
) -> (String, String) {
    use crate::themes::PromptTheme;
    let path = safe(&crate::platform::display_path(cwd));
    let user = safe(user);
    let branch = branch.map(safe);
    let marker = if appearance.prompt == PromptTheme::Atomic {
        "╰─❯ "
    } else {
        "› "
    };
    let mut raw = String::new();
    let mut styled = String::new();
    let mut segment = |text: &str, sgr: &str| {
        raw.push_str(text);
        if color {
            styled.push_str(&format!("\x1b[{sgr}m{text}\x1b[0m"));
        } else {
            styled.push_str(text);
        }
    };
    match appearance.prompt {
        PromptTheme::Powerline => {
            let arrow = if appearance.icons == crate::presentation::IconMode::Nerd {
                "\u{e0b0}"
            } else {
                "▶"
            };
            segment(&format!(" {user} "), "38;2;255;255;255;48;2;143;45;151");
            segment(arrow, "38;2;143;45;151;48;2;225;91;139");
            segment(&format!(" {path} "), "38;2;255;255;255;48;2;225;91;139");
            if let Some(branch) = &branch {
                segment(arrow, "38;2;225;91;139;48;2;250;162;123");
                segment(&format!(" git:{branch} "), "38;2;28;28;35;48;2;250;162;123");
            }
            segment(" ▶ K Shell ▶", "38;2;166;219;242");
        }
        PromptTheme::Atomic => {
            segment("╭─", "38;2;35;186;231");
            segment("( K Shell )", "38;2;255;255;255;48;2;0;130;196");
            segment(&format!("( {path} )"), "38;2;35;35;35;48;2;255;206;81");
            if let Some(branch) = &branch {
                segment(&format!("( git:{branch} )"), "38;2;255;224;109");
            }
            segment(&format!(" ( {user} )"), "38;2;128;211;186");
        }
        PromptTheme::Bubblesline => {
            segment(&format!("( {user} ❯ )"), "38;2;183;123;237;48;2;64;64;64");
        }
        PromptTheme::Cert => {
            segment(&format!(" {user} "), "38;2;255;255;255;48;2;226;100;115");
            segment("░", "38;2;226;100;115");
            segment(&format!(" {path} "), "38;2;255;255;255;48;2;34;181;77");
            segment("░", "38;2;34;181;77");
            if let Some(branch) = &branch {
                segment(
                    &format!(" git({branch}) "),
                    "38;2;255;255;255;48;2;75;179;196",
                );
            }
        }
        PromptTheme::Negligible => {
            segment(&path, "38;2;0;183;232");
            if let Some(branch) = &branch {
                segment(" :: ", "38;2;231;82;58");
                segment(&format!("git:{branch}"), "38;2;231;82;58");
            }
        }
        PromptTheme::Default => {
            return render_with_appearance(
                cwd,
                status,
                color,
                Appearance {
                    prompt: PromptTheme::Default,
                    ..appearance
                },
            );
        }
    }
    segment("\n", "0");
    raw.push_str(marker);
    styled.push_str(
        &if status == 0 {
            Style::Success
        } else {
            Style::Error
        }
        .paint_in(marker, color, appearance.palette),
    );
    (raw, styled)
}
