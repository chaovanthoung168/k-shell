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
